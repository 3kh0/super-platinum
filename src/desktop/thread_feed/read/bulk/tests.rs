use super::*;
use crate::resource_reads::render_test::{button, click, mount, text};
use super_platinum_core::slack::models::ThreadCounts;

fn render(state: Signal<ShellState>) -> Element {
    crate::view::thread_list_for_test(state, &state.read())
}
fn page(state: Signal<ShellState>, unread: bool) -> ThreadsViewPage {
    ThreadsViewPage {
        threads: state.peek().core.threads_view.items.clone(),
        threads_state: Some(ThreadCounts {
            has_unreads: unread,
            ..Default::default()
        }),
        has_more: true,
        ..Default::default()
    }
}
async fn publish(state: Signal<ShellState>, unread: bool) {
    let source = Source::capture(&state.peek()).unwrap();
    let page = page(state, unread);
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async { Ok(page) })
            .await
            .is_some()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn actual_mark_all_handler_preserves_loaded_rows_on_uncertain_result_then_refresh_and_acknowledged_state()
 {
    let (mut dom, mut state, captured) = mount("thread-feed-mark-all", render);
    state.write().overlay = None;
    dom.render_immediate_to_vec();
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "thread-feed-mark-all-read"
        )
        .unwrap()
        .disabled
    );
    assert!(
        Action::capture(&state.peek()).is_none(),
        "public fixture data cannot authorize a write"
    );
    publish(state, true).await;
    dom.render_immediate_to_vec();
    let control = button(
        captured.borrow().as_ref().unwrap(),
        "thread-feed-mark-all-read",
    )
    .unwrap();
    assert!(!control.disabled);
    let before = {
        let shell = state.peek();
        (
            serde_json::to_value(&shell.core.threads_view.items).unwrap(),
            shell.thread_scope(),
            shell.core.composer.text.clone(),
        )
    };
    let original = Action::capture(&state.peek()).unwrap();
    click(&dom, control);
    assert_eq!(status(&state.peek()), Some("Marking threads read…"));
    assert!(
        original.begin(state).is_none(),
        "handler reserves before spawn"
    );
    dom.render_immediate_to_vec();
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "thread-feed-mark-all-read"
        )
        .unwrap()
        .disabled
    );
    // The actual production handler uses the disconnected in-memory transport.
    // It must report uncertainty without clearing any loaded row or count.
    for _ in 0..16 {
        dom.render_immediate_to_vec();
        tokio::task::yield_now().await;
    }
    assert!(
        status(&state.peek())
            .unwrap()
            .starts_with("Could not confirm")
    );
    assert!(Action::capture(&state.peek()).is_none());
    assert!(has_unreads(&state.peek()));
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        before.0
    );
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Refresh before trying again."));
    publish(state, true).await;
    assert!(status(&state.peek()).is_none());
    let action = Action::capture(&state.peek()).unwrap();
    let lease = action.begin(state).unwrap();
    assert!(
        action
            .run_with(state, lease, |request| async move {
                assert!(request.current());
                assert_eq!(request.action.source.team, "T1");
                assert_eq!(
                    request.action.source.session.token,
                    "synthetic-resource-read"
                );
                Ok(())
            })
            .await
    );
    assert!(
        has_unreads(&state.peek()),
        "acknowledgement does not invent a zero count"
    );
    let response = page(state, false);
    assert!(
        action
            .source
            .run_with(state, Mode::Refresh, |_| async { Ok(response) })
            .await
            .is_some()
    );
    dom.render_immediate_to_vec();
    assert!(!has_unreads(&state.peek()));
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "thread-feed-mark-all-read"
        )
        .is_none()
    );
    assert_eq!(state.peek().thread_scope(), before.1);
    assert_eq!(state.peek().core.composer.text, before.2);
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        before.0
    );
}

#[tokio::test(flavor = "current_thread")]
async fn queued_cancel_requires_full_refresh_and_retired_inflight_cleanup_cannot_release_successor()
{
    use crate::bootstrap::test_support::pending;
    let (mut dom, mut state, _) = mount("thread-feed-mark-all", render);
    state.write().overlay = None;
    publish(state, true).await;
    let queued = Action::capture(&state.peek()).unwrap();
    let lease = queued.begin(state).unwrap();
    {
        let held_read = state.peek();
        drop(lease);
        assert!(status(&held_read).unwrap().starts_with("Could not confirm"));
    }
    assert!(
        status(&state.peek())
            .unwrap()
            .starts_with("Could not confirm")
    );
    let older = Source::capture(&state.peek()).unwrap();
    assert!(
        older
            .run_with(state, Mode::Older, |_| async {
                Ok(ThreadsViewPage::default())
            })
            .await
            .is_some()
    );
    assert!(
        Action::capture(&state.peek()).is_none(),
        "append is not a full reconciliation"
    );
    publish(state, true).await;
    let action = Action::capture(&state.peek()).unwrap();
    let lease = action.begin(state).unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let mut work = Box::pin(action.run_with(state, lease, |request| async move {
        assert!(request.current());
        rx.await.unwrap();
        assert!(!request.current());
        Ok(())
    }));
    pending(work.as_mut()).await;
    state.write().core.active_account = Some("successor-account".into());
    publish(state, true).await;
    let next = Action::capture(&state.peek()).unwrap();
    let next_lease = next.begin(state).unwrap();
    let successor = next_lease.operation.clone();
    tx.send(()).unwrap();
    assert!(!work.await);
    assert!(successor.owns(&state.peek()));
    assert_eq!(status(&state.peek()), Some("Marking threads read…"));
    assert!(has_unreads(&state.peek()));
    assert!(next.run_with(state, next_lease, |_| async { Ok(()) }).await);
    assert!(status(&state.peek()).is_none());
    // A queued original action cannot dispatch under a replacement account.
    assert!(queued.begin(state).is_none());
    dom.render_immediate_to_vec();
}
