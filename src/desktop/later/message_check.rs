//! Mounted native acceptance of the real Later metadata route, disconnected.
use super::message_fixture as fixture;
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
mod support;
use support::*;
fn ensure(ok: bool, error: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(error.into()) }
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
        eprintln!("later-cold-message-navigation-check: {result}");
    });
}
async fn open(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    dom(desktop, &format!("open({})", json!(id)))
        .await
        .map(|_| ())
}
async fn close(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    dom(desktop, "click('.thread-close')").await?;
    Ok(())
}
async fn routed(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
) -> Result<Value, String> {
    wait(state, |s| {
        s.main_view == MainView::Later
            && s.core.active_channel.as_deref() == Some(id)
            && s.thread_root.as_deref() == Some(fixture::root(id).as_str())
            && !s.thread_loading
    })
    .await?;
    let route = fixture::routes()
        .last()
        .cloned()
        .ok_or("Missing actual route receipt")?;
    ensure(
        route["channel"] == id
            && route["requestedReply"] == fixture::key(id).ts
            && route["target"] == fixture::key(id).ts
            && route["root"] == fixture::root(id),
        "Wrong root/reply route",
    )?;
    dom(
        desktop,
        &format!(
            "thread({},{})",
            json!(fixture::root(id)),
            json!(fixture::key(id).ts)
        ),
    )
    .await
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut ordinal = 0;
    let initial = support::initial(desktop).await?;
    ensure(
        fixture::enabled(&state.peek())
            && state.peek().core.later.hydrated.is_empty()
            && fixture::snapshot(&state.peek())["knownChannels"]
                .as_array()
                .unwrap()
                .is_empty(),
        "Initial target must be cold",
    )?;
    stage(state, "cold-list", &mut ordinal, initial).await?;
    open(desktop, "CLATERA").await?;
    let g = routed(state, desktop, "CLATERA").await?;
    stage(state, "cold-reply", &mut ordinal, g).await?;
    close(desktop).await?;
    open(desktop, "CLATERN").await?;
    wait(state, |s| {
        s.core.active_channel.as_deref() == Some("CLATERN")
            && fixture::routes()
                .last()
                .is_some_and(|r| r["channel"] == "CLATERN")
    })
    .await?;
    ensure(
        state.peek().thread_root.is_none() && state.peek().thread_target.is_none(),
        "Standalone message opened a thread",
    )?;
    let g = dom(
        desktop,
        &format!("message({})", json!(fixture::key("CLATERN").ts)),
    )
    .await?;
    stage(state, "standalone-message", &mut ordinal, g).await?;
    fixture::arm("message", "CLATERM", fixture::Outcome::Missing, false);
    open(desktop, "CLATERM").await?;
    let g = dom(
        desktop,
        "list('CLATERM','This saved message is unavailable.')",
    )
    .await?;
    ensure(
        state
            .peek()
            .core
            .later
            .items
            .contains_key(&fixture::key("CLATERM"))
            && state.peek().thread_root.is_none(),
        "Missing message changed membership/destination",
    )?;
    stage(state, "missing", &mut ordinal, g).await?;
    dom(
        desktop,
        r#"click('.toast [aria-label="Dismiss notification"]')"#,
    )
    .await?;
    open(desktop, "CLATERM").await?;
    let g = routed(state, desktop, "CLATERM").await?;
    stage(state, "missing-retry", &mut ordinal, g).await?;
    close(desktop).await?;
    fixture::arm("channel", "CLATERE", fixture::Outcome::Error, false);
    open(desktop, "CLATERE").await?;
    let g = dom(
        desktop,
        "list('CLATERE','Could not open this conversation. Please try again.')",
    )
    .await?;
    ensure(
        state
            .peek()
            .core
            .later
            .items
            .contains_key(&fixture::key("CLATERE"))
            && !state.peek().core.workspaces["T1"]
                .channels
                .contains_key("CLATERE"),
        "Failed metadata changed membership/channel",
    )?;
    stage(state, "channel-error", &mut ordinal, g).await?;
    dom(
        desktop,
        r#"click('.toast [aria-label="Dismiss notification"]')"#,
    )
    .await?;
    open(desktop, "CLATERE").await?;
    let g = routed(state, desktop, "CLATERE").await?;
    stage(state, "channel-retry", &mut ordinal, g).await?;
    close(desktop).await?;
    fixture::arm("message", "CLATERS", fixture::Outcome::Success, true);
    open(desktop, "CLATERS").await?;
    let slow = held("message", "CLATERS").await?;
    open(desktop, "CLATERB").await?;
    let _ = routed(state, desktop, "CLATERB").await?;
    release(state, slow, false).await?;
    ensure(
        !state
            .peek()
            .core
            .later
            .hydrated
            .contains_key(&fixture::key("CLATERS"))
            && !state.peek().core.workspaces["T1"]
                .channels
                .contains_key("CLATERS"),
        "Obsolete A metadata published",
    )?;
    let g = routed(state, desktop, "CLATERB").await?;
    stage(state, "newer-wins", &mut ordinal, g).await?;
    close(desktop).await?;
    fixture::arm("channel", "CLATERD", fixture::Outcome::Success, true);
    open(desktop, "CLATERD").await?;
    let slow = held("channel", "CLATERD").await?;
    dom(desktop, "click('.rail button[title=Home]')").await?;
    wait(state, |s| s.main_view == MainView::Home).await?;
    release(state, slow, false).await?;
    ensure(
        !state.peek().core.workspaces["T1"]
            .channels
            .contains_key("CLATERD")
            && state.peek().thread_root.is_none(),
        "Departure response reopened Later",
    )?;
    let g = dom(desktop, "surface('.rail button[title=Home]')").await?;
    stage(state, "departure-wins", &mut ordinal, g).await?;
    dom(desktop, "click('.rail button[title=Later]')").await?;
    wait(state, |s| s.main_view == MainView::Later).await?;
    fixture::arm("history", "CLATERF", fixture::Outcome::Success, true);
    open(desktop, "CLATERF").await?;
    let slow = held("history", "CLATERF").await?;
    let before = state.peek().core.later.generation;
    dom(desktop, "tab('Completed')").await?;
    wait(state, |s| s.core.later.generation != before).await?;
    release(state, slow, false).await?;
    ensure(
        state.peek().thread_root.is_none()
            && !fixture::routes().iter().any(|r| r["channel"] == "CLATERF"),
        "Old list visit opened thread after history",
    )?;
    let g = dom(desktop, "surface('.later-tabs [aria-selected=true]')").await?;
    stage(state, "filter-wins", &mut ordinal, g).await?;
    dom(desktop, "tab('In progress')").await?;
    fixture::arm("history", "CLATERF", fixture::Outcome::Success, true);
    open(desktop, "CLATERF").await?;
    let slow = held("history", "CLATERF").await?;
    let shortcut = state.peek().shortcut_generation;
    state.write().open_palette();
    wait(state, |s| {
        s.overlay == Some(crate::state::Overlay::Palette) && s.shortcut_generation != shortcut
    })
    .await?;
    let g = dom(desktop, "surface('#shell-overlay-dialog')").await?;
    stage(state, "shortcut-open", &mut ordinal, g).await?;
    let dismissal = dom(desktop, "dismissPalette()").await?;
    wait(state, |s| s.overlay.is_none()).await?;
    release(state, slow, false).await?;
    ensure(
        state.peek().thread_root.is_none()
            && !fixture::routes().iter().any(|r| r["channel"] == "CLATERF"),
        "Closed shortcut allowed obsolete thread",
    )?;
    let mut g = dom(desktop, "surface('.later-tabs [aria-selected=true]')").await?;
    g["dismissal"] = dismissal;
    stage(state, "shortcut-wins", &mut ordinal, g).await?;
    dom(desktop, "list('CLATERQ',null)").await?;
    fixture::arm("message", "CLATERQ", fixture::Outcome::Success, true);
    open(desktop, "CLATERQ").await?;
    let slow = held("message", "CLATERQ").await?;
    // Explicit offline account retirement, not a fabricated live sign-in.
    {
        let mut s = state.write();
        s.core.account_epoch += 1;
        s.core.active_account = Some("later-cold-message-retired".into());
        s.close_thread();
        s.main_view = MainView::Home;
    }
    release(state, slow, false).await?;
    ensure(
        !state
            .peek()
            .core
            .later
            .hydrated
            .contains_key(&fixture::key("CLATERQ"))
            && !state.peek().core.workspaces["T1"]
                .channels
                .contains_key("CLATERQ"),
        "Retired account accepted metadata",
    )?;
    let g = dom(desktop, "surface('.rail button[title=Home]')").await?;
    stage(state, "account-wins", &mut ordinal, g.clone()).await?;
    ensure(
        fixture::outstanding() == 0 && state.peek().core.later.items.len() == 9,
        "Pending response or lost saved membership",
    )?;
    Ok(
        json!({"ok":true,"stages":ordinal,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"noHttp":true,"navigation":fixture::snapshot(&state.peek()),"requests":fixture::requests(),"routes":fixture::routes(),"layout":g,"scope":"Actual asynchronous saved metadata route and mounted routing; synthetic preseeded history/thread bags, no live history I/O"}),
    )
}
