use super::*;
use crate::channel_tabs::threads::previous as controls;
fn ts(i: usize) -> String {
    format!("{:010}.000001", 1_710_000_000 + i)
}
fn long_response(hidden: bool) -> Value {
    let replies=(1..=200).map(|i|json!({"ts":ts(i),"user":"UINIT0001","text":format!("Cached reply {i}"),"_hidden_stub":hidden&&i==100})).collect::<Vec<_>>();
    json!({"ok":true,"threads":[{"root_msg":{"channel":C,"user":"UINIT0001","ts":ROOT,"thread_ts":ROOT,"subscribed":true,"reply_count":200,"last_read":ts(200),"text":"Read the accepted discussion"},"latest_replies":replies}],"has_more":false})
}
fn ids(first: usize, hidden: bool) -> Vec<String> {
    std::iter::once(ROOT.to_string())
        .chain((first..=200).filter(|i| !hidden || *i != 100).map(ts))
        .collect()
}
#[tokio::test]
async fn generated_previous_replies_expand_owned_200_as_2_102_200_without_reads_and_keep_failed_refresh()
 {
    let mut u = ui::mount(setup(true).await);
    let state = u.state;
    let io = u.io.clone();
    let first = io.expect();
    u.click("data-channel-tab-id", "threads");
    u.pump(|| io.calls.get() == 1).await;
    first.send(Ok(long_response(false))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| s.loaded() && !s.loading()))
        .await;
    assert_eq!(u.message_ids(), ids(199, false));
    let key = (
        state.peek().core.active_team.clone().unwrap(),
        C.into(),
        ROOT.into(),
    );
    let canonical = serde_json::to_value(&state.peek().core.threads[&key].messages).unwrap();
    let stale = controls::capture(&state.peek(), ROOT).unwrap();
    u.click("data-previous-thread", ROOT);
    u.pump(|| u_message_count(state) == 102).await;
    assert_eq!(u.message_ids(), ids(99, false));
    stale.reveal(state);
    assert_eq!(
        u_message_count(state),
        102,
        "an already spent minimum cannot expand again"
    );
    u.click("data-previous-thread", ROOT);
    u.pump(|| u_message_count(state) == 200).await;
    assert_eq!(u.message_ids(), ids(1, false));
    assert_eq!(io.calls.get(), 1);
    assert!(io.opens.borrow().is_empty());
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads[&key].messages).unwrap(),
        canonical
    );
    assert!(controls::capture(&state.peek(), ROOT).is_none());
    let failed = io.expect();
    u.click("id", "channel-threads-refresh");
    u.pump(|| io.calls.get() == 2).await;
    failed
        .send(Err(Error::Offline("synthetic outage".into())))
        .unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && !s.notice.is_empty()))
        .await;
    assert_eq!(
        u.message_ids(),
        ids(1, false),
        "failed refresh keeps the expanded accepted context"
    );
    let refresh = io.expect();
    u.click("id", "channel-threads-retry");
    u.pump(|| io.calls.get() == 3).await;
    refresh.send(Ok(long_response(false))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && s.notice.is_empty()))
        .await;
    assert_eq!(
        u.message_ids(),
        ids(199, false),
        "successful identical read seeds a fresh publication minimum"
    );
    stale.reveal(state);
    assert_eq!(u_message_count(state), 2);
    assert_eq!(io.calls.get(), 3);
}
fn u_message_count(state: Signal<ShellState>) -> usize {
    loaded(&state.peek())[0].replies.len()
}
#[tokio::test]
async fn generated_previous_budget_counts_hidden_owned_timestamps_and_refuses_current_tombstone() {
    let mut u = ui::mount(setup(true).await);
    let mut state = u.state;
    let io = u.io.clone();
    let first = io.expect();
    u.click("data-channel-tab-id", "threads");
    u.pump(|| io.calls.get() == 1).await;
    first.send(Ok(long_response(true))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| s.loaded() && !s.loading()))
        .await;
    assert_eq!(u.message_ids(), ids(199, true));
    u.click("data-previous-thread", ROOT);
    u.pump(|| u_message_count(state) == 101).await;
    assert_eq!(u.message_ids(), ids(99, true));
    assert_eq!(controls::minimum(&state.peek(), ROOT), Some(&Some(ts(99))));
    let queued = controls::capture(&state.peek(), ROOT).unwrap();
    let key = (
        state.peek().core.active_team.clone().unwrap(),
        C.into(),
        ROOT.into(),
    );
    state
        .write()
        .core
        .threads
        .get_mut(&key)
        .unwrap()
        .record_remote_deletion(&ts(50));
    queued.reveal(state);
    assert_eq!(controls::minimum(&state.peek(), ROOT), Some(&Some(ts(99))));
    assert!(controls::capture(&state.peek(), ROOT).is_none());
    assert_eq!(io.calls.get(), 1);
    assert!(io.opens.borrow().is_empty());
}
