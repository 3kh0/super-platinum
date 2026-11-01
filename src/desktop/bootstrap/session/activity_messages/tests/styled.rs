//! Actual accepted hydration and live local edits drive passive mention styles.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};

fn styles(v: &VNode) -> Vec<String> {
    fn at(v: &VNode, n: &TemplateNode, inside: bool, out: &mut Vec<String>) {
        match n {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut classes = Vec::new();
                let mut listeners = 0;
                for a in attrs.iter() {
                    match a {
                        TemplateAttribute::Static {
                            name: "class",
                            value,
                            ..
                        } => classes.push((*value).to_string()),
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                if a.name == "class" {
                                    if let AttributeValue::Text(s) = &a.value {
                                        classes.push(s.clone());
                                    }
                                }
                                listeners +=
                                    usize::from(matches!(a.value, AttributeValue::Listener(_)));
                            }
                        }
                        _ => {}
                    }
                }
                let inside = inside
                    || classes
                        .iter()
                        .any(|s| s.split_whitespace().any(|s| s == "activity-preview"));
                if inside {
                    assert_eq!(listeners, 0);
                    assert!(
                        ![
                            "a", "button", "img", "script", "iframe", "audio", "video", "object"
                        ]
                        .contains(tag)
                    );
                    out.extend(classes);
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

#[tokio::test]
async fn actual_hydrated_mention_styles_keep_text_and_follow_edit_clear_and_deletion() {
    let (mut dom, mut state, captured) = mount("activity", render);
    {
        let mut s = state.write();
        s.main_view = MainView::Activity;
        s.overlay = None;
        let epoch = s.core.account_epoch;
        s.core.activity.ensure_scope(epoch, "T1");
        s.core.activity.items = serde_json::from_value(json!([{
            "key":"styled-mention", "feed_ts":TS, "is_unread":true,
            "item":{"type":"at_user","message":{"channel":"C1","ts":TS,"author_user_id":"U2","text":"Old embedded fallback"}}
        }])).unwrap();
    }
    let (transport, session, generation, before) = {
        let s = state.peek();
        (
            s.core.transport.clone().unwrap(),
            s.core.session.as_ref().unwrap().workspaces["T1"].clone(),
            s.core.workspaces["T1"].rt_generation,
            (
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.composer.text.clone(),
                s.core.activity.selected.clone(),
            ),
        )
    };
    hydrate_with(state, &transport, &session, |ids| async move {
        assert_eq!(ids, vec![("C1".into(), vec![TS.into()])]);
        let mut result = page();
        let body = &mut result.messages_data.get_mut("C1").unwrap().messages[0];
        body.text = Some("Review this important update".into());
        body.blocks = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"Review "},
                {"type":"text","text":"this important update","style":{"bold":true}}
            ]}]}),
        ];
        Ok(result)
    })
    .await;
    dom.render_immediate_to_vec();
    let value = text(captured.borrow().as_ref().unwrap());
    assert!(value.contains("Review this important update"));
    assert!(!value.contains("Old embedded fallback"));
    assert!(
        styles(captured.borrow().as_ref().unwrap())
            .iter()
            .any(|s| s.split_whitespace().any(|s| s == "bold"))
    );
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        event(
            "Edited inline emphasis",
            json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"Edited "},
                {"type":"text","text":"inline emphasis","style":{"italic":true}}
            ]}]}]),
        ),
        false,
    );
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Edited inline emphasis"));
    let current = styles(captured.borrow().as_ref().unwrap());
    assert!(
        current
            .iter()
            .any(|s| s.split_whitespace().any(|s| s == "italic"))
    );
    assert!(
        !current
            .iter()
            .any(|s| s.split_whitespace().any(|s| s == "bold"))
    );
    // Other native kinds and reactions preserve their existing plain preview.
    state.write().core.activity.items[0].item.kind = "thread_reply".into();
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Edited inline emphasis"));
    assert!(
        !styles(captured.borrow().as_ref().unwrap())
            .iter()
            .any(|s| s.split_whitespace().any(|s| s == "italic"))
    );
    state.write().core.activity.items[0].item.kind = "at_user".into();
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        event("", json!([])),
        false,
    );
    dom.render_immediate_to_vec();
    let cleared = text(captured.borrow().as_ref().unwrap());
    assert!(
        !cleared.contains("Edited inline emphasis") && !cleared.contains("Old embedded fallback")
    );
    // A message with no text/blocks has the original native placeholder. A
    // nonempty typed body that projects empty follows the original edit guard.
    assert!(cleared.contains("[no text]"));
    assert!(
        !styles(captured.borrow().as_ref().unwrap()).iter().any(|s| s
            .split_whitespace()
            .any(|s| ["bold", "italic", "strike"].contains(&s)))
    );
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        event("", json!([{"type":"divider"}])),
        false,
    );
    dom.render_immediate_to_vec();
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Old embedded fallback"));
    assert!(styles(captured.borrow().as_ref().unwrap()).is_empty());
    let deletion = parse_event(
        &json!({"type":"message","subtype":"message_deleted","channel":"C1","deleted_ts":TS})
            .to_string(),
    )
    .unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, deletion, false);
    dom.render_immediate_to_vec();
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Old embedded fallback"));
    assert!(styles(captured.borrow().as_ref().unwrap()).is_empty());
    let s = state.peek();
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.core.activity.selected.clone()
        ),
        before
    );
    assert!(s.core.activity.items[0].is_unread);
}
