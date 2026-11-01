use super::*;
use crate::resource_reads::render_test::{button, click, mount, text};
use serde_json::json;
const ROOT: &str = "1719800000.000000";
const PEER: &str = "1719700000.000000";
fn item(root: &str, rows: usize, count: usize) -> ThreadViewItem {
    let seconds = root.split('.').next().unwrap().parse::<u64>().unwrap() + 1;
    serde_json::from_value(json!({
        "root_msg":{"type":"message","channel":"C1","team":"T1","ts":root,"thread_ts":root,"subscribed":true,"reply_count":count,"user":"U1","text":"Original discussion"},
        "latest_replies":(1..=rows).map(|n|json!({"type":"message","channel":"C1","team":"T1","thread_ts":root,"ts":format!("{seconds}.{n:06}"),"user":"U1","text":format!("[EARLIER{n:03}]")})).collect::<Vec<_>>()
    })).unwrap()
}
fn page(items: Vec<ThreadViewItem>) -> ThreadsViewPage {
    ThreadsViewPage {
        threads: items,
        has_more: true,
        ..Default::default()
    }
}
fn render(state: Signal<ShellState>) -> Element {
    crate::view::thread_list_for_test(state, &state.read())
}
async fn publish(state: Signal<ShellState>, items: Vec<ThreadViewItem>, mode: Mode) {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, mode, |_| async { Ok(page(items)) })
            .await
            .is_some()
    );
}
fn action(state: Signal<ShellState>, root: &str) -> Option<Action> {
    let s = state.peek();
    let item = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.ts.as_deref() == Some(root))?;
    Source::capture(&s)?.earlier_replies(&s, item)
}
fn visible(state: Signal<ShellState>, root: &str) -> usize {
    let s = state.peek();
    let item = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.ts.as_deref() == Some(root))
        .unwrap();
    crate::view::selected_thread_replies(&s, item).len()
}

#[tokio::test(flavor = "current_thread")]
async fn thread_earlier_original_packet_mounted_button_reveals_complete_context_without_io_or_core_mutation()
 {
    let (mut dom, mut state, captured) = mount("thread-feed", render);
    state.write().overlay = None;
    let original = item(ROOT, 160, 160);
    // A public fixture/vector does not construct the independent receipt.
    state.write().core.threads_view.items = vec![original.clone()];
    assert!(action(state, ROOT).is_none());
    publish(state, vec![original], Mode::Refresh).await;
    dom.render_immediate_to_vec();
    assert_eq!(visible(state, ROOT), 100);
    let body = text(captured.borrow().as_ref().unwrap());
    assert!(!body.contains("[EARLIER001]"));
    assert!(body.contains("[EARLIER160]"));
    let before = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    let thread_bags = state.peek().core.threads.len();
    let saved = action(state, ROOT).unwrap();
    let control = button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap();
    assert!(!control.disabled);
    click(&dom, control);
    dom.render_immediate_to_vec();
    assert_eq!(visible(state, ROOT), 160);
    let body = text(captured.borrow().as_ref().unwrap());
    assert!(body.contains("[EARLIER001]") && body.contains("[EARLIER160]"));
    assert!(!body.contains("Open the thread to read more replies."));
    assert!(button(captured.borrow().as_ref().unwrap(), &saved.control_id()).is_none());
    assert!(action(state, ROOT).is_none());
    saved.reveal(state); // repeated/queued callbacks cannot advance a successor
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        before
    );
    assert_eq!(state.peek().core.threads.len(), thread_bags);
    assert!(!state.peek().core.thread_open);
    assert!(state.peek().core.profile_pane.is_none());
    // Expanded context is the SAME live feed copy, not a private stale body.
    let ts = "1719800001.000001";
    state.write().core.threads_view.observe(
        &super_platinum_core::slack::events::RtEvent::ReactionAdded {
            channel: "C1".into(),
            ts: ts.into(),
            user: "U2".into(),
            reaction: "eyes".into(),
        },
    );
    state.write().core.threads_view.observe(&super_platinum_core::slack::events::RtEvent::MessageChanged {
        channel:"C1".into(),message:serde_json::from_value(json!({"type":"message","channel":"C1","thread_ts":ROOT,"ts":ts,"text":"[EDITED EARLIER]"})).unwrap(),original_update:None,
    });
    dom.render_immediate_to_vec();
    assert_eq!(visible(state, ROOT), 160);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("[EDITED EARLIER]"));
    assert_eq!(
        state.peek().core.threads_view.items[0].latest_replies[0].reactions[0].count,
        1
    );
}

#[tokio::test(flavor = "current_thread")]
async fn thread_earlier_partial_start_hidden_ambiguous_or_public_union_cannot_supply_local_coverage()
 {
    let (_dom, mut state, _) = mount("thread-feed", render);
    state.write().overlay = None;
    publish(state, vec![item(ROOT, 150, 500)], Mode::Refresh).await;
    assert!(
        action(state, ROOT).is_none(),
        "50 older without original reachedStart needs network"
    );
    state.write().core.threads_view.items[0]
        .root_msg
        .reply_count = Some(150);
    assert!(
        action(state, ROOT).is_none(),
        "later public count cannot mint original start"
    );
    let key = ("T1".into(), "C1".into(), ROOT.into());
    state.write().core.threads.entry(key).or_default().messages =
        item(ROOT, 1000, 1000).latest_replies;
    assert!(
        action(state, ROOT).is_none(),
        "larger canonical union supplies no original timestamps"
    );
    let mut generic_hidden = item(ROOT, 160, 160);
    generic_hidden.latest_replies[0]
        .extra
        .insert("hidden".into(), json!(true));
    publish(state, vec![generic_hidden], Mode::Refresh).await;
    assert!(
        action(state, ROOT).is_some(),
        "generic hidden is not Source isHiddenInThread"
    );
    for variant in 0..4 {
        let mut row = item(ROOT, 160, 160);
        match variant {
            0 => {
                row.latest_replies[0]
                    .extra
                    .insert("no_display".into(), json!(true));
            }
            1 => {
                row.latest_replies[0].team = Some("T2".into());
            }
            2 => {
                row.unread_replies = vec![row.latest_replies[0].clone()];
                row.unread_replies[0].text = Some("ambiguous duplicate".into());
            }
            _ => {
                row.root_msg.extra.remove("subscribed");
            }
        }
        publish(state, vec![row], Mode::Refresh).await;
        assert!(
            action(state, ROOT).is_none(),
            "unsupported variant {variant}"
        );
    }
    // A full100 older is independently revealable without reachedStart;
    // the finite renderer cap does not claim that the whole thread is loaded.
    publish(state, vec![item(ROOT, 250, 500)], Mode::Refresh).await;
    action(state, ROOT).unwrap().reveal(state);
    assert_eq!(visible(state, ROOT), 200);
    assert!(action(state, ROOT).is_none());
    assert_eq!(
        state.peek().core.threads_view.items[0].root_msg.reply_count,
        Some(500)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn thread_earlier_queued_peer_append_and_observed_owner_or_membership_retirement_preserve_original_windows()
 {
    let (_dom, mut state, _) = mount("thread-feed", render);
    state.write().overlay = None;
    publish(
        state,
        vec![item(ROOT, 160, 160), item(PEER, 160, 160)],
        Mode::Refresh,
    )
    .await;
    let first = action(state, ROOT).unwrap();
    let peer = action(state, PEER).unwrap();
    first.reveal(state);
    peer.reveal(state);
    assert_eq!(visible(state, ROOT), 160);
    assert_eq!(
        visible(state, PEER),
        100,
        "queued peer cannot borrow successor pointer"
    );
    action(state, PEER).unwrap().reveal(state);
    assert_eq!(visible(state, PEER), 160);
    publish(state, vec![item("1719600000.000000", 3, 3)], Mode::Older).await;
    assert_eq!(
        visible(state, ROOT),
        160,
        "original append retains untouched original receipt"
    );
    assert_eq!(visible(state, PEER), 160);
    // Full core's existing merge still delivers realtime partials/reactions to
    // these same vectors. An observed membership replacement instead retires.
    let original = state.peek().core.threads_view.items[0].clone();
    state.write().core.threads_view.items[0]
        .latest_replies
        .remove(0);
    assert_eq!(visible(state, ROOT), 100);
    state.write().core.threads_view.items[0] = original;
    assert_eq!(
        visible(state, ROOT),
        100,
        "restoring public data cannot revive retired key"
    );
    publish(state, vec![item(ROOT, 160, 160)], Mode::Refresh).await;
    let old = action(state, ROOT).unwrap();
    let cookie = state.peek().core.session.as_ref().unwrap().d_cookie.clone();
    state.write().core.session.as_mut().unwrap().d_cookie = "replacement".into();
    old.reveal(state);
    state.write().core.session.as_mut().unwrap().d_cookie = cookie;
    old.reveal(state);
    assert!(action(state, ROOT).is_none());
    assert_eq!(visible(state, ROOT), 100);
}
