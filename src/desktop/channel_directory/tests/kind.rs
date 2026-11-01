//! Actual new select handler and the original read controller, no OS/native input.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode};

fn select_listener(v: &VNode) -> AttributeValue {
    fn find(v: &VNode, n: &TemplateNode) -> Option<AttributeValue> {
        match n {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                let mut matched = false;
                let mut listener = None;
                for a in attrs.iter() {
                    match a {
                        TemplateAttribute::Static {
                            name: "id",
                            value: "your-channels-type",
                            ..
                        } => matched = true,
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                if a.name == "id"
                                    && matches!(&a.value, AttributeValue::Text(s) if s == "your-channels-type")
                                {
                                    matched = true;
                                }
                                if a.name == "onchange" {
                                    listener = Some(a.value.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if matched {
                    return listener;
                }
                children.iter().find_map(|n| find(v, n))
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Fragment(nodes) => nodes
                    .iter()
                    .find_map(|v| v.template.roots.iter().find_map(|n| find(v, n))),
                _ => None,
            },
            _ => None,
        }
    }
    v.template
        .roots
        .iter()
        .find_map(|n| find(v, n))
        .expect("actual type select callback")
}
fn change(dom: &VirtualDom, listener: AttributeValue, value: &str) {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let AttributeValue::Listener(listener) = listener else {
        panic!("actual listener")
    };
    let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.in_scope(ScopeId::ROOT, || {
        listener.call(dioxus::core::Event::new(payload.into_any(), true))
    });
}

#[tokio::test(flavor = "current_thread")]
async fn actual_type_selection_retires_old_results_and_retries_only_its_joined_page() {
    let (mut dom, mut state, captured) = mount("your-channels", render);
    let entry = enter(state);
    entry.change(state, Some("release".into()), Some(Sort::MostMembers));
    publish(&entry, state, read::Mode::Search, 1).await;
    dom.render_immediate_to_vec();
    let conversation = (
        state.peek().core.active_channel.clone(),
        state.peek().channel_generation,
        state.peek().core.composer.text.clone(),
    );
    let native_channels =
        serde_json::to_value(&state.peek().core.workspaces["T1"].channels).unwrap();
    let old_row = button(captured.borrow().as_ref().unwrap(), "your-channel-C1").unwrap();
    let old_page = state.peek().channel_directory.loaded.clone().unwrap();
    let (retired, retired_lease) = entry.begin(state, read::Mode::Refresh, None).unwrap();
    let mut last_listener = None;
    for (value, kind) in [
        ("public", Kind::Public),
        ("private", Kind::Private),
        ("archived", Kind::Archived),
    ] {
        dom.render_immediate_to_vec();
        let listener = select_listener(captured.borrow().as_ref().unwrap());
        last_listener = Some(listener.clone());
        change(&dom, listener, value);
        assert!(state.peek().channel_directory.kind == kind);
        assert!(
            state.peek().channel_directory.busy(),
            "handler reserves its page1 before spawn"
        );
        assert!(!old_page.current(&state.peek()));
        assert!(!retired.current());
        settle(&mut dom, || !state.peek().channel_directory.busy()).await;
        let failed = state.peek().channel_directory.failed.clone().unwrap();
        assert!(failed.kind == kind && failed.scope == Scope::Joined);
        assert_eq!(failed.text, "release");
        assert!(failed.sort == Sort::MostMembers);
        assert_eq!((failed.page, failed.cursor), (1, None));
        assert!(
            state.peek().channel_directory.loaded.is_some(),
            "old metadata survives error without acquiring currentness"
        );
        let (request, lease) = entry.begin(state, read::Mode::Retry, None).unwrap();
        Entry::run_with(request, lease, |request| async move {
            assert!(request.current() && request.query().kind == kind);
            Ok(Some(api::decode(raw(1), request.query()).unwrap()))
        })
        .await;
    }
    // Cleanup of an older refresh cannot retire the accepted Archived result.
    drop(retired_lease);
    click(&dom, old_row);
    assert!(state.peek().channel_directory.selected.is_none());
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-next").unwrap(),
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert!(failed.kind == Kind::Archived && failed.scope == Scope::Joined);
    assert_eq!(
        (failed.page, failed.cursor.as_deref()),
        (2, Some("next-page"))
    );
    publish(&entry, state, read::Mode::Retry, 2).await;
    assert!(
        state
            .peek()
            .channel_directory
            .loaded
            .as_ref()
            .unwrap()
            .current(&state.peek())
    );
    assert_eq!(
        (
            state.peek().core.active_channel.clone(),
            state.peek().channel_generation,
            state.peek().core.composer.text.clone()
        ),
        conversation
    );
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces["T1"].channels).unwrap(),
        native_channels
    );
    entry.close(state);
    state.write().core.active_account = Some("successor-account".into());
    let successor = enter(state);
    let revision = state.peek().channel_directory.revision;
    change(&dom, last_listener.unwrap(), "public");
    assert!(state.peek().channel_directory.kind == Kind::Active);
    assert_eq!(state.peek().channel_directory.revision, revision);
    assert!(!state.peek().channel_directory.busy());
    assert!(successor.current(&state.peek()));
}
