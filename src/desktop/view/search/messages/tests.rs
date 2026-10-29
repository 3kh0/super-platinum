use super::*;
use dioxus::core::{AttributeValue, Mutation};
use dioxus::core::{ElementId, ScopeId};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

#[test]
fn generated_page_controls_clamp_five_numbers_and_keep_failed_target_distinct_from_shown_page() {
    assert_eq!(range(1, 2), vec![1, 2]);
    assert_eq!(range(1, 9), vec![1, 2, 3, 4, 5]);
    assert_eq!(range(4, 9), vec![2, 3, 4, 5, 6]);
    assert_eq!(range(8, 9), vec![5, 6, 7, 8, 9]);
    assert_eq!(range(9, 9), vec![5, 6, 7, 8, 9]);
    assert_eq!(
        range(u32::MAX, u32::MAX),
        ((u32::MAX - 4)..=u32::MAX).collect::<Vec<_>>()
    );
    assert!(range(0, 9).is_empty() && range(10, 9).is_empty());
    for name in [
        "search-messages-pages",
        "search-messages-pages-narrow",
        "search-messages-page2",
        "search-messages-page2-narrow",
        "search-messages-page-failed",
        "search-messages-page-failed-narrow",
    ] {
        let mut dom = VirtualDom::new_with_props(
            |name: &'static str| {
                let state = use_signal(|| {
                    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
                    pages::fixture(&mut shell, name);
                    shell
                });
                controls(state, &state.read())
            },
            name,
        );
        let edits = dom.rebuild_to_vec();
        let labels = edits
            .edits
            .iter()
            .filter_map(|m| match m {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    ..
                } => Some(value.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for label in ["Page 1", "Page 2", "Page 3", "Page 4", "Page 5"] {
            assert!(labels.contains(&label), "{name}: {label}");
        }
        assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"aria-current",value:AttributeValue::Text(value),..} if value=="page")));
        if name.contains("failed") {
            let text = format!("{edits:?}");
            assert!(
                text.contains("Page 4")
                    && text.contains("Showing page 1 of 9")
                    && text.contains("Retry page")
            );
        }
    }
}

type Attributes = BTreeMap<(ElementId, String), String>;
fn record(attributes: &mut Attributes, edits: Vec<Mutation>) {
    for edit in edits {
        if let Mutation::SetAttribute {
            id, name, value, ..
        } = edit
        {
            let value = match value {
                AttributeValue::Text(s) => s,
                AttributeValue::Bool(b) => b.to_string(),
                _ => continue,
            };
            attributes.insert((id, name.to_owned()), value);
        }
    }
}
fn element(attributes: &Attributes, name: &str, value: &str) -> ElementId {
    attributes
        .iter()
        .find_map(|((id, key), v)| (key == name && v == value).then_some(*id))
        .unwrap()
}
fn click(dom: &VirtualDom, id: ElementId) {
    let payload = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.in_scope(ScopeId::ROOT, || {
        dom.runtime().handle_event(
            "click",
            dioxus::core::Event::new(payload.into_any(), true),
            id,
        )
    });
}
async fn pump(dom: &mut VirtualDom, attributes: &mut Attributes, ready: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            record(attributes, dom.render_immediate_to_vec().edits);
            if ready() {
                break;
            }
            dom.wait_for_work().await;
        }
    })
    .await
    .expect("synthetic page control must execute and settle");
}

#[tokio::test]
async fn actual_enabled_page_buttons_dispatch_next_cached_previous_failed_fourth_retry_and_refuse_pending_overlap()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = pages::tests::loaded_harness().await;
    let mut old = h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            controls(state, &state.read())
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    let state = slot.get().unwrap();
    let io = Rc::new(pages::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let next = element(&attributes, "id", NEXT_ID);
    let previous = element(&attributes, "id", PREVIOUS_ID);
    assert_eq!(attributes[&(next, "disabled".into())], "false");
    assert_eq!(attributes[&(previous, "disabled".into())], "true");
    let reply = io.expect(2);
    click(&dom, next);
    assert!(state.peek().search_pager.loading);
    let revision = state.peek().search_pager.revision();
    click(&dom, next);
    assert_eq!(
        state.peek().search_pager.revision(),
        revision,
        "old or pending handler cannot admit another read"
    );
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 1).await;
    assert_eq!(attributes[&(next, "disabled".into())], "true");
    reply
        .send(Ok(
            json!({"ok":true,"items":[],"pagination":{"page":2,"page_count":2,"total_count":88}}),
        ))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(
        pages::status(&state.peek()),
        Some((2, 2, 7, 88, false, false))
    );
    assert_eq!(attributes[&(previous, "disabled".into())], "false");
    click(&dom, previous);
    assert_eq!(
        pages::status(&state.peek()),
        Some((1, 1, 7, 88, false, false))
    );
    record(&mut attributes, dom.render_immediate_to_vec().edits);
    assert_eq!(&*io.pages.borrow(), &[2]);
    assert_eq!(io.lookups.get(), 0);

    let fourth = element(&attributes, "aria-label", "Page 4");
    let reply = io.expect(4);
    click(&dom, fourth);
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 2).await;
    reply
        .send(Err(super_platinum_core::slack::Error::Offline(
            "synthetic outage".into(),
        )))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        state.peek().search_pager.failed
    })
    .await;
    assert_eq!(
        pages::status(&state.peek()),
        Some((1, 4, 7, 88, false, true))
    );
    let retry = element(&attributes, "id", RETRY_ID);
    assert_eq!(attributes[&(retry, "disabled".into())], "false");
    let reply = io.expect(4);
    click(&dom, retry);
    assert!(state.peek().search_pager.loading);
    pump(&mut dom, &mut attributes, || io.pages.borrow().len() == 3).await;
    reply
        .send(Ok(
            json!({"ok":true,"items":[],"pagination":{"page":4,"page_count":1,"total_count":0}}),
        ))
        .unwrap();
    pump(&mut dom, &mut attributes, || {
        !state.peek().search_pager.loading
    })
    .await;
    assert_eq!(
        pages::status(&state.peek()),
        Some((4, 4, 7, 0, false, false))
    );
    assert_eq!(&*io.pages.borrow(), &[2, 4, 4]);
    assert_eq!(io.lookups.get(), 0);

    let mut fallback = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::bootstrap::search_fixture(&mut fallback, "search");
    assert!(!owns(&fallback));
    assert!(!pages::has_scope(&fallback));
    let mut legacy = VirtualDom::new_with_props(
        |input: Input| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            controls(state, &state.read())
        },
        Rc::new(RefCell::new(Some(fallback))),
    );
    let edits = legacy.rebuild_to_vec();
    assert!(!edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"id",value:AttributeValue::Text(value),..} if [PREVIOUS_ID,NEXT_ID,RETRY_ID].contains(&value.as_str()))));
}
