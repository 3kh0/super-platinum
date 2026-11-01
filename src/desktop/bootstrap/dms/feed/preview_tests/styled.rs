//! Accepted DM previews decorate the existing snippet without gaining actions.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};

fn styles(v: &VNode) -> Vec<String> {
    fn at(v: &VNode, node: &TemplateNode, inside: bool, out: &mut Vec<String>) {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut classes = Vec::new();
                let mut listeners = 0;
                for attr in attrs.iter() {
                    match attr {
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
                        .any(|s| s.split_whitespace().any(|s| s == "dm-snippet"));
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
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for child in nodes {
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
async fn accepted_dm_inline_styles_follow_edits_and_deletion_without_changing_snippet_or_navigation()
 {
    let (mut dom, mut state, captured) = mount("dms-cold-rows", render);
    let owner = Owner::capture(&state.peek()).unwrap();
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let mut entry = rich();
    entry.message = Some(
        serde_json::from_value(json!({
            "ts":"1719801000.000400", "user":user, "text":"Review the release plan",
            "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"Review "},
                {"type":"text","text":"the release","style":{"bold":true}},
                {"type":"text","text":" plan"}
            ]}]}]
        }))
        .unwrap(),
    );
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.detail_open(),
        )
    };
    assert_eq!(
        load_with(state, owner, false, None, |request| async move {
            assert!(request.url.contains("client.dms"));
            Ok(page(entry))
        })
        .await,
        Outcome::Published
    );
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("You: Review the release plan"));
    assert!(
        styles(captured.borrow().as_ref().unwrap())
            .iter()
            .any(|s| s.split_whitespace().any(|s| s == "bold"))
    );
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    crate::realtime::apply_in_view(
        &mut state.write().core,
        "T1",
        generation,
        edit(
            json!({"ts":"1719801000.000400", "text":"Updated inline emphasis", "attachments":[],
            "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"Updated "},
                {"type":"text","text":"inline emphasis","style":{"italic":true}}
            ]}]}]}),
        ),
        false,
    );
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("You: Updated inline emphasis"));
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
    let deleted = parse_event(&json!({"type":"message","subtype":"message_deleted","channel":"D_COLD_CACHED","deleted_ts":"1719801000.000400"}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, deleted, false);
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    assert!(
        !styles(captured.borrow().as_ref().unwrap()).iter().any(|s| s
            .split_whitespace()
            .any(|s| ["bold", "italic", "strike"].contains(&s)))
    );
    let s = state.peek();
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.detail_open()
        ),
        before
    );
    assert_eq!(s.core.dms.entries.len(), 1);
    assert_eq!(
        s.core.dms.entries[0].latest.as_deref(),
        Some("1719801000.000400")
    );
    assert_eq!(s.core.dms.next_cursor.as_deref(), Some("same-older"));
}
