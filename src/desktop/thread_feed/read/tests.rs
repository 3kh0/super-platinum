use super::*;
use crate::bootstrap::test_support::{harness, pending};
use super_platinum_core::slack::models::ThreadCounts;

fn page(root: &str, latest: &str, more: bool) -> ThreadsViewPage {
    serde_json::from_value(serde_json::json!({"threads":[{
        "root_msg":{"channel":"C1","ts":root,"text":"Loaded root","latest_reply":latest,"last_read":latest,"reply_count":1},
        "latest_replies":[{"ts":latest,"thread_ts":root,"text":"Loaded reply"}],"unread_replies":[]
    }],"has_more":more,"max_ts":"999999.0"})).unwrap()
}
fn setup(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    shell.main_view = MainView::Threads;
    shell.overlay = None;
    shell.toast = None;
    shell.core.threads_view = Default::default();
    let epoch = shell.core.account_epoch;
    shell.core.threads_view.ensure_scope(epoch, "T1");
}
fn source(state: Signal<ShellState>) -> Source {
    Source::capture(&state.peek()).unwrap()
}
fn offline() -> Error {
    Error::Offline("synthetic offline".into())
}

#[tokio::test(flavor = "current_thread")]
async fn actual_thread_reads_retry_exact_refresh_and_older_coordinates_through_later_restore_and_preserve_read_revision()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    source(state)
        .run_with(state, Mode::Refresh, |r| async move {
            assert!(r.cursor.is_none());
            assert_eq!(r.transport.d_cookie(), "synthetic");
            let mut response = page("10.0", "100.0", true);
            response.threads_state = Some(ThreadCounts {
                has_unreads: true,
                mention_count: 7,
                ..Default::default()
            });
            Ok(response)
        })
        .await;
    assert_eq!(
        state.peek().core.workspaces["T1"]
            .thread_counts
            .snapshot
            .as_ref()
            .unwrap()
            .mention_count,
        7
    );
    state.write().core.threads_view.vip_only = true;
    state.write().core.threads_view.selected = Some(("C1".into(), "10.0".into()));
    let before = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    source(state)
        .run_with(state, Mode::Refresh, |r| async move {
            assert!(r.cursor.is_none());
            Err(offline())
        })
        .await;
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        before
    );
    assert_eq!(
        state.peek().core.threads_view.next_cursor.as_deref(),
        Some("100.0")
    );
    assert!(
        state
            .peek()
            .core
            .threads_view
            .failed_read()
            .unwrap()
            .cursor
            .is_none()
    );
    assert_eq!(
        state.peek().core.threads_view.selected,
        Some(("C1".into(), "10.0".into()))
    );
    source(state)
        .run_with(state, Mode::Retry, |r| async move {
            assert!(r.cursor.is_none());
            Ok(page("20.0", "200.0", true))
        })
        .await;
    assert_eq!(state.peek().core.threads_view.items.len(), 1);
    source(state)
        .run_with(state, Mode::Older, |r| async move {
            assert_eq!(r.cursor.as_deref(), Some("200.0"));
            Err(offline())
        })
        .await;
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let captured = Source::capture_with(&state.peek(), Some(visit)).unwrap();
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(captured.run_with(state, Mode::Resume, |r| async move {
        assert_eq!(r.cursor.as_deref(), Some("200.0"));
        held.await.unwrap()
    }));
    pending(work.as_mut()).await;
    let next = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    Source::capture_with(&state.peek(), Some(next))
        .unwrap()
        .run_with(state, Mode::Resume, |_| async {
            panic!("admitted REST must not duplicate on later restore")
        })
        .await;
    assert!(!captured.visit_current(&state.peek()));
    assert!(captured.admitted_current(&state.peek()));
    {
        let mut shell = state.write();
        let epoch = shell.core.account_epoch;
        shell.core.activity.ensure_scope(epoch, "T1");
        for ts in ["150.0", "190.0"] {
            shell.core.activity.upsert(serde_json::from_value(serde_json::json!({
            "key":ts,"feed_ts":ts,"is_unread":true,"item":{"type":"at_user","message":{"channel":"C1","thread_ts":"15.0","ts":ts}}
        })).unwrap());
        }
        shell
            .core
            .apply_thread_mark(&("T1".into(), "C1".into(), "15.0".into()), "150.0");
    }
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .thread_counts
        .apply_live(ThreadCounts {
            has_unreads: true,
            mention_count: 2,
            ..Default::default()
        });
    let mut response = page("15.0", "190.0", false);
    response.threads_state = Some(ThreadCounts {
        has_unreads: true,
        mention_count: 99,
        ..Default::default()
    });
    release.send(Ok(response)).unwrap();
    work.await;
    let shell = state.peek();
    assert_eq!(shell.core.threads_view.items.len(), 2);
    assert_eq!(
        shell.core.threads_view.next_cursor.as_deref(),
        Some("190.0")
    );
    assert_eq!(shell.core.threads_view.max_ts.as_deref(), Some("999999.0"));
    assert!(shell.core.threads_view.vip_only);
    assert!(!shell.core.threads_view.failed && !shell.core.threads_view.loading);
    assert!(shell.core.threads_view.failed_read().is_none());
    assert!(
        !shell
            .core
            .activity
            .items
            .iter()
            .find(|i| i.key == "150.0")
            .unwrap()
            .is_unread
    );
    assert!(
        shell
            .core
            .activity
            .items
            .iter()
            .find(|i| i.key == "190.0")
            .unwrap()
            .is_unread
    );
    assert_eq!(
        shell.core.workspaces["T1"]
            .thread_counts
            .snapshot
            .as_ref()
            .unwrap()
            .mention_count,
        2,
        "late page count snapshot must not replace intervening live totals"
    );
    assert_eq!(shell.core.composer.text, "Keep draft");
}

#[tokio::test(flavor = "current_thread")]
async fn original_socket_generation_and_credentials_retire_held_errors_quietly_then_allow_a_current_read()
 {
    for socket in [false, true] {
        let h = harness();
        let mut state = h.state;
        setup(state);
        let captured = source(state);
        let (release, held) = tokio::sync::oneshot::channel();
        let mut work = Box::pin(captured.run_with(state, Mode::Refresh, |_| async move {
            held.await.unwrap();
            Err(offline())
        }));
        pending(work.as_mut()).await;
        {
            let mut shell = state.write();
            if socket {
                shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
            } else {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token
                    .push_str("-new");
            }
            shell.toast = None;
        }
        release.send(()).unwrap();
        work.await;
        assert!(!state.peek().core.threads_view.loading && !state.peek().core.threads_view.failed);
        assert!(state.peek().core.threads_view.failed_read().is_none());
        assert!(state.peek().toast.is_none());
        assert!(state.peek().thread_feed_reads.pending.is_none());
        source(state)
            .run_with(state, Mode::Refresh, |_| async {
                Ok(page("10.0", "100.0", false))
            })
            .await;
        assert_eq!(state.peek().core.threads_view.items.len(), 1);
        assert!(state.peek().core.threads_view.loaded);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn dropped_read_releases_owned_busy_slot_and_old_rendered_or_reconnect_sources_cannot_retarget_successor()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let original = source(state);
    let mut work = Box::pin(original.run_with(state, Mode::Refresh, |_| {
        std::future::pending::<Result<ThreadsViewPage, Error>>()
    }));
    pending(work.as_mut()).await;
    drop(work);
    assert!(!state.peek().core.threads_view.loading);
    assert!(state.peek().thread_feed_reads.pending.is_none());
    let successor = source(state);
    let mut work = Box::pin(successor.run_with(state, Mode::Refresh, |_| {
        std::future::pending::<Result<ThreadsViewPage, Error>>()
    }));
    pending(work.as_mut()).await;
    original
        .run_with(state, Mode::Refresh, |_| async {
            panic!("old rendered frame cannot replace successor")
        })
        .await;
    assert!(state.peek().core.threads_view.loading);
    drop(work);
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let retired = Source::capture_with(&state.peek(), Some(visit)).unwrap();
    crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    retired
        .run_with(state, Mode::Resume, |_| async {
            panic!("queued old reconnect cannot admit HTTP")
        })
        .await;
    let retired = source(state);
    {
        let mut shell = state.write();
        shell.main_view = MainView::Home;
        shell.close_thread();
        shell.main_view = MainView::Threads;
        shell.close_thread();
    }
    retired
        .run_with(state, Mode::Refresh, |_| async {
            panic!("old rail visit cannot return as a new read")
        })
        .await;
    source(state)
        .run_with(state, Mode::Refresh, |_| async {
            Ok(page("30.0", "300.0", false))
        })
        .await;
    assert_eq!(
        state.peek().core.threads_view.items[0]
            .root_ts()
            .map(String::as_str),
        Some("30.0")
    );
}
