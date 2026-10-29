use super::*;
use crate::bootstrap::test_support::{harness, pending};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::later::{SavedItem, SavedKey};
fn setup(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    shell.overlay = None;
    shell.main_view = MainView::Later;
    shell.core.later = Default::default();
    let epoch = shell.core.account_epoch;
    shell.core.later.ensure_scope(epoch, "T1");
}
fn frame(state: Signal<ShellState>) -> Arc<Frame> {
    Frame::capture(&state.peek()).unwrap()
}
fn page(ids: &[&str], cursor: &str) -> SavedPage {
    let mut page = SavedPage::default();
    page.response_metadata.next_cursor = cursor.into();
    page.saved_items = ids
        .iter()
        .map(|id| SavedItem {
            item_type: "message".into(),
            item_id: "C1".into(),
            ts: (*id).into(),
            ..Default::default()
        })
        .collect();
    page.counts.uncompleted_count = page.saved_items.len();
    page
}
fn keys(state: Signal<ShellState>) -> Vec<String> {
    state
        .peek()
        .core
        .later
        .visible
        .iter()
        .map(|key| key.ts.clone())
        .collect()
}
async fn first(state: Signal<ShellState>) {
    assert!(
        run_with(
            state,
            frame(state).replace(LaterFilter::Saved),
            |fetch| async move {
                assert!(fetch.cursor.is_none());
                Ok(page(&["1.1"], "opaque+next"))
            }
        )
        .await
        .is_some()
    );
}
#[tokio::test]
async fn append_error_retry_uses_exact_failed_cursor_and_keeps_loaded_rows_without_duplicates() {
    let h = harness();
    let state = h.state;
    setup(state);
    first(state).await;
    let old = frame(state).more().unwrap();
    assert!(
        run_with(state, old.clone(), |fetch| async move {
            assert_eq!(fetch.cursor.as_deref(), Some("opaque+next"));
            Err(Error::RateLimited {
                retry_after_secs: Some(17),
            })
        })
        .await
        .is_none()
    );
    assert_eq!(keys(state), ["1.1"]);
    assert!(state.peek().core.later.loaded);
    assert_eq!(
        state.peek().core.later.next_cursor.as_deref(),
        Some("opaque+next")
    );
    assert!(
        run_with(state, old, |_| async {
            panic!("old rendered source retired")
        })
        .await
        .is_none()
    );
    let retry = frame(state).retry().unwrap();
    assert!(
        run_with(state, retry, |fetch| async move {
            assert_eq!(fetch.filter, LaterFilter::Saved);
            assert_eq!(fetch.cursor.as_deref(), Some("opaque+next"));
            Ok(page(&["1.1", "2.1"], "opaque+last"))
        })
        .await
        .is_some()
    );
    assert_eq!(keys(state), ["1.1", "2.1"]);
    assert_eq!(
        state.peek().core.later.next_cursor.as_deref(),
        Some("opaque+last")
    );
    assert!(frame(state).retry().is_none());
    assert!(state.peek().core.later.error.is_none());
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}
#[tokio::test]
async fn same_filter_refresh_keeps_all_loaded_pages_and_continuation_until_success_replaces_them() {
    let h = harness();
    let state = h.state;
    setup(state);
    first(state).await;
    run_with(state, frame(state).more().unwrap(), |_| async {
        Ok(page(&["2.1"], "retained-end"))
    })
    .await
    .unwrap();
    let before = keys(state);
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(run_with(
        state,
        frame(state).replace(LaterFilter::Saved),
        |fetch| async move {
            assert!(fetch.cursor.is_none());
            receive.await.unwrap();
            Err(Error::Offline("signed?token=NEVER_DISPLAY".into()))
        },
    ));
    pending(work.as_mut()).await;
    assert_eq!(keys(state), before);
    assert!(state.peek().core.later.loaded);
    assert_eq!(
        state.peek().core.later.next_cursor.as_deref(),
        Some("retained-end")
    );
    send.send(()).unwrap();
    assert!(work.await.is_none());
    assert_eq!(keys(state), before);
    assert_eq!(
        state.peek().core.later.next_cursor.as_deref(),
        Some("retained-end")
    );
    assert!(
        !state
            .peek()
            .core
            .later
            .error
            .as_ref()
            .unwrap()
            .contains("NEVER_DISPLAY")
    );
    run_with(state, frame(state).retry().unwrap(), |fetch| async move {
        assert!(fetch.cursor.is_none());
        Ok(page(&["9.1"], "fresh-end"))
    })
    .await
    .unwrap();
    assert_eq!(keys(state), ["9.1"]);
    assert_eq!(
        state.peek().core.later.next_cursor.as_deref(),
        Some("fresh-end")
    );
}
#[tokio::test]
async fn retired_render_and_held_token_read_cannot_fetch_or_clear_replacement_owner_result_or_busy_ticket()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    first(state).await;
    // Discriminating admission: no generation, epoch, account or transport change.
    let admission = frame(state).more().unwrap();
    let generation = state.peek().core.later.generation;
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token
        .push_str("-admission-token");
    assert!(
        run_with(state, admission, |_| async {
            panic!("token-only replacement must refuse admission")
        })
        .await
        .is_none()
    );
    assert_eq!(state.peek().core.later.generation, generation);
    // Discriminating completion: resolve before any successor advances generation.
    let (send, receive) = tokio::sync::oneshot::channel();
    let mut held = Box::pin(run_with(
        state,
        frame(state).more().unwrap(),
        |_| async move {
            receive.await.unwrap();
            let mut page = page(&["forbidden"], "forbidden");
            page.counts.uncompleted_count = 999;
            Ok(page)
        },
    ));
    pending(held.as_mut()).await;
    let generation = state.peek().core.later.generation;
    let before = (
        keys(state),
        state.peek().core.later.next_cursor.clone(),
        state.peek().core.later.counts.uncompleted_count,
        state.peek().core.later.error.clone(),
    );
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token
        .push_str("-completion-token");
    send.send(()).unwrap();
    assert!(held.await.is_none());
    assert_eq!(state.peek().core.later.generation, generation);
    assert_eq!(
        (
            keys(state),
            state.peek().core.later.next_cursor.clone(),
            state.peek().core.later.counts.uncompleted_count,
            state.peek().core.later.error.clone()
        ),
        before
    );
    assert!(
        !state.peek().core.later.loading,
        "only the retired request's own ticket is released"
    );
    let stale = frame(state).more().unwrap();
    let (old_send, old_receive) = tokio::sync::oneshot::channel();
    let mut old = Box::pin(run_with(state, stale.clone(), |_| async move {
        old_receive.await.unwrap();
        Ok(page(&["old"], "old"))
    }));
    pending(old.as_mut()).await;
    state
        .write()
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token
        .push_str("-new");
    assert!(
        run_with(state, stale.clone(), |_| async {
            panic!("no request under replacement credentials")
        })
        .await
        .is_none()
    );
    let (new_send, new_receive) = tokio::sync::oneshot::channel();
    let mut new = Box::pin(run_with(
        state,
        frame(state).replace(LaterFilter::Saved),
        |_| async move {
            new_receive.await.unwrap();
            Ok(page(&["new"], "new-next"))
        },
    ));
    pending(new.as_mut()).await;
    let generation = state.peek().core.later.generation;
    let counts = state.peek().core.later.counts.uncompleted_count;
    let error = state.peek().core.later.error.clone();
    old_send.send(()).unwrap();
    assert!(old.await.is_none());
    assert!(state.peek().core.later.loading);
    assert_eq!(state.peek().core.later.generation, generation);
    assert_eq!(keys(state), ["1.1"]);
    assert_eq!(state.peek().core.later.counts.uncompleted_count, counts);
    assert_eq!(state.peek().core.later.error, error);
    new_send.send(()).unwrap();
    new.await.unwrap();
    assert_eq!(keys(state), ["new"]);
    // Exercise the actual rail journey while suppressing all reads. Restore the
    // identical transport/session before replay; only the real visit clock differs.
    state.write().surfaces.clear();
    let rendered = frame(state).replace(LaterFilter::Saved);
    let generation = state.peek().core.later.generation;
    let original_transport = state.write().core.transport.take();
    crate::bootstrap::load_main_view(state, MainView::Home).await;
    crate::bootstrap::load_main_view(state, MainView::Later).await;
    state.write().core.transport = original_transport;
    assert_eq!(state.peek().core.later.generation, generation);
    assert!(rendered.frame.authority.current(&state.peek()));
    assert!(!rendered.frame.current(&state.peek()));
    assert!(
        run_with(state, rendered, |_| async {
            panic!("round-trip stale control")
        })
        .await
        .is_none()
    );
    let stale = frame(state).replace(LaterFilter::Saved);
    state.write().core.active_account = Some("replacement".into());
    // The actual UI entry refuses the stale source before needing a runtime/task.
    start(state, stale);
    assert_eq!(keys(state), ["new"]);
    assert!(!state.peek().core.later.loading);
}
#[tokio::test]
async fn retry_descriptor_retires_on_filter_change_preview_errors_do_not_create_one_and_cancellation_is_retryable()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    first(state).await;
    run_with(state, frame(state).more().unwrap(), |_| async {
        Err(Error::Offline("synthetic".into()))
    })
    .await;
    let retry = frame(state).retry().unwrap();
    run_with(
        state,
        frame(state).replace(LaterFilter::Completed),
        |_| async { Ok(page(&[], "")) },
    )
    .await
    .unwrap();
    assert!(state.peek().core.later.next_cursor.is_none());
    assert!(
        run_with(state, retry, |_| async {
            panic!("old failed page cannot replay in another filter")
        })
        .await
        .is_none()
    );
    state.write().core.later.error = Some("Preview failed".into());
    assert!(frame(state).retry().is_none());
    run_with(state, frame(state).replace(LaterFilter::Saved), |_| async {
        Ok(page(&["3.1"], "kept"))
    })
    .await
    .unwrap();
    let mut canceled = Box::pin(run_with(state, frame(state).more().unwrap(), |_| {
        std::future::pending::<Result<SavedPage, Error>>()
    }));
    pending(canceled.as_mut()).await;
    assert!(state.peek().core.later.loading);
    drop(canceled);
    assert!(!state.peek().core.later.loading);
    assert_eq!(keys(state), ["3.1"]);
    assert_eq!(state.peek().core.later.next_cursor.as_deref(), Some("kept"));
    assert!(frame(state).more().is_some());
    // A canceled predecessor cannot clear a later accepted reservation.
    let mut predecessor = Box::pin(run_with(
        state,
        frame(state).replace(LaterFilter::Saved),
        |_| std::future::pending::<Result<SavedPage, Error>>(),
    ));
    pending(predecessor.as_mut()).await;
    let mut successor = Box::pin(run_with(
        state,
        frame(state).replace(LaterFilter::Completed),
        |_| std::future::pending::<Result<SavedPage, Error>>(),
    ));
    pending(successor.as_mut()).await;
    drop(predecessor);
    assert!(state.peek().core.later.loading);
    drop(successor);
    assert!(!state.peek().core.later.loading);
}

mod mounted;
