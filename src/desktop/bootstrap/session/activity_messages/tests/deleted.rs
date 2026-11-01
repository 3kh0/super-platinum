use super::*;
use crate::resource_reads::render_test::{Control, click};
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
const PEER: &str = "1719801000.000200";

fn row(v: &VNode, key: &str) -> Option<Control> {
    fn at(v: &VNode, node: &TemplateNode, key: &str) -> Option<Control> {
        match node {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                let mut found = false;
                let mut listener = None;
                for attr in attrs.iter() {
                    match attr {
                        TemplateAttribute::Static { name, value, .. } => {
                            found |= *name == "data-activity-key" && *value == key;
                        }
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                if a.name == "data-activity-key"
                                    && matches!(&a.value, AttributeValue::Text(t) if t==key)
                                {
                                    found = true;
                                }
                                if a.name == "onclick" {
                                    listener = Some(a.value.clone());
                                }
                            }
                        }
                    }
                }
                if found {
                    return listener.map(|click| Control {
                        click,
                        disabled: false,
                    });
                }
                children.iter().find_map(|n| at(v, n, key))
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Fragment(nodes) => nodes.iter().find_map(|n| row(n, key)),
                _ => None,
            },
            _ => None,
        }
    }
    v.template.roots.iter().find_map(|n| at(v, n, key))
}
fn visible(s: &ShellState) -> serde_json::Value {
    json!({"channel":s.core.active_channel,"channel_generation":s.channel_generation,
        "thread_generation":s.thread_generation,"composer":s.core.composer.text,
        "selected":s.core.activity.selected,"items":s.core.activity.items,
        "surfaces":format!("{:?}",s.surfaces),"overlay":format!("{:?}",s.overlay)})
}

#[tokio::test]
async fn actual_deleted_activity_card_and_queued_actions_cannot_restore_body_or_navigation() {
    let (mut dom, mut state, captured) = mount("activity", render);
    {
        let mut s = state.write();
        s.main_view = MainView::Activity;
        s.overlay = None;
        let epoch = s.core.account_epoch;
        s.core.activity.ensure_scope(epoch, "T1");
        s.core.activity.items = serde_json::from_value(json!([
            {"key":"deleted-mention","feed_ts":TS,"is_unread":true,"item":{"type":"at_user","message":{"channel":"C1","ts":TS,"text":"Stale deleted embedded body"}}},
            {"key":"peer-mention","feed_ts":PEER,"is_unread":true,"item":{"type":"at_user","message":{"channel":"C1","ts":PEER,"text":"Independent peer index"}}}
        ])).unwrap();
        assert!(s.select_activity_item("deleted-mention".into(), Some("C1"), Some(TS), None));
    }
    let queued =
        crate::activity_feed::Navigation::capture(&state.peek(), "deleted-mention").unwrap();
    dom.render_immediate_to_vec();
    let retained = row(captured.borrow().as_ref().unwrap(), "deleted-mention").unwrap();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Stale deleted embedded body"));
    let (transport, session, generation, before) = {
        let s = state.peek();
        (
            s.core.transport.clone().unwrap(),
            s.core.session.as_ref().unwrap().workspaces["T1"].clone(),
            s.core.workspaces["T1"].rt_generation,
            visible(&s),
        )
    };
    let (release, held) = tokio::sync::oneshot::channel();
    let mut held = Some(held);
    let mut work = Box::pin(hydrate_with(state, &transport, &session, move |ids| {
        assert_eq!(ids, vec![("C1".into(), vec![TS.into(), PEER.into()])]);
        let held = held.take().unwrap();
        async move {
            held.await.unwrap();
            let mut result = page();
            result.messages_data.get_mut("C1").unwrap().messages.push(
                super_platinum_core::slack::models::Message {
                    ts: Some(PEER.into()),
                    text: Some("Independent accepted peer".into()),
                    ..Default::default()
                },
            );
            Ok(result)
        }
    }));
    pending(work.as_mut()).await;
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        event("Edited before deletion", json!([])),
        false,
    );
    let deletion = parse_event(
        &json!({"type":"message","subtype":"message_deleted","channel":"C1","deleted_ts":TS})
            .to_string(),
    )
    .unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, deletion, false);
    assert!(state.peek().core.activity.hydrated.is_empty());
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        event("Late deleted edit", json!([])),
        false,
    );
    queued.run(state).await;
    assert_eq!(visible(&state.peek()), before);
    assert!(crate::activity_feed::Navigation::capture(&state.peek(), "deleted-mention").is_none());
    release.send(()).unwrap();
    work.await;
    dom.render_immediate_to_vec();
    let rendered = text(captured.borrow().as_ref().unwrap());
    assert!(
        !rendered.contains("Stale deleted embedded body")
            && !rendered.contains("Late deleted edit")
            && !rendered.contains("Old hydrated rich")
    );
    assert!(rendered.contains("Independent accepted peer"));
    assert!(row(captured.borrow().as_ref().unwrap(), "deleted-mention").is_none());
    assert!(row(captured.borrow().as_ref().unwrap(), "peer-mention").is_some());
    assert!(
        !state
            .peek()
            .core
            .activity
            .hydrated
            .contains_key(&("C1".into(), TS.into()))
    );
    click(&dom, retained);
    assert_eq!(visible(&state.peek()), before);
    let calls = std::cell::Cell::new(0);
    hydrate_with(state, &transport, &session, |_| {
        calls.set(calls.get() + 1);
        async { Ok(page()) }
    })
    .await;
    assert_eq!(
        calls.get(),
        0,
        "known deletion must not reenter missing-body acquisition"
    );
    assert_eq!(visible(&state.peek()), before);
}
