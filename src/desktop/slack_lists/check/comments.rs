//! Closed comments reads through the actual record button and controller.
use super::*;
use crate::slack_lists::comments::fixture::{self, Mode, Model};
use std::time::Duration;
mod harness;
const STAGES: &[&str] = &[
    "detail",
    "loading",
    "first",
    "next",
    "malformed",
    "wrong-root",
    "offline",
    "empty",
    "cooldown",
    "back",
    "synthetic",
    "record-held",
    "record-replaced",
    "account-held",
    "account-replaced",
];
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::check_enabled() || fixture::compose::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let text = format!("slack-list-comments-check: {result}\n");
            std::io::stderr()
                .lock()
                .write_all(text.as_bytes())
                .expect("Comments terminal receipt");
        })
        .await
        .expect("Comments terminal writer");
    });
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(d, &format!("window.__listComments.click({})", json!(id))).await?;
    Ok(())
}
async fn open(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(d, &format!("window.__listComments.open({})", json!(id))).await?;
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==={}}})",
            json!(id)
        ),
    )
    .await?;
    Ok(())
}
async fn wait(
    state: Signal<ShellState>,
    model: &Model,
    requests: usize,
    loading: bool,
) -> Result<Value, String> {
    for _ in 0..150 {
        let v = fixture::snapshot(&state.peek(), model);
        let count = v["ledger"]["requests"].as_array().unwrap().len();
        if count > requests {
            return Err(format!(
                "Unexpected comments request count {count}>{requests}"
            ));
        }
        if count == requests && v["comments"]["loading"] == loading {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Comments state did not settle at {requests}/{loading}"
    ))
}
async fn cancelled(state: Signal<ShellState>, model: &Model, count: u64) -> Result<(), String> {
    for _ in 0..100 {
        if fixture::snapshot(&state.peek(), model)["ledger"]["cancelled"] == count {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Original held comments request did not cancel".into())
}
async fn stage(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    if STAGES.get(rows.len()) != Some(&name) {
        return Err("Comments stage order".into());
    }
    let geometry = wait_dom(d, "window.__listComments.snapshot()").await?;
    let before = fixture::snapshot(&state.peek(), model);
    if before["writes"] != 0 || before["noSession"] != true || before["noTransport"] != true {
        return Err("Comments fixture wrote or acquired live credentials".into());
    }
    let path = harness::capture(d, name).await?;
    let after = fixture::snapshot(&state.peek(), model);
    // Remaining cooldown seconds are presentation time, not source/read mutation.
    let mut comparable = after.clone();
    if comparable["comments"].is_object() {
        comparable["comments"]["retryAfter"] = before["comments"]["retryAfter"].clone();
    }
    if comparable != before {
        return Err("Comments model changed during capture".into());
    }
    rows.push(
        json!({"stage":name,"model":before,"geometry":geometry,"proof":proof,"capture":path}),
    );
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    checked(d, include_str!("comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let mut stages = vec![];
    stage(state, &model, d, "detail", Value::Null, &mut stages).await?;
    fixture::mode(&model, Mode::Held)?;
    click(d, "slack-list-record-comments").await?;
    wait(state, &model, 4, true).await?;
    stage(state, &model, d, "loading", Value::Null, &mut stages).await?;
    let release = fixture::release(&model)?;
    wait(state, &model, 4, false).await?;
    stage(
        state,
        &model,
        d,
        "first",
        json!({"release":release}),
        &mut stages,
    )
    .await?;
    click(d, "slack-list-comments-next").await?;
    wait(state, &model, 8, false).await?;
    stage(state, &model, d, "next", Value::Null, &mut stages).await?;
    for (mode, name, count) in [
        (Mode::Malformed, "malformed", 12),
        (Mode::WrongRoot, "wrong-root", 16),
        (Mode::Offline, "offline", 20),
        (Mode::Empty, "empty", 24),
        (Mode::RateLimited, "cooldown", 28),
    ] {
        fixture::mode(&model, mode)?;
        click(d, "slack-list-comments-refresh").await?;
        wait(state, &model, count, false).await?;
        let proof = if name == "cooldown" {
            let before = fixture::snapshot(&state.peek(), &model)["ledger"].clone();
            let native=checked(d,"(()=>{const e=document.getElementById('slack-list-comments-refresh');if(!e?.disabled)return {ok:false};e.click();return {ok:true,disabled:true};})()").await?;
            tokio::time::sleep(Duration::from_millis(120)).await;
            let after = fixture::snapshot(&state.peek(), &model)["ledger"].clone();
            if before != after {
                return Err("Cooldown dispatched a read".into());
            }
            json!({"before":before,"after":after,"native":native})
        } else {
            Value::Null
        };
        stage(state, &model, d, name, proof, &mut stages).await?;
    }
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
    )
    .await?;
    click(d, "slack-list-comments-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&document.activeElement===document.getElementById('slack-list-record-comments')})").await?;
    stage(state, &model, d, "back", Value::Null, &mut stages).await?;
    click(d, "slack-list-record-back").await?;
    open(d, "R_LIST_001").await?;
    click(d, "slack-list-record-comments").await?;
    wait(state, &model, 32, false).await?;
    stage(state, &model, d, "synthetic", Value::Null, &mut stages).await?;
    fixture::mode(&model, Mode::Held)?;
    click(d, "slack-list-comments-refresh").await?;
    wait(state, &model, 36, true).await?;
    stage(state, &model, d, "record-held", Value::Null, &mut stages).await?;
    let before = fixture::snapshot(&state.peek(), &model);
    fixture::replace_record(&mut state.write())?;
    cancelled(state, &model, 1).await?;
    let release = fixture::release(&model)?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&!!document.getElementById('slack-list-record-detail')})").await?;
    stage(
        state,
        &model,
        d,
        "record-replaced",
        json!({"before":before,"release":release}),
        &mut stages,
    )
    .await?;
    fixture::mode(&model, Mode::Held)?;
    click(d, "slack-list-record-comments").await?;
    wait(state, &model, 40, true).await?;
    stage(state, &model, d, "account-held", Value::Null, &mut stages).await?;
    let before = fixture::snapshot(&state.peek(), &model);
    let (media, epoch) = {
        let s = state.peek();
        (s.media.clone(), s.core.account_epoch)
    };
    let mut replacement = ShellState::fixture(media);
    replacement.slack_lists = Default::default();
    replacement.overlay = None;
    replacement.core.active_account = Some("list-comments-successor".into());
    replacement.core.account_epoch = epoch + 1;
    crate::bootstrap::install_loaded(state, replacement);
    cancelled(state, &model, 2).await?;
    let release = fixture::release(&model)?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&!document.querySelector('.slack-list-dialog')})").await?;
    stage(
        state,
        &model,
        d,
        "account-replaced",
        json!({"before":before,"release":release}),
        &mut stages,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":readiness["fixture"],"readiness":readiness,"stages":stages,"final":fixture::snapshot(&state.peek(),&model),"noLiveNetwork":true,"initialRecords":fixture::initial_records(&model)}),
    )
}
