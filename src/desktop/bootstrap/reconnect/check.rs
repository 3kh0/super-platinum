//! Mounted, disconnected acceptance of the actual reconnect continuation.
use super::fixture;
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
mod support;
use support::*;
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled(&state.peek()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        eprintln!("reconnect-thread-continuation-check: {result}");
    });
}
async fn thread(
    desktop: &dioxus::desktop::DesktopContext,
    root: &str,
    reply: &str,
) -> Result<Value, String> {
    stable(
        desktop,
        &format!("thread({},{})", json!(root), json!(reply)),
    )
    .await
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: &mut usize,
) -> Result<(), String> {
    let g = thread(desktop, fixture::ROOT, fixture::CACHED).await?;
    stage(state, name, ordinal, g).await
}
async fn hold(
    mut state: Signal<ShellState>,
    phase: &str,
    target: Option<&str>,
) -> Result<usize, String> {
    fixture::reset(&mut state.write(), target);
    fixture::arm(phase);
    spawn(super::reload_after_outage(state));
    held(phase).await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut ordinal = 0;
    capture(state, desktop, "initial", &mut ordinal).await?;
    fixture::reset(&mut state.write(), Some(fixture::TARGET));
    let generation = state.peek().thread_generation;
    super::reload_after_outage(state).await;
    let request = fixture::requests()
        .last()
        .cloned()
        .ok_or("No continued reply read")?;
    ensure(
        request["phase"] == "thread"
            && request["requestedTarget"] == fixture::TARGET
            && request["accepted"] == true
            && state.peek().thread_generation == generation,
        "Reconnect lost target or began a new visit",
    )?;
    let g = thread(desktop, fixture::ROOT, fixture::TARGET).await?;
    stage(state, "exact-target", &mut ordinal, g).await?;

    let id = hold(state, "history", Some("1750000900.000099")).await?;
    ensure(
        state.peek().thread_target.as_deref() == Some("1750000900.000099"),
        "Pending manual target was already consumed",
    )?;
    let action = dom(desktop, "clear()").await?;
    wait(state, |s| s.thread_target.is_none()).await?;
    release(state, id, true).await?;
    wait(state, |_| {
        fixture::requests()
            .last()
            .is_some_and(|r| r["phase"] == "thread" && r["accepted"] == true)
    })
    .await?;
    ensure(
        fixture::requests().last().unwrap()["requestedTarget"].is_null(),
        "Reconnect restored a cleared target",
    )?;
    let mut g = thread(desktop, fixture::ROOT, fixture::CACHED).await?;
    g["manualNavigation"] = action;
    stage(state, "manual-clear", &mut ordinal, g).await?;

    let id = hold(state, "surface", None).await?;
    dom(desktop, "click('.thread-close')").await?;
    wait(state, |s| s.thread_root.is_none()).await?;
    release(state, id, false).await?;
    let g = stable(desktop, "surface('.rail button[title=Home]')").await?;
    stage(state, "close-wins", &mut ordinal, g).await?;

    let id = hold(state, "history", None).await?;
    crate::bootstrap::open_thread_at(
        state,
        fixture::CHANNEL.into(),
        fixture::OTHER.into(),
        Some(fixture::OTHER_REPLY.into()),
    )
    .await;
    release(state, id, false).await?;
    ensure(
        state.peek().thread_root.as_deref() == Some(fixture::OTHER),
        "Old reconnect replaced newer thread",
    )?;
    let g = thread(desktop, fixture::OTHER, fixture::OTHER_REPLY).await?;
    stage(state, "newer-thread-wins", &mut ordinal, g).await?;

    let id = hold(state, "surface", None).await?;
    dom(desktop, "click('.rail button[title=Later]')").await?;
    wait(state, |s| {
        s.main_view == MainView::Later && s.thread_root.is_none()
    })
    .await?;
    release(state, id, false).await?;
    let g = stable(desktop, "surface('.rail button[title=Later]')").await?;
    stage(state, "departure-wins", &mut ordinal, g).await?;

    let id = hold(state, "history", None).await?;
    state.write().open_palette();
    let g = stable(desktop, "surface('#shell-overlay-dialog')").await?;
    stage(state, "shortcut-open", &mut ordinal, g).await?;
    let action = dom(desktop, "dismissPalette()").await?;
    wait(state, |s| s.overlay.is_none()).await?;
    release(state, id, false).await?;
    ensure(
        !state.peek().thread_loading,
        "Retired reconnect left a busy thread",
    )?;
    let mut g = thread(desktop, fixture::ROOT, fixture::CACHED).await?;
    g["dismissal"] = action;
    stage(state, "shortcut-wins", &mut ordinal, g).await?;

    let id = hold(state, "thread", None).await?;
    dom(desktop, "click('.thread-close')").await?;
    wait(state, |s| s.thread_root.is_none()).await?;
    release(state, id, false).await?;
    let g = stable(desktop, "surface('.rail button[title=Home]')").await?;
    stage(state, "reply-close-wins", &mut ordinal, g).await?;

    let id = hold(state, "thread", None).await?;
    let generation = state.peek().thread_generation;
    super::reload_after_outage(state).await;
    release(state, id, false).await?;
    ensure(
        state.peek().thread_generation == generation && !state.peek().thread_loading,
        "Newer refresh changed the visit or left it busy",
    )?;
    capture(state, desktop, "newer-refresh-wins", &mut ordinal).await?;

    fixture::reset_failure(&mut state.write());
    let cache = fixture::snapshot(&state.peek())["cache"].clone();
    fixture::fail_next_thread();
    super::reload_after_outage(state).await;
    ensure(
        fixture::snapshot(&state.peek())["cache"] == cache
            && state.peek().thread_error.is_some()
            && state.peek().thread_target.as_deref() == Some(fixture::TARGET),
        "Failed refresh discarded cached replies",
    )?;
    let g = stable(
        desktop,
        &format!(
            "failure({},{})",
            json!(fixture::ROOT),
            json!(fixture::CACHED)
        ),
    )
    .await?;
    stage(state, "failure", &mut ordinal, g).await?;
    dom(desktop, "click('.thread-load-notice button')").await?;
    wait(state, |s| s.thread_error.is_none() && !s.thread_loading).await?;
    let recovered = fixture::snapshot(&state.peek())["cache"].clone();
    let request = fixture::requests()
        .last()
        .cloned()
        .ok_or("No retry request")?;
    ensure(
        request["requestedTarget"] == fixture::TARGET && request["accepted"] == true,
        "Retry lost the pending exact reply",
    )?;
    ensure(
        recovered[0]["messages"]
            .as_array()
            .unwrap()
            .starts_with(cache[0]["messages"].as_array().unwrap())
            && recovered[1] == cache[1],
        "Retry replaced cached reply content",
    )?;
    let g = thread(desktop, fixture::ROOT, fixture::TARGET).await?;
    stage(state, "retry", &mut ordinal, g).await?;
    let cache = recovered;

    let id = hold(state, "history", None).await?;
    {
        let mut s = state.write();
        s.core.account_epoch += 1;
        s.core.active_account = Some("reconnect-thread-retired".into());
        s.close_thread();
    }
    release(state, id, false).await?;
    ensure(
        fixture::snapshot(&state.peek())["cache"] == cache,
        "Retired account mutated cached replies",
    )?;
    let g = stable(desktop, "surface('.rail button[title=Home]')").await?;
    stage(state, "account-wins", &mut ordinal, g.clone()).await?;
    ensure(
        fixture::requests()
            .iter()
            .all(|r| r["released"] == true && !r["accepted"].is_null()),
        "Unresolved reconnect read",
    )?;
    let history_guards = super::history_gate::fixture_checks(&state.peek())?;
    Ok(
        json!({"ok":true,"historyGuards":history_guards,"stages":ordinal,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"noHttp":true,"noReadAcknowledgement":true,"navigation":fixture::snapshot(&state.peek()),"runs":fixture::runs(),"requests":fixture::requests(),"layout":g,"scope":"Actual reload/continuation and mounted controls; explicit disconnected synthetic surface/history holds and reply results, no live history or read-ACK proof"}),
    )
}
