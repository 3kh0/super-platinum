use super::*;
use crate::bootstrap::test_support::{harness, pending};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn page(ts: &str, cursor: Option<&str>) -> ActivityFeedPage {
    serde_json::from_value(serde_json::json!({"items":[{
        "key":format!("mention-{ts}"),"feed_ts":ts,"is_unread":true,
        "item":{"type":"at_user","message":{"channel":"C1","ts":ts}}
    }],"response_metadata":{"next_cursor":cursor}}))
    .unwrap()
}
fn setup(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    shell.main_view = MainView::Activity;
    shell.overlay = None;
    shell.toast = None;
    shell.core.activity = Default::default();
    let epoch = shell.core.account_epoch;
    shell.core.activity.ensure_scope(epoch, "T1");
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .activity_unread_count = Some(100);
}
fn source(state: Signal<ShellState>) -> Source {
    Source::capture(&state.peek()).unwrap()
}
fn offline() -> Error {
    Error::Offline("synthetic offline".into())
}

#[tokio::test(flavor = "current_thread")]
async fn failed_unread_fill_reconnect_retries_exact_opaque_page_once_and_preserves_loaded_read_state()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let observed = calls.clone();
    source(state)
        .run_with(state, Mode::Refresh, move |request| {
            assert_eq!(request.transport.d_cookie(), "synthetic");
            assert_eq!(request.session.token, "synthetic");
            assert_eq!(request.session.team_id, "T1");
            observed.borrow_mut().push(request.cursor);
            let index = observed.borrow().len();
            async move {
                match index {
                    1 => Ok(page("3.0", Some("first/+?=&"))),
                    2 => Ok(page("2.0", Some("second/+?=&"))),
                    3 => Err(offline()),
                    _ => panic!("failure must stop the initial fill"),
                }
            }
        })
        .await;
    assert_eq!(
        *calls.borrow(),
        vec![None, Some("first/+?=&".into()), Some("second/+?=&".into())]
    );
    assert_eq!(state.peek().core.activity.items.len(), 2);
    {
        let mut shell = state.write();
        shell.core.activity.mark_item_read("mention-3.0");
        shell.core.activity.selected = Some("mention-3.0".into());
        shell.core.activity.unread_only = true;
    }
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let resumed = Source::capture_with(&state.peek(), Some(visit)).unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let (release, held) = tokio::sync::oneshot::channel();
    let mut held = Some(held);
    let mut work = Box::pin(resumed.run_with(state, Mode::Resume, move |request| {
        observed.set(observed.get() + 1);
        assert_eq!(request.cursor.as_deref(), Some("second/+?=&"));
        let receiver = held.take().expect("one admitted failed-page retry");
        async move { receiver.await.unwrap() }
    }));
    pending(work.as_mut()).await;
    let later = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    assert!(!resumed.visit_current(&state.peek()));
    assert!(resumed.admitted_current(&state.peek()));
    let replacement = Source::capture_with(&state.peek(), Some(later)).unwrap();
    replacement
        .run_with(state, Mode::Resume, |_| async {
            panic!("a later restore must not duplicate the admitted read")
        })
        .await;
    release.send(Ok(page("1.0", Some("third/+?=&")))).unwrap();
    work.await;
    assert_eq!(
        calls.get(),
        1,
        "recovery has no invented unread-fill continuation budget"
    );
    let shell = state.peek();
    assert_eq!(shell.core.activity.items.len(), 3);
    assert_eq!(
        shell.core.activity.next_cursor.as_deref(),
        Some("third/+?=&")
    );
    assert_eq!(shell.core.activity.selected.as_deref(), Some("mention-3.0"));
    assert!(
        !shell
            .core
            .activity
            .items
            .iter()
            .find(|i| i.key == "mention-3.0")
            .unwrap()
            .is_pending()
    );
    assert!(shell.core.activity.unread_only);
    assert!(!shell.core.activity.failed && !shell.core.activity.loading);
    assert!(shell.core.activity.failed_read().is_none());
    assert_eq!(shell.core.composer.text, "Keep draft");
}

#[tokio::test(flavor = "current_thread")]
async fn refresh_retry_uses_page_one_with_retained_older_cursor_and_repeated_failure_never_loops() {
    let h = harness();
    let mut state = h.state;
    setup(state);
    {
        let mut shell = state.write();
        let first = shell.core.activity.begin(false).unwrap();
        shell
            .core
            .activity
            .apply_page(&first, page("2.0", Some("older")));
    }
    source(state)
        .run_with(state, Mode::Refresh, |r| {
            assert!(r.cursor.is_none());
            async { Err(offline()) }
        })
        .await;
    assert_eq!(state.peek().core.activity.items[0].feed_ts, "2.0");
    assert_eq!(
        state.peek().core.activity.next_cursor.as_deref(),
        Some("older")
    );
    assert!(
        state
            .peek()
            .core
            .activity
            .failed_read()
            .unwrap()
            .cursor
            .is_none()
    );
    let rendered = source(state);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    rendered
        .run_with(state, Mode::Retry, move |r| {
            observed.set(observed.get() + 1);
            assert!(r.cursor.is_none());
            async { Err(offline()) }
        })
        .await;
    assert_eq!(calls.get(), 1);
    rendered
        .run_with(state, Mode::Retry, |_| async {
            panic!("old rendered stamp cannot retry a newer failure")
        })
        .await;
    source(state)
        .run_with(state, Mode::Retry, |r| {
            assert!(r.cursor.is_none());
            async { Ok(page("4.0", Some("fresh"))) }
        })
        .await;
    assert_eq!(state.peek().core.activity.items.len(), 1);
    assert_eq!(state.peek().core.activity.items[0].feed_ts, "4.0");
    assert_eq!(
        state.peek().core.activity.next_cursor.as_deref(),
        Some("fresh")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn held_read_token_retirement_and_drop_release_only_original_request_without_visible_failure()
{
    let h = harness();
    let mut state = h.state;
    setup(state);
    let captured = source(state);
    let (release, held) = tokio::sync::oneshot::channel();
    let mut held = Some(held);
    let mut work = Box::pin(captured.run_with(state, Mode::Refresh, move |_| {
        let receiver = held.take().expect("one admitted request");
        async move {
            receiver.await.unwrap();
            Err(offline())
        }
    }));
    pending(work.as_mut()).await;
    {
        let mut shell = state.write();
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token
            .push_str("-replacement");
        shell.toast = None;
    }
    release.send(()).unwrap();
    work.await;
    assert!(!state.peek().core.activity.loading);
    assert!(!state.peek().core.activity.failed);
    assert!(state.peek().core.activity.failed_read().is_none());
    assert!(state.peek().toast.is_none());
    let h = harness();
    let mut state = h.state;
    setup(state);
    let captured = source(state);
    let mut work = Box::pin(captured.run_with(state, Mode::Refresh, |_| async {
        std::future::pending::<Result<ActivityFeedPage, Error>>().await
    }));
    pending(work.as_mut()).await;
    drop(work);
    assert!(!state.peek().core.activity.loading);
    assert!(!state.peek().core.activity.failed);
    let current = state.write().core.activity.begin(false).unwrap();
    assert!(state.peek().core.activity.accepts(&current));
}

#[tokio::test(flavor = "current_thread")]
async fn initial_fill_keeps_five_page_cap_and_original_rail_and_reconnect_admission() {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    source(state)
        .run_with(state, Mode::Refresh, move |_| {
            observed.set(observed.get() + 1);
            let n = observed.get();
            async move { Ok(page(&format!("{n}.0"), Some(&format!("cursor-{n}")))) }
        })
        .await;
    assert_eq!(calls.get(), 5);
    assert_eq!(state.peek().core.activity.items.len(), 5);
    assert_eq!(
        state.peek().core.activity.next_cursor.as_deref(),
        Some("cursor-5")
    );
    let old = source(state);
    {
        let mut s = state.write();
        s.main_view = MainView::Home;
        s.close_thread();
        s.main_view = MainView::Activity;
        s.close_thread();
    }
    old.run_with(state, Mode::Older, |_| async {
        panic!("rail round-trip must retire queued read")
    })
    .await;
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let old = Source::capture_with(&state.peek(), Some(visit)).unwrap();
    crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    old.run_with(state, Mode::Resume, |_| async {
        panic!("retired reconnect must not admit HTTP")
    })
    .await;
    assert_eq!(state.peek().core.activity.items.len(), 5);
}
