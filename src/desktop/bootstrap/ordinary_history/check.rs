//! Actual mounted ordinary refresh/near-top/retry with a closed page adapter.
use super::fixture;
use crate::state::ShellState;
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
        if let Err(error) = emit("ordinary-history-cursor-check: ", result).await {
            let _ = tokio::task::spawn_blocking(move || {
                eprintln!("ordinary-history-cursor-output-error: {error}");
            })
            .await;
        }
    });
}
fn launch(state: Signal<ShellState>) -> usize {
    let start = fixture::requests().len();
    spawn(crate::bootstrap::refresh_selected_channel(state));
    start
}
async fn loaded(state: Signal<ShellState>, start: usize) -> Result<(), String> {
    wait(state, |_| fixture::requests().len() > start).await?;
    idle(state).await
}
async fn idle(state: Signal<ShellState>) -> Result<(), String> {
    wait(state, |s| {
        s.core
            .active_team
            .as_ref()
            .zip(s.core.active_channel.as_ref())
            .is_some_and(|(t, c)| super::entry(s, t, c).is_some_and(|e| !e.loading))
            && !s.loading_older
    })
    .await
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: &mut usize,
    call: &str,
) -> Result<Value, String> {
    let g = stable(desktop, call).await?;
    stage(state, name, ordinal, g.clone()).await?;
    Ok(g)
}
async fn retry(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    if state.peek().toast.is_some() {
        dom(desktop, "click('#status-toast .toast-dismiss')").await?;
    }
    let start = fixture::requests().len();
    dom(desktop, "click('#ordinary-history-continue')").await?;
    loaded(state, start).await
}
fn has(s: &ShellState, channel: &str, id: usize) -> bool {
    s.core.workspaces["T1"].messages[channel]
        .messages
        .iter()
        .any(|m| m.ts.as_deref() == Some(fixture::ts(id).as_str()))
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut ordinal = 0;
    fixture::batch("C2", 30..54, Some("first"), false);
    let start = launch(state);
    loaded(state, start).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    capture(state, desktop, "short-initial", &mut ordinal, "anchor()").await?;

    fixture::batch("C2", 20..30, Some("second"), true);
    let action = dom(desktop, "nearTop()").await?;
    let id = held(1).await?;
    let before = stable(desktop, "anchor()").await?;
    release(state, id, true).await?;
    idle(state).await?;
    let call = format!(
        "preserved({},{})",
        before["anchor"]["id"], before["anchor"]["offset"]
    );
    let mut g = stable(desktop, &call).await?;
    g["nearTopAction"] = action;
    g["before"] = before;
    stage(state, "prepend", &mut ordinal, g).await?;
    ensure(
        state.peek().core.composer.text == "Unsent draft C2",
        "Prepend changed composer draft",
    )?;

    fixture::error("C2", "fixture_history_failed");
    let start = fixture::requests().len();
    dom(desktop, "nearTop()").await?;
    loaded(state, start).await?;
    capture(state, desktop, "failure", &mut ordinal, "notice()").await?;
    fixture::batch("C2", 10..20, None, false);
    retry(state, desktop).await?;
    capture(state, desktop, "retry", &mut ordinal, "anchor()").await?;

    fixture::reset(&mut state.write(), "C2", 0..0);
    fixture::batch("C2", 0..0, Some("empty-a"), false);
    fixture::batch("C2", 0..0, Some("empty-b"), false);
    fixture::batch("C2", 30..54, None, false);
    let start = launch(state);
    loaded(state, start).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    capture(
        state,
        desktop,
        "empty-initial-complete",
        &mut ordinal,
        "anchor()",
    )
    .await?;

    fixture::reset(&mut state.write(), "C2", 0..0);
    fixture::batch("C2", 30..54, Some("repeat"), false);
    let start = launch(state);
    loaded(state, start).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    fixture::batch("C2", 20..30, Some("repeat"), false);
    let start = fixture::requests().len();
    dom(desktop, "nearTop()").await?;
    loaded(state, start).await?;
    ensure(
        !has(&state.peek(), "C2", 20),
        "Invalid repeated-cursor page was published",
    )?;
    capture(state, desktop, "repeated-cursor", &mut ordinal, "notice()").await?;
    fixture::batch("C2", 20..30, None, false);
    retry(state, desktop).await?;
    capture(state, desktop, "repeated-retry", &mut ordinal, "anchor()").await?;

    fixture::reset(&mut state.write(), "C2", 0..0);
    for i in 0..8 {
        fixture::batch("C2", 0..0, Some(&format!("budget-{i}")), false);
    }
    let start = launch(state);
    loaded(state, start).await?;
    capture(state, desktop, "bounded-paused", &mut ordinal, "notice()").await?;
    fixture::batch("C2", 30..54, None, false);
    retry(state, desktop).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    capture(
        state,
        desktop,
        "bounded-continued",
        &mut ordinal,
        "anchor()",
    )
    .await?;

    fixture::reset(&mut state.write(), "C2", 0..0);
    fixture::batch("C2", 30..54, Some("expired"), false);
    let start = launch(state);
    loaded(state, start).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    fixture::error("C2", "invalid_cursor");
    fixture::batch("C2", 40..54, Some("fresh"), false);
    fixture::batch("C2", 20..40, Some("expired-again"), false);
    // Advertised continuation can be loaded automatically while the reader
    // remains near the top. Its exact response must exist before the trigger;
    // the ledger proves restart20-22 without a transient screenshot barrier.
    fixture::error("C2", "invalid_cursor");
    let start = fixture::requests().len();
    dom(desktop, "nearTop()").await?;
    loaded(state, start).await?;
    if !super::entry(&state.peek(), "T1", "C2").is_some_and(|e| e.restart_required) {
        dom(desktop, "nearTop()").await?;
    }
    wait(state, |s| {
        super::entry(s, "T1", "C2").is_some_and(|e| e.restart_required && !e.loading)
            && !s.loading_older
    })
    .await?;
    capture(state, desktop, "expired-reload", &mut ordinal, "notice()").await?;
    fixture::batch("C2", 10..54, None, false);
    retry(state, desktop).await?;
    capture(state, desktop, "expired-reloaded", &mut ordinal, "anchor()").await?;

    // The older read owns a spinner. A newer initial read on the SAME visit
    // supersedes its ticket without inheriting an orphaned spinner or response.
    fixture::reset(&mut state.write(), "C2", 0..0);
    fixture::batch("C2", 30..54, Some("same-channel"), false);
    let start = launch(state);
    loaded(state, start).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    let start = fixture::requests().len();
    fixture::batch("C2", 20..30, None, true);
    dom(desktop, "nearTop()").await?;
    let id = held(start).await?;
    fixture::batch("C2", 30..54, None, false);
    let start = launch(state);
    loaded(state, start).await?;
    release(state, id, false).await?;
    ensure(
        !has(&state.peek(), "C2", 20) && !state.peek().loading_older,
        "Superseded older read published or stole spinner",
    )?;
    capture(
        state,
        desktop,
        "newer-request-wins",
        &mut ordinal,
        "anchor()",
    )
    .await?;

    fixture::reset(&mut state.write(), "C2", 0..0);
    fixture::batch("C2", 30..54, Some("credential"), false);
    let start = launch(state);
    loaded(state, start).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    let start = fixture::requests().len();
    fixture::batch("C2", 20..30, None, true);
    dom(desktop, "nearTop()").await?;
    let id = held(start).await?;
    fixture::retire_source();
    release(state, id, false).await?;
    wait(state, |s| {
        !s.loading_older && !s.core.workspaces["T1"].messages["C2"].history_refreshing
    })
    .await?;
    ensure(
        !has(&state.peek(), "C2", 20),
        "Retired source capability published history",
    )?;
    capture(state, desktop, "source-retired", &mut ordinal, "anchor()").await?;

    fixture::reset(&mut state.write(), "C2", 0..0);
    let start = fixture::requests().len();
    fixture::batch("C2", 10..34, None, true);
    launch(state);
    let id = held(start).await?;
    fixture::select(&mut state.write(), "C1");
    fixture::batch("C1", 30..54, None, false);
    let current = launch(state);
    loaded(state, current).await?;
    wait(state, |s| s.core.pending_scroll_to.is_none()).await?;
    let before = stable(desktop, "anchor()").await?;
    release(state, id, true).await?;
    ensure(
        has(&state.peek(), "C2", 10) && state.peek().core.active_channel.as_deref() == Some("C1"),
        "Background cache policy changed navigation",
    )?;
    let call = format!(
        "preserved({},{})",
        before["anchor"]["id"], before["anchor"]["offset"]
    );
    capture(state, desktop, "other-channel-wins", &mut ordinal, &call).await?;

    fixture::reset(&mut state.write(), "C2", 0..0);
    let start = fixture::requests().len();
    fixture::batch("C2", 10..34, None, true);
    launch(state);
    let id = held(start).await?;
    {
        let mut s = state.write();
        s.core.account_epoch += 1;
        s.core.active_account = Some("ordinary-history-retired".into());
    }
    release(state, id, false).await?;
    ensure(
        !has(&state.peek(), "C2", 10),
        "Retired account accepted a history page",
    )?;
    let g = capture(state, desktop, "account-wins", &mut ordinal, "surface()").await?;
    ensure(
        fixture::drained(),
        "Controlled history plans or requests remain unclosed",
    )?;
    Ok(
        json!({"ok":true,"stages":ordinal,"noSession":true,"noTransport":true,"noHttp":true,"noReadAcknowledgement":true,"requests":fixture::requests(),"navigation":fixture::snapshot(&state.peek()),"layout":g,"scope":"Actual ordinary async request/file validation/merge/cursor/retry and near-top prepend; controlled pages and disconnected source retirement, no live Slack/credential rotation/server proof"}),
    )
}
