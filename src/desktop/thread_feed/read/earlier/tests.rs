//! Actual mounted read, raw decoding, queue refusal and loaded-result recovery.
use super::*;
use crate::resource_reads::render_test::{button, click, mount, settle, text};
use serde_json::json;
use std::rc::Rc;
use super_platinum_core::slack::realtime::{self, RtUpdate};
const ROOT: &str = "1719800000.000000";
const PEER: &str = "1719700000.000000";
fn ts(root: &str, n: usize) -> String {
    format!(
        "{}.{n:06}",
        root.split('.').next().unwrap().parse::<u64>().unwrap() + 1
    )
}
fn message(root: &str, n: usize) -> Message {
    serde_json::from_value(json!({"type":"message","channel":"C1","team":"T1","thread_ts":root,"ts":ts(root,n),"user":"U1","text":format!("[NETWORK {root} {n:03}]")})).unwrap()
}
fn item(root: &str) -> ThreadViewItem {
    serde_json::from_value(json!({"root_msg":{"type":"message","channel":"C1","team":"T1","ts":root,"thread_ts":root,"user":"U1","text":"Original root","subscribed":true,"reply_count":90,"latest_reply":ts(root,90)},"latest_replies":(81..=90).map(|n|message(root,n)).collect::<Vec<_>>()})).unwrap()
}
fn response(root: &str) -> Value {
    let mut messages = vec![serde_json::to_value(item(root).root_msg).unwrap()];
    messages.extend((1..=81).map(|n| serde_json::to_value(message(root, n)).unwrap()));
    json!({"ok":true,"messages":messages,"has_more":false,"response_metadata":{"next_cursor":""}})
}
fn render(state: Signal<ShellState>) -> Element {
    crate::view::thread_list_for_test(state, &state.read())
}
async fn publish(state: Signal<ShellState>) {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async {
                Ok(ThreadsViewPage {
                    threads: vec![item(ROOT), item(PEER)],
                    has_more: true,
                    ..Default::default()
                })
            })
            .await
            .is_some()
    );
}
fn action(state: Signal<ShellState>, root: &str) -> Option<Action> {
    let s = state.peek();
    let i = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.ts.as_deref() == Some(root))?;
    Source::capture(&s)?.network_earlier_replies(&s, i)
}
fn shown(state: Signal<ShellState>) -> Vec<String> {
    let s = state.peek();
    let i = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.ts.as_deref() == Some(ROOT))
        .unwrap();
    crate::view::selected_thread_replies(&s, i)
        .iter()
        .map(|m| m.ts.clone().unwrap())
        .collect()
}
fn apply_updates(mut state: Signal<ShellState>, updates: Vec<RtUpdate>) {
    for update in updates {
        match update {
            RtUpdate::Event { generation, event } => crate::realtime::apply_in_view_with_context(
                &mut state.write(),
                "T1",
                generation,
                *event,
                true,
                true,
            ),
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => accept_barrier(&mut state.write(), "T1", generation, barrier),
            RtUpdate::Connected { .. } => {}
            _ => panic!("fixture only emits original read updates"),
        }
    }
}
#[tokio::test(flavor = "current_thread")]
async fn thread_earlier_network_mounted_full_body_read_preserves_live_rows_recovers_offline_and_rejects_ingress_queue_reentry_and_peer_races()
 {
    let (mut dom, mut state, captured) = mount("thread-feed", render);
    state.write().overlay = None;
    let (connection, initial) = realtime::protected_connection(7);
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 7;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        // A public packet/status cannot construct the independent original IO purpose.
        s.core.threads_view.items = vec![item(ROOT)];
    }
    apply_updates(state, initial);
    assert!(action(state, ROOT).is_none());
    publish(state).await;
    {
        let mut s = state.write();
        let i = s
            .core
            .threads_view
            .items
            .iter_mut()
            .find(|i| i.root_msg.ts.as_deref() == Some(ROOT))
            .unwrap();
        let row = i.latest_replies.pop().unwrap();
        i.unread_replies.push(row);
    }
    assert!(
        action(state, ROOT).is_none(),
        "the latest-only purpose cannot silently become a mixed unread packet"
    );
    publish(state).await;
    let io = Rc::new(test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    dom.render_immediate_to_vec();
    let saved = action(state, ROOT).unwrap();
    let peer = action(state, PEER).unwrap();
    let original = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    let pending = io.expect(ROOT, &ts(ROOT, 81));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 1).await;
    dom.render_immediate_to_vec();
    assert!(
        button(captured.borrow().as_ref().unwrap(), &saved.control_id())
            .unwrap()
            .disabled
    );
    saved.start(state);
    peer.start(state);
    assert_eq!(
        io.reads.borrow().len(),
        1,
        "queued own/peer callbacks cannot claim a newer control occurrence"
    );
    pending
        .send(Err(Error::Offline("fixture outage".into())))
        .unwrap();
    settle(&mut dom, || {
        io.completed.get() == 1 && action(state, ROOT).is_some_and(|a| !a.pending())
    })
    .await;
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        original
    );
    // The original coordinate retries; successful full bodies are decoded raw.
    let reply = io.expect(ROOT, &ts(ROOT, 81));
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 2).await;
    reply.send(Ok(response(ROOT))).unwrap();
    settle(&mut dom, || shown(state).len() == 90).await;
    assert_eq!(shown(state).first(), Some(&ts(ROOT, 1)));
    assert!(text(captured.borrow().as_ref().unwrap()).contains(&format!("[NETWORK {ROOT} 001]")));
    assert!(action(state, ROOT).is_none());
    assert_eq!(
        state
            .peek()
            .core
            .threads_view
            .items
            .iter()
            .find(|i| i.root_msg.ts.as_deref() == Some(PEER))
            .unwrap()
            .latest_replies
            .len(),
        10
    );
    assert!(!state.peek().core.thread_open && state.peek().core.profile_pane.is_none());
    // Newly read IDs are reading data, never an original author/profile grant.
    {
        let s = state.peek();
        let i = s
            .core
            .threads_view
            .items
            .iter()
            .find(|i| i.root_msg.ts.as_deref() == Some(ROOT))
            .unwrap();
        assert!(
            Source::capture(&s)
                .unwrap()
                .profile(&s, i, &i.latest_replies[0])
                .is_none()
        );
    }
    // Already published context uses live reducers and survives a later arrival.
    let edit=json!({"type":"message","subtype":"message_changed","channel":"C1","message":{"type":"message","channel":"C1","thread_ts":ROOT,"ts":ts(ROOT,1),"text":"[LIVE EARLIER EDIT]"}}).to_string();
    apply_updates(state, realtime::protected_updates(&connection, 7, &edit));
    apply_updates(state,realtime::protected_updates(&connection,7,&json!({"type":"reaction_added","user":"U2","reaction":"eyes","item":{"type":"message","channel":"C1","ts":ts(ROOT,1)}}).to_string()));
    apply_updates(
        state,
        realtime::protected_updates(
            &connection,
            7,
            &serde_json::to_string(&message(ROOT, 91)).unwrap(),
        ),
    );
    dom.render_immediate_to_vec();
    assert_eq!(shown(state).len(), 91);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("[LIVE EARLIER EDIT]"));
    let s = state.peek();
    let i = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.ts.as_deref() == Some(ROOT))
        .unwrap();
    assert_eq!(i.latest_replies[0].reactions[0].count, 1);
    drop(s);

    // A frame queued BEFORE arming cannot be hidden by delayed native delivery.
    publish(state).await;
    let queued = realtime::protected_updates(
        &connection,
        7,
        r#"{"type":"message","subtype":"message_changed","channel":"C1"}"#,
    );
    let before_reads = io.reads.borrow().len();
    let blocked = action(state, ROOT).unwrap();
    assert!(blocked.pending());
    blocked.start(state);
    assert_eq!(io.reads.borrow().len(), before_reads);
    apply_updates(state, queued);
    blocked.start(state);
    assert_eq!(
        io.reads.borrow().len(),
        before_reads,
        "a captured disabled control cannot borrow a later barrier"
    );
    dom.render_immediate_to_vec();
    // Ingress DURING the HTTP await retires even a malformed/dropped frame,
    // before the native consumer sees the barrier or the HTTP response wins.
    let reply = io.expect(ROOT, &ts(ROOT, 81));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == before_reads + 1).await;
    let queued = realtime::protected_updates(
        &connection,
        7,
        r#"{"type":"future_message_shape","message":{"channel":"C1"}}"#,
    );
    reply.send(Ok(response(ROOT))).unwrap();
    settle(&mut dom, || io.completed.get() == 3).await;
    assert_eq!(
        shown(state).len(),
        10,
        "raw ingress defeats HTTP publication without waiting for native dequeue"
    );
    apply_updates(state, queued);
    dom.render_immediate_to_vec();
    // Typed defaults, explicit server denial and hidden rows are all rejected
    // by the actual raw path, preserving the same retry coordinate and rows.
    for defect in 0..6 {
        let before = io.reads.borrow().len();
        let completed = io.completed.get();
        let reply = io.expect(ROOT, &ts(ROOT, 81));
        dom.render_immediate_to_vec();
        click(
            &dom,
            button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap(),
        );
        settle(&mut dom, || io.reads.borrow().len() == before + 1).await;
        let mut malformed = response(ROOT);
        match defect {
            0 => {
                malformed.as_object_mut().unwrap().remove("has_more");
            }
            1 => malformed["deleted"] = json!([ts(ROOT, 1)]),
            2 => malformed["messages"][1]["hidden"] = json!(true),
            3 => malformed["messages"][0]["subscribed"] = json!(false),
            4 => malformed["messages"][0]["reply_count"] = json!(91),
            _ => malformed["messages"][0]["latest_reply"] = json!(ts(ROOT, 91)),
        }
        reply.send(Ok(malformed)).unwrap();
        settle(&mut dom, || {
            io.completed.get() == completed + 1 && action(state, ROOT).is_some_and(|a| !a.pending())
        })
        .await;
        assert_eq!(shown(state).len(), 10);
    }
    // Original callback/response cannot survive actual thread visit retirement.
    let reply = io.expect(ROOT, &ts(ROOT, 81));
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 10).await;
    {
        let mut s = state.write();
        s.close_thread();
        s.main_view = MainView::Home;
        s.main_view = MainView::Threads;
    }
    reply.send(Ok(response(ROOT))).unwrap();
    settle(&mut dom, || io.completed.get() == 10).await;
    assert_eq!(shown(state).len(), 10);
    // An incomplete first page stays explicit pane fallback, no retry loop.
    publish(state).await;
    dom.render_immediate_to_vec();
    let reply = io.expect(ROOT, &ts(ROOT, 81));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 11).await;
    let mut incomplete = response(ROOT);
    incomplete["has_more"] = json!(true);
    incomplete["response_metadata"]["next_cursor"] = json!("next");
    reply.send(Ok(incomplete)).unwrap();
    settle(&mut dom, || {
        io.completed.get() == 11 && action(state, ROOT).is_none()
    })
    .await;
    assert_eq!(shown(state).len(), 10);
    assert!(
        text(captured.borrow().as_ref().unwrap()).contains("Open the thread to read more replies.")
    );
    assert_eq!(
        io.reads.borrow().iter().filter(|(r, _)| r == ROOT).count(),
        11
    );
    assert!(!state.peek().core.thread_open && state.peek().core.profile_pane.is_none());
}

mod network_profiles;
mod paging;
