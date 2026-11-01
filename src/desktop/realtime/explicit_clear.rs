use crate::resource_reads::render_test::{mount, text};
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::slack::realtime::parse_event;

fn render(state: Signal<ShellState>) -> Element {
    crate::view::thread_list_for_test(state, &state.read())
}

#[test]
fn original_explicit_body_clear_updates_actual_global_root_and_owned_transcript_thread_copies_without_erasing_reactions()
 {
    let (mut dom, mut state, captured) = mount("thread-feed-rich-root", render);
    let mut root = state.peek().core.threads_view.items[0].root_msg.clone();
    root.attachments =
        vec![serde_json::from_value(serde_json::json!({"text":"Old attachment"})).unwrap()];
    root.reactions = vec![
        serde_json::from_value(serde_json::json!({"name":"eyes","count":1,"users":["U1"]}))
            .unwrap(),
    ];
    let channel = root.channel.clone().unwrap();
    let ts = root.ts.clone().unwrap();
    let key = ("T1".into(), channel.clone(), ts.clone());
    {
        let mut s = state.write();
        s.core.threads_view.items[0].root_msg = root.clone();
        s.core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .entry(channel.clone())
            .or_default()
            .upsert(root.clone());
        s.core.threads.entry(key.clone()).or_default().upsert(root);
    }
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Release notes"));
    let sequence = state.peek().core.threads_view.load_seq;
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let event=parse_event(&serde_json::json!({"type":"message","subtype":"message_changed","channel":channel,"message":{"ts":ts,"text":"Replacement plain first line\nReplacement second line","blocks":[],"attachments":[]}}).to_string()).unwrap();
    super::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    dom.render_immediate_to_vec();
    let rendered = text(captured.borrow().as_ref().unwrap());
    assert!(
        rendered.contains("Replacement plain first line")
            && rendered.contains("Replacement second line")
    );
    assert!(!rendered.contains("Release notes") && !rendered.contains("Review the changes"));
    let s = state.peek();
    for copy in [
        &s.core.threads_view.items[0].root_msg,
        s.core.workspaces["T1"].messages[&channel]
            .messages
            .iter()
            .find(|m| m.ts.as_deref() == Some(ts.as_str()))
            .unwrap(),
        s.core.threads[&key]
            .messages
            .iter()
            .find(|m| m.ts.as_deref() == Some(ts.as_str()))
            .unwrap(),
    ] {
        assert!(copy.blocks.is_empty() && copy.attachments.is_empty());
        assert_eq!(copy.reactions[0].name, "eyes");
        assert_eq!(
            copy.text.as_deref(),
            Some("Replacement plain first line\nReplacement second line")
        );
    }
    assert_eq!(s.core.threads_view.load_seq, sequence);
    assert!(
        s.thread_root.is_none(),
        "render and socket merge start no pane/read"
    );
}
