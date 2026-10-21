//! Real bridge failure/recovery and parsed selected-comment retirement.
//! No controller setters, replacement editor, write replay or journal bypass.
use super::*;

pub(super) const STAGES: &[&str] = &[
    "draft",
    "editing",
    "capture-failed",
    "capture-recovered",
    "source-stale",
    "cancelled",
    "unconfirmed",
    "unconfirmed-capture-failed",
    "unconfirmed-recovered",
    "unconfirmed-checked",
    "observed",
    "reopened",
];
const READS: &[u64] = &[4, 8, 8, 8, 8, 8, 20, 20, 20, 24, 28, 32];
const POLICIES: &[usize] = &[0, 4, 4, 4, 4, 4, 12, 12, 12, 12, 12, 12];
const FAILED: &str = "The original edit capture stopped. Retry capture before saving or leaving.";
const STALE: &str = "The original comment changed. Your edit and reply draft are retained.";
const UNCONFIRMED: &str = "The edited content could not be confirmed. Both drafts are retained; this save will not be sent again.";

async fn wait(
    state: Signal<ShellState>,
    model: &Model,
    expected: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..500 {
        let value = snapshot(state, model);
        if expected(&value) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Own edit recovery did not settle: {}",
        snapshot(state, model)
    ))
}
async fn ready(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let value = wait(state, model, |v| {
        v["capture"]["canSave"] == true && v["capture"]["failed"] == false
    })
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comment-edit-save')?.disabled===false})",
    )
    .await?;
    Ok(value)
}
async fn notice(d: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-comment-edit-status')?.textContent==={}}})",
            json!(text)
        ),
    )
    .await?;
    Ok(())
}
async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let index = stages.len();
    if STAGES.get(index) != Some(&name) {
        return Err("Own edit recovery stage order changed".into());
    }
    wait(state, model, |v| {
        v["comments"]["ledger"]["completed"] == READS[index]
            && v["fixture"]["policy"]
                .as_array()
                .is_some_and(|p| p.len() == POLICIES[index])
            && v["fixture"]["updates"]
                .as_array()
                .is_some_and(|p| p.len() == usize::from(index >= 6))
    })
    .await?;
    capture(state, model, d, name, proof, stages).await?;
    let stage = stages.last().ok_or("Recovery capture missing")?;
    if !stage["model"]["edit"]["active"].is_null() && stage["geometry"]["edit"]["sameNode"] != true
    {
        return Err("Original edit node was replaced during recovery".into());
    }
    if matches!(
        name,
        "capture-failed"
            | "capture-recovered"
            | "source-stale"
            | "unconfirmed-capture-failed"
            | "unconfirmed-recovered"
            | "unconfirmed-checked"
    ) {
        let fault = &stage["geometry"]["captureFault"];
        if fault["count"] != 1
            || fault["armed"] != false
            || fault["originalNode"] != true
            || fault["apiRestored"] != true
        {
            return Err("Recovery did not retain the exact one-shot native fault witness".into());
        }
    }
    Ok(())
}
fn same_mount(value: &Value, remembered: &Value) -> bool {
    ["scope", "mount"]
        .iter()
        .all(|key| value["capture"][*key] == remembered[*key])
        && remembered["generation"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .is_some_and(|g| value["capture"]["generation"].as_u64() == Some(g))
}
async fn fail_capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    remembered: &Value,
    expected_blocks: &Value,
    unknown: bool,
) -> Result<Value, String> {
    let armed = command(d, "fail-edit-snapshot", Value::Null).await?;
    if ["scope", "mount", "generation"]
        .iter()
        .any(|key| armed[*key] != remembered[*key])
        || armed["blocks"] != *expected_blocks
        || armed["armed"] != true
    {
        return Err("Snapshot fault did not target the exact original edit".into());
    }
    wait(state, model, |v| {
        same_mount(v, remembered)
            && v["capture"]["failed"] == true
            && v["capture"]["canRetry"] == true
            && v["capture"]["canSave"] == false
            && v["capture"]["canCancel"] == false
            && v["capture"]["canCheck"] == false
            && v["edit"]["active"]["captureFailed"] == true
            && v["edit"]["active"]["blocks"] == *expected_blocks
            && (!unknown || v["edit"]["attempts"][0]["phase"] == "unknown")
    })
    .await?;
    notice(d, FAILED).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comment-edit-retry')?.disabled===false})",
    )
    .await?;
    Ok(armed)
}
async fn recover(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    remembered: &Value,
    expected_blocks: &Value,
    unknown: bool,
) -> Result<Value, String> {
    let clicked = button(d, "slack-list-comment-edit-retry").await?;
    wait(state, model, |v| {
        same_mount(v, remembered)
            && v["capture"]["failed"] == false
            && v["capture"]["canRetry"] == false
            && v["edit"]["active"]["nativeReady"] == true
            && v["edit"]["active"]["captureFailed"] == false
            && v["edit"]["active"]["blocks"] == *expected_blocks
            && if unknown {
                v["capture"]["canCheck"] == true
                    && v["capture"]["canSave"] == false
                    && v["capture"]["canCancel"] == false
                    && v["edit"]["attempts"][0]["phase"] == "unknown"
            } else {
                v["capture"]["canSave"] == true && v["capture"]["canCancel"] == true
            }
    })
    .await?;
    wait_dom(d, &format!("(()=>{{const e=document.getElementById('slack-list-comment-edit-draft');return {{ok:!!e?.isContentEditable&&!e.closest('[inert]')&&document.getElementById({})?.disabled===false&&!document.getElementById('slack-list-comment-edit-retry')}};}})()",
        json!(if unknown {"slack-list-comment-edit-check"} else {"slack-list-comment-edit-save"}))).await?;
    Ok(clicked)
}
fn change_source(mut state: Signal<ShellState>, model: &Model) -> Result<Value, String> {
    use super_platinum_core::slack::events::RtEvent;
    let mut changed = data::snapshot(&state.peek())["received"].clone();
    changed["text"] = json!("Socket changed original comment 日");
    changed["blocks"][0]["elements"][0]["elements"][0]["text"] = changed["text"].clone();
    changed["edited"] = json!({"user":fixture::completion::SELF,"ts":"1781525900.000001"});
    let frame = json!({"type":"message","subtype":"message_changed","channel":"CCOMMENT01","message":changed});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Selected edit event did not parse")?;
    let witness = match &event {
        RtEvent::MessageChanged {
            original_update: Some(update),
            ..
        } => {
            json!({"kind":format!("{:?}",update.kind()),"outerChannel":update.outer_channel(),"typedTs":update.typed_ts(),"typedChannel":update.typed_channel(),"raw":update.raw()})
        }
        _ => return Err("Selected event lost its original partial witness".into()),
    };
    let before = snapshot(state, model);
    let generation = {
        let mut shell = state.write();
        if !data::enabled(&shell) || !fixture::completion::enabled(&shell) {
            return Err("Original closed own-edit carrier required".into());
        }
        let generation = shell
            .core
            .workspaces
            .get("T1")
            .ok_or("Original workspace missing")?
            .rt_generation;
        if crate::slack_lists::events::prepare(&mut shell, "T1", generation, &event) {
            return Err("Selected body edit unexpectedly requested a List refresh".into());
        }
        crate::realtime::apply_in_view(&mut shell.core, "T1", generation, event, false);
        generation
    };
    let after = snapshot(state, model);
    if before["edit"]["active"]["current"] != true
        || after["edit"]["active"]["current"] != false
        || before["edit"]["active"]["blocks"] != after["edit"]["active"]["blocks"]
        || before["comments"]["draftStore"] != after["comments"]["draftStore"]
        || before["comments"]["comments"]["page"] != after["comments"]["comments"]["page"]
        || before["fixture"] != after["fixture"]
    {
        return Err("Selected event changed original edit/reply/HTTP custody unexpectedly".into());
    }
    Ok(
        json!({"team":"T1","generation":generation,"frame":frame,"parsed":true,"witness":witness,
        "before":{"edit":before["edit"],"comments":before["comments"]},
        "after":{"edit":after["edit"],"comments":after["comments"]}}),
    )
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if stages.len() != 1 || stages[0]["stage"] != "draft" {
        return Err("Recovery requires original common draft".into());
    }
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    ready(state, model, d).await?;
    proof["recoveryBlocks"] =
        command(d, "append", json!({"text":" Recovery edit B 日🦊"})).await?["blocks"].clone();
    wait(state, model, |v| {
        v["edit"]["active"]["blocks"] == proof["recoveryBlocks"]
    })
    .await?;
    proof["firstEdit"] = command(d, "remember-edit", Value::Null).await?;
    take(state, model, d, "editing", proof, stages).await?;
    proof["firstFailure"] = fail_capture(
        state,
        model,
        d,
        &proof["firstEdit"],
        &proof["recoveryBlocks"],
        false,
    )
    .await?;
    take(state, model, d, "capture-failed", proof, stages).await?;
    proof["recoveries"] = json!([{"stage":"capture-recovered","value":recover(state,model,d,&proof["firstEdit"],&proof["recoveryBlocks"],false).await?}]);
    take(state, model, d, "capture-recovered", proof, stages).await?;
    proof["sourceChanged"] = change_source(state, model)?;
    wait(state, model, |v| {
        v["capture"]["canSave"] == false
            && v["capture"]["canCancel"] == true
            && v["edit"]["active"]["current"] == false
    })
    .await?;
    notice(d, STALE).await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-comment-edit-save')?.disabled===true&&document.getElementById('slack-list-comment-edit-cancel')?.disabled===false})").await?;
    proof["staleSave"] = command(d, "double-save", Value::Null).await?;
    take(state, model, d, "source-stale", proof, stages).await?;
    button(d, "slack-list-comment-edit-cancel").await?;
    editor(d, false).await?;
    take(state, model, d, "cancelled", proof, stages).await?;

    // Only an explicit real Refresh replaces the stale typed body with the
    // fixture's unchanged original canonical receipt before another edit.
    proof["refreshAfterStale"] = button(d, "slack-list-comments-refresh").await?;
    wait(state, model, |v| v["comments"]["ledger"]["completed"] == 12).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
    )
    .await?;
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    ready(state, model, d).await?;
    proof["unknownBlocks"] = command(d, "append", json!({"text":" Unconfirmed recovery C 中🙂"}))
        .await?["blocks"]
        .clone();
    wait(state, model, |v| {
        v["edit"]["active"]["blocks"] == proof["unknownBlocks"]
    })
    .await?;
    proof["unknownEdit"] = command(d, "remember-edit", Value::Null).await?;
    data::mode(&state.peek(), data::Mode::Unconfirmed)?;
    button(d, "slack-list-comment-edit-save").await?;
    wait(state, model, |v| {
        v["edit"]["attempts"][0]["phase"] == "unknown"
            && v["capture"]["canCheck"] == true
            && v["capture"]["busy"] == false
    })
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comment-edit-check')?.disabled===false})",
    )
    .await?;
    proof["blockedControls"] = command(d, "blocked-controls", Value::Null).await?;
    take(state, model, d, "unconfirmed", proof, stages).await?;
    proof["unknownFailure"] = fail_capture(
        state,
        model,
        d,
        &proof["unknownEdit"],
        &proof["unknownBlocks"],
        true,
    )
    .await?;
    take(state, model, d, "unconfirmed-capture-failed", proof, stages).await?;
    let recovered = recover(
        state,
        model,
        d,
        &proof["unknownEdit"],
        &proof["unknownBlocks"],
        true,
    )
    .await?;
    proof["recoveries"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"unconfirmed-recovered","value":recovered}));
    take(state, model, d, "unconfirmed-recovered", proof, stages).await?;

    proof["checks"] = json!([{"stage":"unconfirmed-checked","value":button(d,"slack-list-comment-edit-check").await?}]);
    wait(state, model, |v| {
        v["comments"]["ledger"]["completed"] == 24
            && v["edit"]["attempts"][0]["phase"] == "unknown"
            && v["capture"]["canCheck"] == true
            && v["edit"]["active"]["notice"] == UNCONFIRMED
    })
    .await?;
    notice(d, UNCONFIRMED).await?;
    take(state, model, d, "unconfirmed-checked", proof, stages).await?;
    data::publish(&state.peek())?;
    let clicked = button(d, "slack-list-comment-edit-check").await?;
    proof["checks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"observed","value":clicked}));
    wait(state, model, |v| {
        v["comments"]["ledger"]["completed"] == 28
            && v["edit"]["attempts"][0]["phase"] == "observed"
            && v["edit"]["attempts"][0]["receipt"].is_null()
            && v["edit"]["active"].is_null()
    })
    .await?;
    editor(d, false).await?;
    take(state, model, d, "observed", proof, stages).await?;
    button(d, "slack-list-comments-back").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
    stable_drafts(state, model).await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Original recovery reply was not safely forgettable".into());
    }
    click(d, "slack-list-record-comments").await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
    write(d).await?;
    proof["runtimeForgotten"] = json!(true);
    take(state, model, d, "reopened", proof, stages).await?;
    if stages
        .iter()
        .map(|s| s["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != STAGES
    {
        return Err("Recovery final stage order changed".into());
    }
    Ok(())
}
