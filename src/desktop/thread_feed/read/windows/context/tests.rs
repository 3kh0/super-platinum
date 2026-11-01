//! Actual mounted expansion and the SAME native prepare/Core/finish seam.
use super::*;
use crate::resource_reads::render_test::{button, click, mount, text};
use serde_json::json;
const ROOT: &str = "1719800000.000000";
fn message(n: usize) -> Message {
    serde_json::from_value(json!({"type":"message","channel":"C1","team":"T1","thread_ts":ROOT,"ts":format!("1719800001.{n:06}"),"user":"U1","text":format!("[CONTEXT{n:03}]")})).unwrap()
}
fn packet() -> ThreadsViewPage {
    serde_json::from_value(json!({"threads":[{"root_msg":{"type":"message","channel":"C1","team":"T1","thread_ts":ROOT,"ts":ROOT,"subscribed":true,"reply_count":160,"user":"U1","text":"Original context"},"latest_replies":(1..=160).map(message).collect::<Vec<_>>()}],"has_more":false})).unwrap()
}
fn render(state: Signal<ShellState>) -> Element {
    crate::view::thread_list_for_test(state, &state.read())
}
fn visible(state: Signal<ShellState>) -> Vec<String> {
    let s = state.peek();
    crate::view::selected_thread_replies(&s, &s.core.threads_view.items[0])
        .into_iter()
        .filter_map(|m| m.ts.clone())
        .collect()
}
fn apply(mut state: Signal<ShellState>, generation: u64, event: RtEvent) {
    crate::realtime::apply_in_view_with_context(
        &mut state.write(),
        "T1",
        generation,
        event,
        false,
        true,
    );
}
#[tokio::test(flavor = "current_thread")]
async fn thread_context_owned_visible_rows_survive_native_arrival_delete_live_edits_and_cap_without_action_reauthorization()
 {
    let (mut dom, mut state, captured) = mount("thread-feed", render);
    state.write().overlay = None;
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async { Ok(packet()) })
            .await
            .is_some()
    );
    dom.render_immediate_to_vec();
    assert_eq!(visible(state).len(), 100);
    let action = {
        let s = state.peek();
        Source::capture(&s)
            .unwrap()
            .earlier_replies(&s, &s.core.threads_view.items[0])
            .unwrap()
    };
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &action.control_id()).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert_eq!(visible(state).len(), 160);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("[CONTEXT001]"));
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    apply(
        state,
        generation,
        RtEvent::ReactionAdded {
            channel: "C1".into(),
            ts: message(1).ts.unwrap(),
            user: "U2".into(),
            reaction: "eyes".into(),
        },
    );
    let edit=serde_json::from_value(json!({"type":"message","channel":"C1","thread_ts":ROOT,"ts":message(1).ts,"text":"[EDITED CONTEXT001]"})).unwrap();
    apply(
        state,
        generation,
        RtEvent::MessageChanged {
            channel: "C1".into(),
            message: edit,
            original_update: None,
        },
    );
    state.write().main_view = MainView::Home;
    apply(state, generation, RtEvent::Message(message(161)));
    state.write().main_view = MainView::Threads;
    dom.render_immediate_to_vec();
    assert_eq!(visible(state).len(), 161);
    assert_eq!(visible(state).first(), message(1).ts.as_ref());
    let body = text(captured.borrow().as_ref().unwrap());
    assert!(body.contains("[EDITED CONTEXT001]") && body.contains("[CONTEXT161]"));
    assert_eq!(
        state.peek().core.threads_view.items[0].latest_replies[0].reactions[0].count,
        1
    );
    assert!(state.peek().thread_feed_reads.context.is_some());
    // The ORIGINAL exact-membership guard still retires. Reading memory cannot
    // authorize another local expansion, history read or newly arrived profile.
    assert!(
        source
            .earlier_replies(&state.peek(), &state.peek().core.threads_view.items[0])
            .is_none()
    );
    let new_profile = {
        let s = state.peek();
        Source::capture(&s).unwrap().profile(
            &s,
            &s.core.threads_view.items[0],
            s.core.threads_view.items[0].latest_replies.last().unwrap(),
        )
    };
    assert!(new_profile.is_none());
    action.reveal(state);
    assert_eq!(visible(state).len(), 161);
    apply(
        state,
        generation,
        RtEvent::MessageDeleted {
            channel: "C1".into(),
            deleted_ts: message(2).ts.unwrap(),
        },
    );
    dom.render_immediate_to_vec();
    assert_eq!(visible(state).len(), 160);
    assert!(!visible(state).contains(&message(2).ts.unwrap()));
    assert_eq!(visible(state).first(), message(1).ts.as_ref());
    for n in 162..=205 {
        apply(state, generation, RtEvent::Message(message(n)));
    }
    dom.render_immediate_to_vec();
    let current = visible(state);
    assert_eq!(current.len(), 200);
    assert_eq!(current.first(), message(6).ts.as_ref());
    assert_eq!(current.last(), message(205).ts.as_ref());
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("[EDITED CONTEXT001]"));
    // A stale native generation cannot mutate data. Observed original owner
    // retirement cannot be undone by restoring the public generation value.
    let before = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation = generation + 1;
    apply(state, generation, RtEvent::Message(message(206)));
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        before
    );
    assert_eq!(visible(state).len(), 100);
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation = generation;
    assert_eq!(
        visible(state).len(),
        100,
        "restored public metadata cannot revive reading memory"
    );
    assert!(!state.peek().core.thread_open);
    assert!(state.peek().profile_user.is_none());
}
