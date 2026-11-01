use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as support;
use crate::bootstrap::test_support::Harness;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
mod ui;
const C: &str = "CINIT0001";
const ROOT: &str = "1710000000.000001";
const R1: &str = "1710000001.000001";
const R2: &str = "1710000002.000001";
const R3: &str = "1710000003.000001";
fn channel() -> Value {
    json!({"id":C,"name":"warm-harbor","is_channel":true,"is_member":true,"properties":{"tabz":[{"type":"threads","label":"Threads"}]}})
}
fn boot(state: Signal<ShellState>) -> Value {
    let s = state.peek();
    let team = s.core.active_team.as_ref().unwrap();
    let user = &s.core.workspaces[team].self_user_id;
    json!({"ok":true,"self":{"id":user,"name":"fixture-self","team_id":team,"profile":{}},"team":{"id":team,"name":"Channel threads"},"workspaces":[{"id":team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{}})
}
fn channels() -> Value {
    json!({"ok":true,"channels":{"channels":[channel()],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}})
}
fn flags(on: bool) -> Value {
    json!({"ok":true,"assignments":{"channel_tabs_threads":{"group":if on{"on"}else{"off"}},"search_normalize_visible_external_users":{"group":"off"},"preserve_omitted_user_change_member_fields":{"group":"on"}}})
}
async fn setup(on: bool) -> Harness {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install_people(&mut s);
    support::connect(&mut s).unwrap();
    let initial = support::assignments(&mut s, &flags(on)).unwrap();
    let mut h = crate::message_buttons::inputs::user::directory::read_scope::test_support::mount(s);
    support::provide(&h._dom, Rc::new(support::Renderer::default()));
    support::run_pumped(&mut h._dom, initial.drain(h.state))
        .await
        .unwrap();
    let init = boot(h.state);
    support::run_pumped(
        &mut h._dom,
        initial.complete_channels(h.state, init, channels()),
    )
    .await
    .unwrap();
    assert!(h.state.peek().core.cache.is_none());
    assert!(h.state.peek().overlay.is_none());
    h
}
fn response(text: &str) -> Value {
    json!({"ok":true,"threads":[{
    "root_msg":{"channel":C,"user":"UINIT0001","ts":ROOT,"thread_ts":ROOT,"subscribed":true,"reply_count":147,"text":text,"last_read":R1,
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":{"bold":true}},{"type":"link","text":"Read the notes","url":"https://untrusted.invalid"},{"type":"user","user_id":"UINIT0001"}]}]},{"type":"actions","elements":[{"type":"button","action_id":"do-not-run","text":{"type":"plain_text","text":"Unsafe action"},"url":"https://untrusted.invalid/action"}]}],
        "attachments":[{"title":"Launch notes","title_link":"https://untrusted.invalid/title","author_name":"Avery","author_link":"https://untrusted.invalid/author","text":"Attachment context","footer":"Retained footer"}],
        "files":[{"id":"F00000001","title":"launch.pdf","filetype":"pdf","mimetype":"application/pdf","url_private":"https://untrusted.invalid/file"}]},
    "unread_replies":[{"ts":R3,"user":"UINIT0001","text":"unread preferred","_hidden_reply":true}],
    "latest_replies":[{"ts":R1,"user":"UINIT0001","text":"Read reply"},{"ts":R2,"user":"UINIT0001","text":"hidden stub","_hidden_stub":true},{"ts":R3,"text":"duplicate later"}]
}],"has_more":true})
}
#[tokio::test]
async fn original_assignments_joined_canonical_inventory_and_postboot_receipts_admit_only_typed_route()
 {
    fn send<T: Send>() {}
    send::<ShellState>();
    let absent = ShellState::fixture(crate::media::MediaRegistry::default());
    assert!(
        Route::capture(
            &absent,
            &Inventory::from_channel(&channel()).unwrap(),
            "threads"
        )
        .is_none()
    );
    let off = setup(false).await;
    assert!(
        Route::capture(
            &off.state.peek(),
            &super::super::inventory(&off.state.peek()).unwrap(),
            "threads"
        )
        .is_none()
    );
    let mut h = setup(true).await;
    let mut state = h.state;
    let team = state.peek().core.active_team.clone().unwrap();
    let base = super::super::inventory(&state.peek()).unwrap();
    let route = Route::capture(&state.peek(), &base, "threads")
        .expect("original successful assignment/canonical joined row");
    assert!(route.current(&state.peek()));
    assert!(!super::super::selection::record(
        &mut state.write(),
        super::super::selection::Route::Resource(crate::state::ChannelTab::Threads),
        None
    ));
    let boot = crate::app_home::cfm_lifecycle::boot_started(&mut state.write(), &team);
    assert!(!route.current(&state.peek()));
    assert!(Route::capture(&state.peek(), &base, "threads").is_none());
    crate::app_home::cfm_lifecycle::boot_settled(&mut state.write(), boot);
    assert!(
        !route.current(&state.peek()),
        "settled equal boot cannot revive original receipt"
    );
    let fresh = Route::capture(&state.peek(), &base, "threads").unwrap();
    assert_ne!(fresh, route);
    let replacement = support::assignments(&mut state.write(), &flags(true)).unwrap();
    assert!(!fresh.current(&state.peek()));
    let init = boot_data(state);
    support::run_pumped(
        &mut h._dom,
        replacement.complete_channels(state, init, channels()),
    )
    .await
    .unwrap();
    let current = Route::capture(&state.peek(), &base, "threads").unwrap();
    assert!(current.current(&state.peek()));
    assert!(!fresh.current(&state.peek()));
    for raw in [
        json!({"id":C,"is_im":true,"is_channel":true,"is_member":true}),
        json!({"id":C,"is_mpim":true,"is_private":true,"is_member":true}),
        json!({"id":C,"is_channel":true}),
        json!({"id":C,"is_channel":true,"is_member":false}),
    ] {
        assert!(!gate::ordinary_joined(&raw, C));
    }
}
fn boot_data(state: Signal<ShellState>) -> Value {
    boot(state)
}

#[tokio::test]
async fn actual_channel_tab_controls_read_canonical_rich_context_hold_refresh_retry_and_owned_open_thread()
 {
    let h = setup(true).await;
    let mut u = ui::mount(h);
    let state = u.state;
    let io = u.io.clone();
    let first = io.expect();
    u.click("data-channel-tab-id", "threads");
    u.pump(|| io.calls.get() == 1).await;
    assert!(state.peek().channel_tabs.threads.loading());
    assert!(state.peek().core.threads.is_empty());
    first.send(Ok(response("Initial root"))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| s.loaded() && !s.loading()))
        .await;
    let key = (
        state.peek().core.active_team.clone().unwrap(),
        C.into(),
        ROOT.into(),
    );
    assert_eq!(state.peek().core.threads[&key].messages.len(), 4);
    assert_eq!(loaded(&state.peek())[0].replies.len(), 2);
    assert_eq!(
        loaded(&state.peek())[0].replies[1].text.as_deref(),
        Some("unread preferred")
    );
    assert!(state.peek().core.threads_view.items.is_empty());
    assert!(state.peek().viewer.is_none());
    // Only tab navigation/manager, Refresh and Open thread may carry clicks;
    // author/avatar/body/file/button/attachment descendants add none.
    assert_eq!(u.click_listener_count(), 6);
    u.double_click("data-message-id", ROOT);
    assert!(io.opens.borrow().is_empty());
    let second = io.expect();
    u.click("id", "channel-threads-refresh");
    u.pump(|| io.calls.get() == 2).await;
    assert!(visible(&state.peek()).unwrap().loading());
    assert_eq!(
        state.peek().core.threads[&key].messages[0].text.as_deref(),
        Some("Initial root")
    );
    assert_eq!(loaded(&state.peek()).len(), 1);
    second
        .send(Err(Error::Offline("synthetic outage".into())))
        .unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && !s.notice.is_empty()))
        .await;
    assert_eq!(loaded(&state.peek()).len(), 1);
    let retry = io.expect();
    u.click("id", "channel-threads-retry");
    u.pump(|| io.calls.get() == 3).await;
    retry.send(Ok(response("Recovered root"))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && s.notice.is_empty()))
        .await;
    assert_eq!(
        state.peek().core.threads[&key].messages[0].text.as_deref(),
        Some("Recovered root")
    );
    u.click("data-open-thread", ROOT);
    u.pump(|| io.opens.borrow().len() == 1).await;
    assert_eq!(state.peek().thread_root.as_deref(), Some(ROOT));
    assert_eq!(
        io.opens.borrow()[0],
        (ROOT.into(), state.peek().thread_scope())
    );
    assert_eq!(io.calls.get(), 3);
    assert!(state.peek().viewer.is_none());
}
#[tokio::test]
async fn held_response_rejects_actual_canonical_edit_and_preserves_loaded_discussion_until_explicit_refresh()
 {
    let h = setup(true).await;
    let mut u = ui::mount(h);
    let mut state = u.state;
    let io = u.io.clone();
    let first = io.expect();
    u.click("data-channel-tab-id", "threads");
    u.pump(|| io.calls.get() == 1).await;
    first.send(Ok(response("Original"))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| s.loaded() && !s.loading()))
        .await;
    let held = io.expect();
    u.click("id", "channel-threads-refresh");
    u.pump(|| io.calls.get() == 2).await;
    let key = (
        state.peek().core.active_team.clone().unwrap(),
        C.into(),
        ROOT.into(),
    );
    state.write().core.threads.get_mut(&key).unwrap().messages[0].text =
        Some("newer local edit".into());
    held.send(Ok(response("stale server"))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && !s.notice.is_empty()))
        .await;
    assert_eq!(
        state.peek().core.threads[&key].messages[0].text.as_deref(),
        Some("newer local edit")
    );
    assert!(visible(&state.peek()).unwrap().loaded());
    let next = io.expect();
    u.click("id", "channel-threads-refresh");
    u.pump(|| io.calls.get() == 3).await;
    next.send(Ok(response("Explicit fresh read"))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && s.notice.is_empty()))
        .await;
    assert_eq!(
        state.peek().core.threads[&key].messages[0].text.as_deref(),
        Some("Explicit fresh read")
    );
}
#[tokio::test]
async fn rate_limit_wake_enables_explicit_retry_without_automatic_read() {
    let h = setup(true).await;
    let mut u = ui::mount(h);
    let state = u.state;
    let io = u.io.clone();
    let first = io.expect();
    u.click("data-channel-tab-id", "threads");
    u.pump(|| io.calls.get() == 1).await;
    first
        .send(Err(Error::RateLimited {
            retry_after_secs: Some(1),
        }))
        .unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && s.retry_at.is_some()))
        .await;
    let before = std::time::Instant::now();
    let source = Source::capture(&state.peek()).unwrap();
    u.dom
        .in_scope(dioxus::core::ScopeId::ROOT, || source.start(state));
    u.pump(|| visible(&state.peek()).is_some_and(|s| s.retry_at.is_none()))
        .await;
    assert!(before.elapsed() >= std::time::Duration::from_millis(900));
    assert_eq!(
        io.calls.get(),
        1,
        "wake enables UI only, never retries automatically"
    );
    let retry = io.expect();
    u.click("id", "channel-threads-retry");
    u.pump(|| io.calls.get() == 2).await;
    retry.send(Ok(response("after rate limit"))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| s.loaded() && !s.loading()))
        .await;
}

#[tokio::test]
async fn actual_same_tab_reentry_retired_429_and_queued_peer_keep_original_account_cooldown() {
    let h = setup(true).await;
    let mut u = ui::mount(h);
    let mut state = u.state;
    let io = u.io.clone();
    let retired = io.expect();
    u.click("data-channel-tab-id", "threads");
    u.pump(|| io.calls.get() == 1).await;
    let queued_gate = io.hold_dispatch();
    let never_sent = io.expect();
    let old_generation = state.peek().channel_generation;
    u.click("data-channel-tab-id", "threads");
    u.pump(|| io.gated.get() == 1).await;
    assert!(state.peek().channel_generation > old_generation);
    assert_eq!(io.calls.get(), 1);
    retired
        .send(Err(Error::RateLimited {
            retry_after_secs: Some(1),
        }))
        .unwrap();
    u.pump(|| io.completed.get() == 1 && limit_status(&state.peek()).cooling())
        .await;
    queued_gate.send(()).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| !s.loading() && !s.notice.is_empty()))
        .await;
    assert_eq!(
        io.calls.get(),
        1,
        "queued peer is refused at dispatch, not after a second HTTP call"
    );
    assert!(
        never_sent
            .send(Ok(response("must not be consumed")))
            .is_err()
    );
    assert!(state.peek().core.threads.is_empty());
    let before = state.peek().channel_generation;
    u.click("data-channel-tab-id", "threads");
    u.pump(|| {
        state.peek().channel_generation > before
            && visible(&state.peek()).is_some_and(|s| !s.loading() && !s.notice.is_empty())
    })
    .await;
    assert!(limit_status(&state.peek()).cooling());
    assert_eq!(
        io.calls.get(),
        1,
        "fresh positive same-tab gate cannot clear original account throttle"
    );
    u.pump(|| !limit_status(&state.peek()).cooling()).await;
    assert_eq!(
        io.calls.get(),
        1,
        "expiry and keyed UI rerender never mount a read"
    );
    let retry = io.expect();
    u.click("id", "channel-threads-retry");
    u.pump(|| io.calls.get() == 2).await;
    retry.send(Ok(response("explicit after cooldown"))).unwrap();
    u.pump(|| visible(&state.peek()).is_some_and(|s| s.loaded() && !s.loading()))
        .await;
    assert_eq!(io.calls.get(), 2);
    assert_eq!(
        u.click_listener_count(),
        6,
        "scoped tab controls remain present after repeated visits"
    );
    // A same-key replacement transport/credential owner receives no observation
    // from the predecessor. This is throttle metadata only, never read authority.
    let original = Source::capture(&state.peek()).unwrap();
    let original_owner = limits::Owner::from_gate(&original.route.gate);
    let original_ticket = limits::reserve(&mut state.write(), original_owner).unwrap();
    let foreign = setup(true).await;
    let mut replacement = foreign.state;
    replacement.write().channel_tabs.thread_limits =
        std::mem::take(&mut state.write().channel_tabs.thread_limits);
    limits::observe(
        replacement,
        &original_ticket,
        &Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    let base = super::super::inventory(&replacement.peek()).unwrap();
    let route = Route::capture(&replacement.peek(), &base, "threads").unwrap();
    let owner = limits::Owner::from_gate(&route.gate);
    let fresh = limits::reserve(&mut replacement.write(), owner)
        .expect("replacement original owner is not throttled by retired response");
    assert!(limits::allows(&replacement.peek(), &fresh));
    limits::release(&mut replacement.write(), &fresh);
}

mod previous;

mod recovery;
