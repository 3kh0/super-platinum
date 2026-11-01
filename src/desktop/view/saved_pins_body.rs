use crate::{resource_reads::render_test::mount, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::{
    later::SavedKey,
    pins::{PinnedItem, PinsPage},
    slack::{models::Message, realtime::parse_event},
};

fn mounted_text(dom: &VirtualDom) -> String {
    use dioxus::core::{DynamicNode, TemplateNode, VNode};
    fn walk(dom: &VirtualDom, v: &VNode, n: &TemplateNode, out: &mut String) {
        match n {
            TemplateNode::Text { text } => out.push_str(text),
            TemplateNode::Element { children, .. } => {
                for child in *children {
                    walk(dom, v, child, out);
                }
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(text) => out.push_str(&text.value),
                DynamicNode::Fragment(children) => {
                    for child in children.iter() {
                        tree(dom, child, out);
                    }
                }
                DynamicNode::Component(component) => {
                    let scope = component
                        .mounted_scope(*id, v, dom)
                        .expect("actual mounted card component");
                    tree(dom, scope.root_node(), out);
                }
                _ => {}
            },
        }
    }
    fn tree(dom: &VirtualDom, v: &VNode, out: &mut String) {
        for node in v.template.roots {
            walk(dom, v, node, out);
        }
    }
    let mut out = String::new();
    tree(dom, dom.base_scope().root_node(), &mut out);
    out
}

fn render(state: Signal<ShellState>) -> Element {
    let channel = state.read().core.threads_view.items[0]
        .channel()
        .unwrap()
        .clone();
    rsx! {
        super::later::LaterPanel {state}
        super::pins::PinsPanel {state,channel}
    }
}
fn page(channel: &str, message: Message) -> PinsPage {
    PinsPage {
        items: vec![PinnedItem {
            kind: "message".into(),
            channel: Some(channel.into()),
            message: Some(message),
            ..Default::default()
        }],
    }
}

#[test]
fn saved_and_pinned_cards_repaint_original_explicit_removal_and_keep_it_over_late_snapshots_without_overriding_fresh_rich_cards()
 {
    let (mut dom, mut state, _captured) = mount("thread-feed-rich-root", render);
    let mut old = state.peek().core.threads_view.items[0].root_msg.clone();
    old.attachments =
        vec![serde_json::from_value(serde_json::json!({"text":"Old attachment"})).unwrap()];
    old.reactions = vec![
        serde_json::from_value(serde_json::json!({"name":"eyes","count":1,"users":["U1"]}))
            .unwrap(),
    ];
    let channel = old.channel.clone().unwrap();
    let ts = old.ts.clone().unwrap();
    let key = SavedKey::message(&channel, &ts);
    let (hydration, pins_request, generation) = {
        let mut s = state.write();
        let epoch = s.core.account_epoch;
        s.core.later.ensure_scope(epoch, "T1");
        s.core.later.items.insert(
            key.clone(),
            serde_json::from_value(
                serde_json::json!({"item_type":"message","item_id":channel,"ts":ts}),
            )
            .unwrap(),
        );
        s.core.later.visible = vec![key.clone()];
        s.core.later.hydrate_message(key.clone(), old.clone());
        s.core.pins.ensure_scope(epoch, "T1");
        let first = s.core.pins.begin(&channel);
        assert!(s.core.pins.apply(first, page(&channel, old.clone())));
        (
            s.core.later.begin_hydration(),
            s.core.pins.begin(&channel),
            s.core.workspaces["T1"].rt_generation,
        )
    };
    dom.render_immediate_to_vec();
    let original_text = mounted_text(&dom);
    assert!(
        original_text.matches("Release notes").count() == 2,
        "both actual cards: {original_text}"
    );
    let event=parse_event(&serde_json::json!({"type":"message","subtype":"message_changed","channel":channel,"message":{"ts":ts,"text":"Replacement plain card","blocks":[],"attachments":[]}}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    dom.render_immediate_to_vec();
    let plain = mounted_text(&dom);
    assert_eq!(plain.matches("Replacement plain card").count(), 2);
    assert!(!plain.contains("Release notes") && !plain.contains("Old attachment"));
    {
        let mut s = state.write();
        s.core
            .later
            .hydrate_message_since(key.clone(), old.clone(), hydration);
        assert!(s.core.pins.apply(pins_request, page(&channel, old.clone())));
        for current in [
            &s.core.later.hydrated[&key],
            s.core.pins.items[&format!("message:{ts}")]
                .message
                .as_ref()
                .unwrap(),
        ] {
            assert!(current.blocks.is_empty() && current.attachments.is_empty());
            assert_eq!(current.reactions[0].name, "eyes");
        }
    }
    dom.render_immediate_to_vec();
    assert_eq!(
        mounted_text(&dom).matches("Replacement plain card").count(),
        2
    );

    let mut fresh = old;
    fresh.text = Some("Fresh fallback".into());
    fresh.blocks = vec![
        serde_json::json!({"type":"section","text":{"type":"plain_text","text":"Fresh rich card"}}),
    ];
    fresh.attachments =
        vec![serde_json::from_value(serde_json::json!({"text":"Fresh attachment"})).unwrap()];
    {
        let mut s = state.write();
        let newer = s.core.later.begin_hydration();
        let newer_pins = s.core.pins.begin(&channel);
        let count = super_platinum_core::slack::events::RtEvent::MessageChanged {
            channel: channel.clone(),
            message: serde_json::from_value(
                serde_json::json!({"channel":channel,"ts":ts,"reply_count":6}),
            )
            .unwrap(),
            original_update: None,
        };
        crate::realtime::apply_in_view(&mut s.core, "T1", generation, count, false);
        s.core
            .later
            .hydrate_message_since(key.clone(), fresh.clone(), newer);
        assert!(s.core.pins.apply(newer_pins, page(&channel, fresh)));
        assert!(s.thread_root.is_none());
    }
    dom.render_immediate_to_vec();
    let rendered = mounted_text(&dom);
    assert_eq!(rendered.matches("Fresh rich card").count(), 2, "{rendered}");
    assert!(!rendered.contains("Replacement plain card"));
}
