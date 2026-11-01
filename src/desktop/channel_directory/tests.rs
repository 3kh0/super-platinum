use super::*;
use crate::resource_reads::render_test::{button, click, mount, settle, text};
use serde_json::json;

fn render(state: Signal<ShellState>) -> Element {
    if state.read().overlay == Some(Overlay::ChannelDirectory) {
        dialog(state)
    } else {
        crate::view::channel_directory_sidebar_for_test(state)
    }
}
fn raw(page: u32) -> serde_json::Value {
    let cursor = if page == 1 { Some("next-page") } else { None };
    json!({"ok":true,"items":[{"id":format!("C{page}"),"name":format!("release-{page}"),"purpose":"Release planning","member_count":42,"is_member":true}],"pagination":{"page":page,"page_count":2,"total_count":65,"next_cursor":cursor}})
}
async fn publish(entry: &Entry, state: Signal<ShellState>, mode: read::Mode, page: u32) {
    let (request, lease) = entry.begin(state, mode, None).unwrap();
    Entry::run_with(request, lease, |request| async move {
        assert!(request.current());
        assert!(request.query().scope == Scope::Joined);
        assert_eq!(request.query().page, page);
        Ok(Some(api::decode(raw(page), request.query()).unwrap()))
    })
    .await;
}
fn enter(mut state: Signal<ShellState>) -> Entry {
    state.write().overlay = None;
    let launch = Launch::capture(&state.peek()).unwrap();
    launch.enter(state).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn actual_sidebar_open_metadata_search_paging_failure_and_exact_retry_preserve_conversation_and_draft()
 {
    let (mut dom, mut state, captured) = mount("your-channels", render);
    assert!(
        button(captured.borrow().as_ref().unwrap(), "your-channel-C1").is_none(),
        "credential-bearing native harness cannot reuse credential-free presentation data"
    );
    state.write().overlay = None;
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            serde_json::to_value(&s.core.workspaces["T1"].channels).unwrap(),
        )
    };
    dom.render_immediate_to_vec();
    let launch = button(captured.borrow().as_ref().unwrap(), "open-your-channels").unwrap();
    assert!(!launch.disabled);
    click(&dom, launch);
    assert!(
        state.peek().channel_directory.busy(),
        "actual handler reserves before spawn"
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert!(
        state.peek().channel_directory.failed.is_some(),
        "actual disconnected transport reports initial failure"
    );
    let entry = state.peek().channel_directory.entry.clone().unwrap();
    publish(&entry, state, read::Mode::Retry, 1).await;
    dom.render_immediate_to_vec();
    let row = button(captured.borrow().as_ref().unwrap(), "your-channel-C1").unwrap();
    assert!(!row.disabled);
    click(&dom, row);
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("You’re a member"));
    let stale = button(captured.borrow().as_ref().unwrap(), "your-channel-C1").unwrap();
    entry.change(state, Some("release".into()), None);
    click(&dom, stale);
    assert!(state.peek().channel_directory.selected.is_none());
    assert!(state.peek().channel_directory.sort() == Sort::Relevant);
    publish(&entry, state, read::Mode::Search, 1).await;
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), "your-channels-next").unwrap(),
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert_eq!(failed.page, 2);
    assert_eq!(failed.cursor.as_deref(), Some("next-page"));
    assert_eq!(failed.text, "release");
    assert_eq!(
        state
            .peek()
            .channel_directory
            .loaded
            .as_ref()
            .unwrap()
            .query
            .page,
        1
    );
    assert!(
        text(captured.borrow().as_ref().unwrap()).contains("loaded results are still available")
    );
    publish(&entry, state, read::Mode::Retry, 2).await;
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(
            captured.borrow().as_ref().unwrap(),
            "your-channels-previous",
        )
        .unwrap(),
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    let failed = state.peek().channel_directory.failed.clone().unwrap();
    assert_eq!(failed.page, 1);
    assert!(
        failed.cursor.is_none(),
        "previous uses its own retained request coordinate"
    );
    publish(&entry, state, read::Mode::Retry, 1).await;
    let s = state.peek();
    assert_eq!(
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            serde_json::to_value(&s.core.workspaces["T1"].channels).unwrap()
        ),
        before
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stale_callbacks_cancelled_borrows_and_original_rate_family_do_not_mutate_successor() {
    use crate::bootstrap::test_support::pending;
    let (mut dom, mut state, _) = mount("your-channels", render);
    let entry = enter(state);
    let (request, lease) = entry.begin(state, read::Mode::Search, None).unwrap();
    {
        let held = state.peek();
        drop(lease);
        assert!(!held.channel_directory.busy());
    }
    assert!(!request.current());
    let (request, lease) = entry.begin(state, read::Mode::Search, None).unwrap();
    let family = entry.family.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let old_family = family.clone();
    let mut old = Box::pin(Entry::run_with(request, lease, move |request| async move {
        assert!(request.current());
        rx.await.unwrap();
        let result = Err(Error::RateLimited {
            retry_after_secs: Some(60),
        });
        old_family.observe(&result);
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        })
    }));
    pending(old.as_mut()).await;
    entry.change(state, Some("queued peer".into()), None);
    let (peer, peer_lease) = entry.begin(state, read::Mode::Search, None).unwrap();
    tx.send(()).unwrap();
    old.await;
    assert!(!family.ready());
    assert!(peer.current(), "old cleanup cannot release queued peer");
    Entry::run_with(peer, peer_lease, |request| async move {
        assert!(request.current());
        assert!(
            !family.ready(),
            "original observed429 blocks peer dispatch admission"
        );
        Ok(None)
    })
    .await;
    assert!(entry.begin(state, read::Mode::Retry, None).is_none());
    entry.close(state);
    let launch = Launch::capture(&state.peek()).unwrap();
    let reopened = launch.enter(state).unwrap();
    assert!(Arc::ptr_eq(&reopened.family, &entry.family));
    assert!(
        reopened.begin(state, read::Mode::Search, None).is_none(),
        "same-account reentry preserves cooldown"
    );
    reopened.close(state);
    state.write().core.active_account = Some("replacement-account".into());
    let launch = Launch::capture(&state.peek()).unwrap();
    let next = launch.enter(state).unwrap();
    assert!(!Arc::ptr_eq(&next.family, &entry.family));
    assert!(next.family.ready());
    let before = state.peek().channel_directory.revision;
    entry.change(state, Some("old input".into()), None);
    assert_eq!(state.peek().channel_directory.revision, before);
    let (request, lease) = next.begin(state, read::Mode::Search, None).unwrap();
    Entry::run_with(request, lease, |request| async move {
        Ok(Some(api::decode(raw(1), request.query()).unwrap()))
    })
    .await;
    assert!(next.current(&state.peek()));
    dom.render_immediate_to_vec();
}

#[tokio::test(flavor = "current_thread")]
async fn protected_overlay_query_sort_realtime_and_reconnect_retire_only_the_original_read() {
    let (mut dom, mut state, _) = mount("your-channels", render);
    state.write().overlay = None;
    let launch = Launch::capture(&state.peek()).unwrap();
    for protected in [
        Overlay::AppModal,
        Overlay::FileShare,
        Overlay::CanvasCreate,
        Overlay::ButtonConfirmation,
    ] {
        state.write().overlay = Some(protected);
        assert!(launch.enter(state).is_none());
        assert!(state.peek().overlay == Some(protected));
    }
    state.write().overlay = None;
    let entry = launch.enter(state).unwrap();
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    let (request, lease) = entry
        .begin(state, read::Mode::Search, Some(visit.clone()))
        .unwrap();
    let _next = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    assert!(
        request.current(),
        "already admitted original visit survives only restore serial changes"
    );
    drop(lease);
    assert!(
        entry
            .begin(state, read::Mode::Search, Some(visit))
            .is_none(),
        "queued old restore cannot admit a second read"
    );
    let (request, lease) = entry.begin(state, read::Mode::Search, None).unwrap();
    entry.change(state, None, Some(Sort::MostMembers));
    assert!(!request.current());
    drop(lease);
    let (request, lease) = entry.begin(state, read::Mode::Search, None).unwrap();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation += 1;
    assert!(!request.current());
    drop(lease);
    assert!(state.peek().channel_directory.loaded.is_none());
    let (interrupted, old_lease) = entry.begin(state, read::Mode::Search, None).unwrap();
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut state.write());
    dom.in_scope(ScopeId::ROOT, || schedule_after_outage(state, visit));
    assert!(!interrupted.current());
    assert!(state.peek().channel_directory.busy());
    drop(old_lease);
    assert!(
        state.peek().channel_directory.busy(),
        "old cleanup cannot release restoration"
    );
    settle(&mut dom, || !state.peek().channel_directory.busy()).await;
    assert!(state.peek().channel_directory.failed.is_some());
}

mod kind;

mod open;

mod options;

mod retention;

mod reset;
