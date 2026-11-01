use super::*;
use crate::resource_reads::render_test::{mount, text};
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use serde_json::json;

fn render_feed(state: Signal<ShellState>) -> Element {
    super::super::thread_list(state, &state.read())
}

#[test]
fn available_replies_are_coherent_hidden_filtered_deduplicated_and_bounded_without_changing_feed() {
    let shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "thread-feed-replies",
    );
    let mut item = shell.core.threads_view.items[0].clone();
    let channel = item.root_msg.channel.clone().unwrap();
    let root = item.root_msg.ts.clone().unwrap();
    item.latest_replies = (1..=105).rev().map(|n| serde_json::from_value(json!({"channel":channel,"thread_ts":root,"user":"U1","ts":format!("{}.000000",1719800400+n),"text":format!("Reply {n}")})).unwrap()).collect();
    item.unread_replies = vec![item.latest_replies[0].clone()];
    for extra in [
        json!({"ts":"1719800506.000000","channel":"OTHER","thread_ts":root,"text":"foreign channel"}),
        json!({"ts":"1719800507.000000","channel":channel,"thread_ts":"OTHER","text":"foreign root"}),
        json!({"ts":"1719800508.000000","team":"OTHER","text":"foreign team"}),
        json!({"ts":"1719800509.000000","no_display":true,"text":"hidden"}),
        json!({"ts":"1719800510.000000","_hidden_stub":"present","text":"stub"}),
        json!({"ts":"1719800511.000000","isNonExistent":true,"text":"nonexistent"}),
        json!({"ts":root,"text":"root duplicate"}),
        json!({"ts":"1719800400.000001x","text":"invalid timestamp"}),
        json!({"text":"missing timestamp"}),
    ] {
        item.latest_replies
            .push(serde_json::from_value(extra).unwrap());
    }
    let before = serde_json::to_value(&item).unwrap();
    let replies = selected(&shell, &item);
    assert_eq!(replies.len(), MAX_REPLIES);
    assert_eq!(replies[0].ts.as_deref(), Some("1719800406.000000"));
    assert_eq!(
        replies.last().unwrap().ts.as_deref(),
        Some("1719800505.000000")
    );
    assert_eq!(serde_json::to_value(&item).unwrap(), before);
    let mut retired = shell;
    retired.core.account_epoch += 1;
    assert!(selected(&retired, &item).is_empty());
}

// Inspect only the new reply-body subtree; ordinary root/navigation controls
// remain outside this reading projection.
fn inert_bodies(v: &VNode) -> usize {
    fn visit(v: &VNode, node: &TemplateNode, inside: bool, count: &mut usize) {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let body = attrs.iter().any(|a| match a {
                    TemplateAttribute::Static {name:"class",value,..} => value.split_whitespace().any(|s| s=="thread-feed-reply-body"),
                    TemplateAttribute::Dynamic {id} => v.dynamic_attrs[*id].iter().any(|a| a.name=="class"&&matches!(&a.value,AttributeValue::Text(s) if s.split_whitespace().any(|s| s=="thread-feed-reply-body"))),
                    _ => false,
                });
                if body {
                    *count += 1;
                }
                let inside = inside || body;
                if inside {
                    assert!(!matches!(
                        *tag,
                        "a" | "img" | "button" | "form" | "input" | "iframe"
                    ));
                    for a in attrs.iter() {
                        match a {
                            TemplateAttribute::Static { name, .. } => {
                                assert!(!matches!(*name, "href" | "src"))
                            }
                            TemplateAttribute::Dynamic { id } => {
                                for a in v.dynamic_attrs[*id].iter() {
                                    assert!(!matches!(a.name, "href" | "src"));
                                    assert!(!matches!(&a.value, AttributeValue::Listener(_)));
                                }
                            }
                        }
                    }
                }
                for n in children.iter() {
                    visit(v, n, inside, count);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for v in nodes.iter() {
                        for n in v.template.roots.iter() {
                            visit(v, n, inside, count);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut count = 0;
    for n in v.template.roots.iter() {
        visit(v, n, false, &mut count);
    }
    count
}

#[test]
fn actual_global_reply_reading_tracks_realtime_edits_deletes_and_scope_without_history_or_actions()
{
    for alias in ["thread-feed-replies", "thread-feed-replies-narrow"] {
        let (mut dom, mut state, captured) = mount(alias, render_feed);
        let key = {
            let s = state.peek();
            let i = &s.core.threads_view.items[0];
            (
                s.core.active_team.clone().unwrap(),
                i.root_msg.channel.clone().unwrap(),
                i.root_msg.ts.clone().unwrap(),
            )
        };
        let history_before = format!("{:?}", state.peek().core.threads);
        let before = {
            let s = state.peek();
            (
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone(),
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.threads_view.load_seq,
                s.core.thread_cursor(&key).map(str::to_owned),
                s.core.pending_marks.len(),
            )
        };
        dom.render_immediate_to_vec();
        assert_eq!(format!("{:?}", state.peek().core.threads), history_before);
        let rendered = text(captured.borrow().as_ref().unwrap());
        for expected in [
            "Reply context",
            "A second readable paragraph.",
            "Read the reply notes",
            "Reply diagram",
            "Reply action",
            "Latest supplied reply",
            "Open the thread to read more replies.",
        ] {
            assert!(rendered.contains(expected), "{expected}");
        }
        assert!(!rendered.contains("Fallback must not duplicate reply blocks"));
        assert_eq!(inert_bodies(captured.borrow().as_ref().unwrap()), 2);
        let (channel, root, generation) = {
            let s = state.peek();
            let i = &s.core.threads_view.items[0];
            (
                i.root_msg.channel.clone().unwrap(),
                i.root_msg.ts.clone().unwrap(),
                s.core.workspaces["T1"].rt_generation,
            )
        };
        let event=super_platinum_core::slack::realtime::parse_event(&json!({"type":"message","subtype":"message_changed","channel":channel,"message":{"type":"message","channel":channel,"thread_ts":root,"ts":"1719800403.000000","user":"U1","text":"Edited supplied reply"}}).to_string()).unwrap();
        crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
        dom.render_immediate_to_vec();
        let edited = text(captured.borrow().as_ref().unwrap());
        assert!(edited.contains("Edited supplied reply"));
        assert!(!edited.contains("Latest supplied reply"));
        let event=super_platinum_core::slack::realtime::parse_event(&json!({"type":"message","subtype":"message_deleted","channel":channel,"deleted_ts":"1719800403.000000"}).to_string()).unwrap();
        crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
        dom.render_immediate_to_vec();
        let deleted = text(captured.borrow().as_ref().unwrap());
        assert!(!deleted.contains("Edited supplied reply"));
        assert!(deleted.contains("Reply context"));
        assert_eq!(inert_bodies(captured.borrow().as_ref().unwrap()), 1);
        {
            let s = state.peek();
            assert_eq!(
                s.media
                    .registered_source_count("https://fixture.invalid/reply.png"),
                0
            );
            assert_eq!(
                (
                    s.core.composer.text.clone(),
                    s.core.thread_composer.text.clone(),
                    s.core.active_channel.clone(),
                    s.channel_generation,
                    s.thread_generation,
                    s.core.threads_view.load_seq,
                    s.core.thread_cursor(&key).map(str::to_owned),
                    s.core.pending_marks.len()
                ),
                before
            );
        }
        state.write().core.account_epoch += 1;
        dom.render_immediate_to_vec();
        assert!(!text(captured.borrow().as_ref().unwrap()).contains("Reply context"));
    }
}

mod reactions;
