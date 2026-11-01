//! Actual owned getView + mounted New/read/append/refresh journey.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};

fn cues(v: &VNode) -> Vec<(String, bool)> {
    fn at(v: &VNode, node: &TemplateNode, inside: bool, out: &mut Vec<(String, bool)>) {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut before = None;
                let mut ghost = false;
                let mut marker = false;
                let mut listeners = 0;
                for a in attrs.iter() {
                    let mut value = |name: &str, text: &str| {
                        if name == "class" {
                            marker |= text.split_whitespace().any(|s| s == "thread-feed-new");
                            ghost |= text.split_whitespace().any(|s| s == "ghost");
                        }
                        if name == "data-thread-unread-before" {
                            before = Some(text.to_owned());
                        }
                    };
                    match a {
                        TemplateAttribute::Static {
                            name, value: text, ..
                        } => value(name, text),
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                if let AttributeValue::Text(text) = &a.value {
                                    value(a.name, text);
                                }
                                listeners +=
                                    usize::from(matches!(a.value, AttributeValue::Listener(_)));
                            }
                        }
                        _ => {}
                    }
                }
                let inside = inside || marker;
                if inside {
                    assert_eq!(listeners, 0);
                    assert!(["div", "span", "hr"].contains(tag));
                }
                if marker {
                    out.push((
                        before.expect("marker has its exact reply coordinate"),
                        ghost,
                    ));
                }
                for child in children.iter() {
                    at(v, child, inside, out);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(children) = &v.dynamic_nodes[*id] {
                    for child in children {
                        for root in child.template.roots.iter() {
                            at(child, root, inside, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for root in v.template.roots.iter() {
        at(v, root, false, &mut out);
    }
    out
}

#[tokio::test(flavor = "current_thread")]
async fn actual_followed_thread_new_boundary_becomes_ghost_survives_append_and_resets_on_refresh() {
    let (mut dom, mut state, captured) = mount("thread-feed-replies", render);
    state.write().overlay = None;
    let (channel, root) = {
        let mut shell = state.write();
        let item = &mut shell.core.threads_view.items[0];
        item.root_msg.extra.insert("subscribed".into(), true.into());
        item.root_msg
            .extra
            .insert("last_read".into(), "1719800401.000000".into());
        (
            item.root_msg.channel.clone().unwrap(),
            item.root_msg.ts.clone().unwrap(),
        )
    };
    dom.render_immediate_to_vec();
    assert!(cues(captured.borrow().as_ref().unwrap()).is_empty());
    let id = format!("thread-feed-open-{channel}-{root}");
    assert!(
        button(captured.borrow().as_ref().unwrap(), &id)
            .unwrap()
            .disabled
    );
    let mut response = page(state);
    response.threads.truncate(1);
    response.max_ts = Some("1719800300.000000".into());
    publish(state, response).await;
    dom.render_immediate_to_vec();
    assert_eq!(
        cues(captured.borrow().as_ref().unwrap()),
        vec![("1719800403.000000".into(), false)]
    );
    assert!(
        !button(captured.borrow().as_ref().unwrap(), &id)
            .unwrap()
            .disabled
    );
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.thread_root.clone(),
            s.core.composer.text.clone(),
        )
    };
    // The aggregate category is not the source of the boundary.
    state.write().core.threads_view.items[0]
        .unread_replies
        .clear();
    dom.render_immediate_to_vec();
    assert_eq!(
        cues(captured.borrow().as_ref().unwrap()),
        vec![("1719800403.000000".into(), false)]
    );
    let key = ("T1".into(), channel.clone(), root.clone());
    let revision = state.peek().core.thread_read_revision(&key);
    assert!(crate::realtime::activity_counts::confirm_thread_read(
        &mut state.write().core,
        &key,
        "1719800403.000000",
        revision,
    ));
    assert_eq!(
        state.peek().core.thread_cursor(&key),
        Some("1719800403.000000")
    );
    dom.render_immediate_to_vec();
    assert_eq!(
        cues(captured.borrow().as_ref().unwrap()),
        vec![("1719800403.000000".into(), true)]
    );
    assert_eq!(
        state.peek().core.threads_view.max_ts.as_deref(),
        Some("1719800300.000000")
    );
    assert_eq!(
        state.peek().core.threads_view.next_cursor.as_deref(),
        Some("1719800403.000000")
    );
    let older = Source::capture(&state.peek()).unwrap();
    assert!(
        older
            .run_with(state, Mode::Older, |request| async move {
                assert_eq!(request.cursor.as_deref(), Some("1719800403.000000"));
                Ok(ThreadsViewPage::default())
            })
            .await
            .is_some()
    );
    dom.render_immediate_to_vec();
    assert_eq!(
        cues(captured.borrow().as_ref().unwrap()),
        vec![("1719800403.000000".into(), true)]
    );
    publish(state, page(state)).await;
    dom.render_immediate_to_vec();
    assert!(cues(captured.borrow().as_ref().unwrap()).is_empty());
    let s = state.peek();
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.thread_root.clone(),
            s.core.composer.text.clone()
        ),
        before
    );
    assert_eq!(
        s.core.thread_cursor(&("T1".into(), channel, root)),
        Some("1719800403.000000")
    );
}
