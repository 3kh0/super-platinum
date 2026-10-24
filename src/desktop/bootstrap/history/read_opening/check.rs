//! Mounted acceptance of the shared unread loop, using controlled offline pages.
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
        eprintln!("reconnect-unread-opening-check: {result}");
    });
}
fn launch(state: Signal<ShellState>, reconnect: bool) {
    if reconnect {
        spawn(crate::bootstrap::reload_after_outage(state));
    } else {
        spawn(crate::bootstrap::refresh_selected_channel(state));
    }
}
fn final_page(channel: &str) {
    fixture::page(
        channel,
        fixture::FIRST,
        "First unread message",
        false,
        None,
        false,
    );
}
async fn complete(state: Signal<ShellState>) -> Result<(), String> {
    wait(state, |s| {
        s.read_presentation.unread_loading.is_none()
            && s.read_presentation.unread_fetching.is_none()
            && s.core.pending_scroll_to.is_none()
    })
    .await
}
async fn paused(state: Signal<ShellState>) -> Result<(), String> {
    wait(state, |s| {
        s.read_presentation.unread_paused
            && s.read_presentation.unread_loading.is_some()
            && s.read_presentation.unread_fetching.is_none()
    })
    .await
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: &mut usize,
    call: &str,
) -> Result<(), String> {
    let g = stable(desktop, call).await?;
    stage(state, name, ordinal, g).await
}
fn has(s: &ShellState, channel: &str, ts: &str) -> bool {
    s.core.workspaces["T1"].messages[channel]
        .messages
        .iter()
        .any(|m| m.ts.as_deref() == Some(ts))
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut ordinal = 0;
    capture(state, desktop, "initial", &mut ordinal, "cached()").await?;

    fixture::batch("C2", None, true);
    final_page("C2");
    launch(state, false);
    let id = held(0).await?;
    launch(state, true);
    joined(state, 1).await?;
    capture(
        state,
        desktop,
        "ordinary-join",
        &mut ordinal,
        "notice(false)",
    )
    .await?;
    release(state, id, true).await?;
    complete(state).await?;
    ensure(
        fixture::requests().len() == 2,
        "Joined ordinary opening replayed pages",
    )?;
    capture(
        state,
        desktop,
        "ordinary-complete",
        &mut ordinal,
        "anchor()",
    )
    .await?;

    fixture::reset(&mut state.write(), "C2");
    fixture::page(
        "C2",
        "1750001000.000080",
        "Retired reconnect page",
        false,
        None,
        true,
    );
    final_page("C2");
    launch(state, true);
    let id = held(2).await?;
    launch(state, true);
    joined(state, 3).await?;
    capture(
        state,
        desktop,
        "replacement-join",
        &mut ordinal,
        "notice(false)",
    )
    .await?;
    release(state, id, false).await?;
    complete(state).await?;
    ensure(
        !has(&state.peek(), "C2", "1750001000.000080"),
        "Retired reconnect page merged",
    )?;
    capture(
        state,
        desktop,
        "replacement-complete",
        &mut ordinal,
        "anchor()",
    )
    .await?;

    fixture::reset(&mut state.write(), "C2");
    fixture::batch("C2", Some("continuation-1"), false);
    fixture::error("C2");
    launch(state, false);
    paused(state).await?;
    ensure(
        fixture::snapshot(&state.peek())["continuation"]["cursor"] == "continuation-1"
            && has(&state.peek(), "C2", "1750001000.000030")
            && has(&state.peek(), "C2", fixture::CACHED),
        "Failure lost prefix or cursor",
    )?;
    capture(state, desktop, "failure", &mut ordinal, "notice(true)").await?;
    dom(desktop, "click('#status-toast .toast-dismiss')").await?;
    final_page("C2");
    dom(desktop, "click('#unread-opening-continue')").await?;
    complete(state).await?;
    ensure(
        fixture::requests().last().unwrap()["args"]["cursor"] == "continuation-1",
        "Continue replayed prefix",
    )?;
    capture(state, desktop, "continued", &mut ordinal, "anchor()").await?;

    fixture::reset(&mut state.write(), "C2");
    for index in 0..8 {
        fixture::page(
            "C2",
            &format!("1750001000.{:06}", 90 - index),
            &format!("Budget unread {}", 90 - index),
            false,
            Some(&format!("budget-{}", index + 1)),
            false,
        );
    }
    launch(state, false);
    paused(state).await?;
    ensure(
        fixture::requests().len() == 15
            && fixture::snapshot(&state.peek())["continuation"]["cursor"] == "budget-8",
        "Eight-page budget did not preserve continuation",
    )?;
    capture(
        state,
        desktop,
        "budget-paused",
        &mut ordinal,
        "notice(true)",
    )
    .await?;
    final_page("C2");
    dom(desktop, "click('#unread-opening-continue')").await?;
    complete(state).await?;
    ensure(
        fixture::requests().last().unwrap()["args"]["cursor"] == "budget-8",
        "Budget Continue replayed pages",
    )?;
    capture(state, desktop, "budget-continued", &mut ordinal, "anchor()").await?;

    fixture::reset(&mut state.write(), "C2");
    fixture::page(
        "C2",
        "1750001000.000070",
        "Old target page",
        false,
        None,
        true,
    );
    launch(state, false);
    let id = held(16).await?;
    dom(desktop, "click('#unread-opening-latest')").await?;
    complete(state).await?;
    release(state, id, false).await?;
    ensure(
        !has(&state.peek(), "C2", "1750001000.000070"),
        "Old target merged after Go to latest",
    )?;
    capture(state, desktop, "latest-wins", &mut ordinal, "latest()").await?;

    fixture::reset(&mut state.write(), "C2");
    fixture::page(
        "C2",
        "1750001000.000060",
        "Old channel page",
        false,
        None,
        true,
    );
    launch(state, false);
    let old = held(17).await?;
    fixture::reset(&mut state.write(), "C1");
    fixture::page(
        "C1",
        fixture::FIRST,
        "First unread message",
        false,
        None,
        true,
    );
    launch(state, false);
    let newer = held(18).await?;
    let ticket = state.peek().read_presentation.unread_fetching.clone();
    release(state, old, false).await?;
    ensure(
        ticket.is_some()
            && state.peek().read_presentation.unread_fetching == ticket
            && !has(&state.peek(), "C2", "1750001000.000060"),
        "Old channel stole replacement ticket",
    )?;
    capture(
        state,
        desktop,
        "newer-ticket-waiting",
        &mut ordinal,
        "notice(false)",
    )
    .await?;
    release(state, newer, true).await?;
    complete(state).await?;
    capture(
        state,
        desktop,
        "newer-ticket-complete",
        &mut ordinal,
        "anchor()",
    )
    .await?;

    fixture::reset(&mut state.write(), "C2");
    fixture::page(
        "C2",
        "1750001000.000050",
        "Timed out page",
        false,
        None,
        true,
    );
    launch(state, false);
    let id = held(19).await?;
    capture(
        state,
        desktop,
        "timeout-waiting",
        &mut ordinal,
        "notice(false)",
    )
    .await?;
    // Exercise the actual unchanged20s deadline, not an accelerated fixture clock.
    for _ in 0..500 {
        if !fixture::requests()[id]["outcome"].is_null() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    paused(state).await?;
    let request = fixture::requests()[id].clone();
    ensure(
        request["outcome"] == "timeout"
            && request["elapsedMs"].as_u64().is_some_and(|n| n >= 19_500)
            && !has(&state.peek(), "C2", "1750001000.000050"),
        "Real deadline did not retain cached boundary",
    )?;
    capture(
        state,
        desktop,
        "timeout-paused",
        &mut ordinal,
        "notice(true)",
    )
    .await?;
    dom(desktop, "click('#status-toast .toast-dismiss')").await?;
    final_page("C2");
    dom(desktop, "click('#unread-opening-continue')").await?;
    complete(state).await?;
    capture(
        state,
        desktop,
        "timeout-continued",
        &mut ordinal,
        "anchor()",
    )
    .await?;

    fixture::reset(&mut state.write(), "C2");
    fixture::page(
        "C2",
        "1750001000.000040",
        "Retired account page",
        false,
        None,
        true,
    );
    launch(state, false);
    let id = held(21).await?;
    {
        let mut s = state.write();
        crate::read_preferences::opening::cancel(&mut s);
        s.core.account_epoch += 1;
        s.core.active_account = Some("reconnect-unread-retired".into());
    }
    release(state, id, false).await?;
    ensure(
        !has(&state.peek(), "C2", "1750001000.000040")
            && state.peek().read_presentation.unread_fetching.is_none(),
        "Retired account accepted history",
    )?;
    let g = stable(desktop, "surface()").await?;
    stage(state, "account-wins", &mut ordinal, g.clone()).await?;
    ensure(
        fixture::drained() && fixture::requests().len() == 22 && fixture::openings().len() == 14,
        "Unread fixture did not close all planned reads",
    )?;
    Ok(
        json!({"ok":true,"stages":ordinal,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"noHttp":true,"noReadAcknowledgement":true,"navigation":fixture::snapshot(&state.peek()),"openings":fixture::openings(),"requests":fixture::requests(),"events":fixture::events(),"layout":g,"scope":"Actual shared async unread read/join/pagination/file-display validation/merge/release/continuation with explicitly synthetic pages and no credentials or transport; no server or physical outage proof"}),
    )
}
