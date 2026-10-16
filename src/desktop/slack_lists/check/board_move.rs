//! Eight owned native checkpoints of the existing two-attempt Board cell workflow.
use super::*;
use crate::slack_lists::fixture::{self, board_move::check as adapter};
mod harness;
mod oracle;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !adapter::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => {
                json!({"ok":false,"error":error,"failedLedger":adapter::snapshot(&state.peek()).ok()})
            }
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("slack-list-board-move-check: {receipt}\n").as_bytes())
                .expect("Board Move terminal output");
        })
        .await
        .expect("Board Move terminal writer");
    });
}
async fn settled(
    state: Signal<ShellState>,
    baseline: &Value,
    index: usize,
) -> Result<Value, String> {
    for _ in 0..150 {
        adapter::snapshot(&state.peek())?;
        if let Ok(v) = oracle::stable(&state.peek(), baseline, index) {
            return Ok(v);
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    oracle::stable(&state.peek(), baseline, index)
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    baseline: &Value,
    index: usize,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    if stages.len() != index {
        return Err("Board capture stage order".into());
    }
    let ledger = settled(state, baseline, index).await?;
    let checkbox = ledger["checkbox"].as_bool().unwrap();
    let name = oracle::STAGES[index];
    let geometry = wait_dom(
        d,
        &format!(
            "window.__listBoardMove.geometry({},{},{})",
            json!(name),
            checkbox,
            oracle::groups(checkbox, oracle::loaded(index), 10)
        ),
    )
    .await?;
    if geometry["owner"]["visit"].as_str()
        != ledger["visit"].as_u64().map(|v| v.to_string()).as_deref()
        || geometry["owner"]["selection"].as_str()
            != ledger["selection"]
                .as_u64()
                .map(|v| v.to_string())
                .as_deref()
        || geometry["owner"]["editor"].as_str()
            != ledger["editor"].as_u64().map(|v| v.to_string()).as_deref()
    {
        return Err("Rendered Board Move owner differs".into());
    }
    if oracle::stable(&state.peek(), baseline, index)? != ledger {
        return Err("State changed during geometry".into());
    }
    let png = harness::capture(d, name).await?;
    if oracle::stable(&state.peek(), baseline, index)? != ledger {
        return Err("State changed during native capture".into());
    }
    captures.insert(name.into(), png);
    stages.push(json!({"ordinal":index,"stage":name,"geometry":geometry,"ledger":ledger,"noSession":true,"noTransport":true}));
    Ok(())
}
async fn action(d: &dioxus::desktop::DesktopContext, call: &str) -> Result<Value, String> {
    checked(d, &format!("window.__listBoardMove.{call}")).await
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let identity = harness::wait_runner(state).await?;
    checked(d, include_str!("board_move/ui.js")).await?;
    // Static fixture's initial dialog is closed before setup baseline and ledger arming.
    action(d, "click('#slack-list-editor-cancel')").await?;
    wait_dom(
        d,
        "({ok:!document.querySelector('#slack-list-cell-editor')})",
    )
    .await?;
    action(d, "query()").await?;
    let mut found = false;
    for _ in 0..150 {
        if state
            .peek()
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|v| v.layout.find.input == "Launch" && v.layout.find.applied == "Launch")
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if !found {
        return Err("Find did not settle".into());
    }
    action(d, "bindFind()").await?;
    adapter::arm(&mut state.write())?;
    fixture::refresh(&mut state.write());
    let baseline = adapter::snapshot(&state.peek())?;
    let checkbox = baseline["checkbox"].as_bool().unwrap();
    let first = if checkbox { "true" } else { "" };
    let second = if checkbox { "" } else { "OPT_DONE" };
    action(d, "open()").await?;
    wait_dom(d, &format!("window.__listBoardMove.noOp({checkbox})")).await?;
    action(d, &format!("select({})", json!(first))).await?;
    action(d, "click('#slack-list-editor-cancel')").await?;
    let cancel = wait_dom(d, "window.__listBoardMove.canceled()").await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    stage(state, d, &baseline, 0, &mut stages, &mut captures).await?;
    action(d, "open()").await?;
    wait_dom(d, &format!("window.__listBoardMove.noOp({checkbox})")).await?;
    action(d, &format!("select({})", json!(first))).await?;
    stage(state, d, &baseline, 1, &mut stages, &mut captures).await?;
    action(d, "click('#slack-list-editor-save')").await?;
    stage(state, d, &baseline, 2, &mut stages, &mut captures).await?;
    fixture::set_outcome_uncertain(&mut state.write());
    fixture::complete_pending(&mut state.write());
    stage(state, d, &baseline, 3, &mut stages, &mut captures).await?;
    action(d, "remember()").await?;
    let duplicate = adapter::duplicate_consumed_read(&mut state.write())?;
    let duplicate_focus = action(d, "unchangedFocus()").await?;
    if checkbox {
        adapter::queue_missing(&mut state.write())?;
        action(d, "click('#slack-list-editor-refresh')").await?;
    }
    stage(state, d, &baseline, 4, &mut stages, &mut captures).await?;
    let before = adapter::snapshot(&state.peek())?;
    action(d, "blocked()").await?;
    if adapter::snapshot(&state.peek())? != before {
        return Err("Disabled control replayed mutation".into());
    }
    action(d, "click('#slack-list-editor-refresh')").await?;
    stage(state, d, &baseline, 5, &mut stages, &mut captures).await?;
    action(d, "open()").await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('#slack-list-board-destination:not(:disabled)')})",
    )
    .await?;
    action(d, &format!("select({})", json!(second))).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('#slack-list-editor-save')?.disabled===false})",
    )
    .await?;
    action(d, "click('#slack-list-editor-save')").await?;
    stage(state, d, &baseline, 6, &mut stages, &mut captures).await?;
    fixture::complete_pending(&mut state.write());
    let ack = adapter::snapshot(&state.peek())?;
    if ack["completed"].as_array().map(Vec::len) != Some(2)
        || ack["completed"][1]["result"] != "positive_ack"
        || ack["loaded"] != baseline["pages"][1]
        || ack["operation"]["phase"] != "Uncertain"
    {
        return Err("Positive ACK incorrectly resolved loaded state".into());
    }
    wait_dom(
        d,
        "({ok:document.querySelector('#slack-list-editor-refresh')?.disabled===false})",
    )
    .await?;
    action(d, "click('#slack-list-editor-refresh')").await?;
    stage(state, d, &baseline, 7, &mut stages, &mut captures).await?;
    let final_ledger = oracle::stable(&state.peek(), &baseline, 7)?;
    Ok(
        json!({"ok":true,"identity":identity,"checkbox":checkbox,"baseline":baseline,"cancel":cancel,"duplicate":duplicate,"duplicateFocus":duplicate_focus,"positiveAckBeforeCheck":ack,"stages":stages,"captures":captures,"finalLedger":final_ledger,"actualForms":2,"localPreflights":2,"localReadCompletions":if checkbox{4}else{3},"ignoredConsumedCompletions":1,"httpExecuted":0,"noSession":true,"noTransport":true}),
    )
}
