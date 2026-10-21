//! Real native composition, rejection and uncertain-save custody. No edit setters.
use super::*;

pub(super) const STAGES: &[&str] = &[
    "draft",
    "composition-refused",
    "rejected",
    "cancelled",
    "unconfirmed",
    "unconfirmed-checked",
    "observed",
    "reopened",
];
const COMPOSING: &str = "Finish composing or close the link editor and recover the reply draft before editing a comment.";

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
        "Own edit custody did not settle: {}",
        snapshot(state, model)
    ))
}

async fn reply_ready(state: Signal<ShellState>, can_send: bool) -> Result<(), String> {
    for _ in 0..500 {
        let ready = {
            let shell = state.peek();
            crate::slack_lists::comments::draft::presentation(&shell)
                .is_some_and(|v| v.can_send == can_send && v.error.is_none())
        };
        if ready {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Original reply composition/recovery did not settle".into())
}

async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if STAGES.get(stages.len()) != Some(&name) {
        return Err("Own edit custody stage order changed".into());
    }
    capture(state, model, d, name, proof, stages).await
}

async fn editable(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d,"(()=>{const e=document.getElementById('slack-list-comment-edit-draft');return {ok:!!e?.isContentEditable&&!e.closest('[inert]')}})()")
        .await?;
    Ok(())
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if stages.len() != 1 || stages[0]["stage"] != "draft" {
        return Err("Custody requires the original common draft capture".into());
    }
    reply_ready(state, true).await?;
    proof["compositionStart"] = command(d, "composition", json!({"active":true})).await?;
    // Wait for the actual reply controller to observe composition before Edit.
    // This selects the existing refusal branch, not a timing-dependent capture.
    reply_ready(state, false).await?;
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    wait(state, model, |v| {
        v["comments"]["draftStore"]["drafts"][0]["error"] == COMPOSING
            && v["edit"]["active"].is_null()
            && v["edit"]["opening"] == false
            && v["fixture"]["policy"] == json!([])
            && v["fixture"]["updates"] == json!([])
    })
    .await?;
    wait_dom(d, &format!(
        "({{ok:document.querySelector('#slack-list-comment-compose .slack-list-error')?.textContent==={}}})",
        json!(COMPOSING)
    )).await?;
    take(state, model, d, "composition-refused", proof, stages).await?;
    proof["compositionEnd"] = command(d, "composition", json!({"active":false})).await?;
    proof["replyRecovery"] = button(d, "slack-list-comment-retry").await?;
    reply_ready(state, true).await?;

    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    let typed = command(
        d,
        "append",
        json!({"text":" Rejected edit B \u{65e5}\u{1f98a}"}),
    )
    .await?;
    proof["rejectedBlocks"] = typed["blocks"].clone();
    data::mode(&state.peek(), data::Mode::Rejected)?;
    button(d, "slack-list-comment-edit-save").await?;
    wait(state, model, |v| {
        v["fixture"]["updates"]
            .as_array()
            .is_some_and(|a| a.len() == 1)
            && v["edit"]["attempts"][0]["phase"] == "rejected"
            && v["edit"]["active"]["busy"] == false
    })
    .await?;
    editable(d).await?;
    take(state, model, d, "rejected", proof, stages).await?;
    button(d, "slack-list-comment-edit-cancel").await?;
    editor(d, false).await?;
    reply_ready(state, true).await?;
    take(state, model, d, "cancelled", proof, stages).await?;

    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    let typed = command(
        d,
        "append",
        json!({"text":" Unconfirmed edit C \u{4e2d}\u{1f642}"}),
    )
    .await?;
    proof["unknownBlocks"] = typed["blocks"].clone();
    data::mode(&state.peek(), data::Mode::Unconfirmed)?;
    button(d, "slack-list-comment-edit-save").await?;
    wait(state, model, |v| {
        v["fixture"]["updates"]
            .as_array()
            .is_some_and(|a| a.len() == 2)
            && v["edit"]["attempts"][1]["phase"] == "unknown"
            && v["edit"]["active"]["busy"] == false
    })
    .await?;
    editable(d).await?;
    proof["blockedControls"] = command(d, "blocked-controls", Value::Null).await?;
    take(state, model, d, "unconfirmed", proof, stages).await?;

    proof["checks"] = json!([]);
    let clicked = button(d, "slack-list-comment-edit-check").await?;
    proof["checks"].as_array_mut().unwrap().push(json!({
        "stage":"unconfirmed-checked","value":clicked
    }));
    wait(state, model, |v| {
        v["comments"]["ledger"]["completed"] == 24
            && v["edit"]["attempts"][1]["phase"] == "unknown"
            && v["edit"]["active"]["busy"] == false
            && v["edit"]["active"]["notice"] == "The edited content could not be confirmed. Both drafts are retained; this save will not be sent again."
    }).await?;
    editable(d).await?;
    take(state, model, d, "unconfirmed-checked", proof, stages).await?;

    // The fixture's independent future row is published only on this explicit
    // step. The original lost-response receipt remains null forever.
    data::publish(&state.peek())?;
    let clicked = button(d, "slack-list-comment-edit-check").await?;
    proof["checks"].as_array_mut().unwrap().push(json!({
        "stage":"observed","value":clicked
    }));
    wait(state, model, |v| {
        v["comments"]["ledger"]["completed"] == 28
            && v["edit"]["attempts"][1]["phase"] == "observed"
            && v["edit"]["attempts"][1]["receipt"].is_null()
            && v["edit"]["active"].is_null()
    })
    .await?;
    editor(d, false).await?;
    reply_ready(state, true).await?;
    take(state, model, d, "observed", proof, stages).await?;

    button(d, "slack-list-comments-back").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
    stable_drafts(state, model).await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Original custody reply was not safely forgettable".into());
    }
    click(d, "slack-list-record-comments").await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
    write(d).await?;
    proof["runtimeForgotten"] = json!(true);
    take(state, model, d, "reopened", proof, stages).await?;
    if stages
        .iter()
        .map(|stage| stage["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != STAGES
    {
        return Err("Own edit custody final stage order changed".into());
    }
    Ok(())
}
