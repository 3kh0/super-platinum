//! Real exclusive SQLite failures before dispatch and after the original ACK.
use super::*;

const STAGES: &[&str] = &[
    "draft",
    "confirming",
    "begin-failed",
    "begin-recovered",
    "delete-held",
    "receipt-failed",
    "receipt-unlocked",
    "receipt-recovered",
    "readback",
    "reopened",
];
const READS: &[u64] = &[4, 8, 12, 12, 16, 16, 16, 16, 20, 24];
const POLICIES: &[usize] = &[0, 2, 4, 4, 6, 6, 6, 6, 6, 6];
const BEGIN_FAILED: &str = "The deletion was not sent because its recovery record could not be saved. Your drafts are retained.";
const RECEIPT_FAILED: &str =
    "The original result could not be saved locally. Retry recovery; no deletion will be resent.";

async fn storage(
    state: Signal<ShellState>,
    ready: &Value,
    sequence: u64,
    proof: &mut Value,
) -> Result<(), String> {
    let action = if sequence % 2 == 1 { "hold" } else { "release" };
    let ack = harness::storage_control(state, ready, sequence, action).await?;
    proof["storageControls"]
        .as_array_mut()
        .ok_or("Missing storage protocol")?
        .push(ack.clone());
    if action == "hold" {
        proof["storage"] = json!({"hold":ack,"release":null});
    } else {
        proof["storage"]["release"] = ack;
    }
    Ok(())
}

async fn notice(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    expected: &str,
) -> Result<(), String> {
    wait_state(state, |v| {
        v["active"]["busy"] == false
            && deleting::presentation(&state.peek())
                .and_then(|p| p.notice)
                .as_deref()
                == Some(expected)
    })
    .await?;
    wait_dom(d, &format!(
        "({{ok:document.getElementById('slack-list-comment-delete-status')?.textContent==={}}})",
        json!(expected),
    )).await?;
    Ok(())
}

async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let index = rows.len();
    if STAGES.get(index) != Some(&name) {
        return Err("Deletion storage stage order changed".into());
    }
    wait_state(state, |_| {
        let v = snapshot(state, model);
        v["comments"]["ledger"]["completed"] == READS[index]
            && v["fixture"]["policy"]
                .as_array()
                .is_some_and(|r| r.len() == POLICIES[index])
            && v["fixture"]["deletes"]
                .as_array()
                .is_some_and(|r| r.len() == usize::from(index >= 4))
    })
    .await?;
    capture(state, model, d, name, proof, rows).await
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let ready = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let source = data::seed(state)?;
    checked(d, include_str!("../../comments.js")).await?;
    checked(d, include_str!("../../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(d, "Independent reply A \u{4e2d}\u{1f642}").await?;
    settled(d).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let mut proof = json!({"source":source,"replySeed":seed,"replyBlocks":remembered["blocks"],
        "storage":null,"storageControls":[]});
    let mut rows = Vec::new();
    take(state, &model, d, "draft", &proof, &mut rows).await?;
    open_delete(state, d).await?;
    take(state, &model, d, "confirming", &proof, &mut rows).await?;

    storage(state, &ready, 1, &mut proof).await?;
    button(d, "confirm").await?;
    notice(state, d, BEGIN_FAILED).await?;
    wait_state(state, |v| v["active"]["canConfirm"] == true).await?;
    take(state, &model, d, "begin-failed", &proof, &mut rows).await?;
    storage(state, &ready, 2, &mut proof).await?;
    take(state, &model, d, "begin-recovered", &proof, &mut rows).await?;

    data::mode(&state.peek(), data::Mode::HeldSuccess)?;
    button(d, "confirm").await?;
    wait_state(state, |_| data::snapshot(&state.peek())["held"] == true).await?;
    proof["blockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
    take(state, &model, d, "delete-held", &proof, &mut rows).await?;

    storage(state, &ready, 3, &mut proof).await?;
    proof["release"] = data::release(&state.peek())?;
    notice(state, d, RECEIPT_FAILED).await?;
    wait_state(state, |v| {
        v["active"]["phase"] == "Pending"
            && v["active"]["localPending"] == true
            && v["active"]["canRetryLocal"] == true
            && v["active"]["canConfirm"] == false
    })
    .await?;
    command(d, "blocked-confirm", Value::Null).await?;
    take(state, &model, d, "receipt-failed", &proof, &mut rows).await?;
    storage(state, &ready, 4, &mut proof).await?;
    take(state, &model, d, "receipt-unlocked", &proof, &mut rows).await?;

    button(d, "retry").await?;
    wait_state(state, |v| {
        v["active"]["phase"] == "Confirmed"
            && v["active"]["busy"] == false
            && v["active"]["localPending"] == false
            && v["active"]["canRetryLocal"] == false
    })
    .await?;
    close_delete(state, d).await?;
    take(state, &model, d, "receipt-recovered", &proof, &mut rows).await?;

    data::publish(&state.peek())?;
    click(d, "slack-list-comments-refresh").await?;
    wait_state(state, |_| {
        let v = fixture::snapshot(&state.peek(), &model);
        v["comments"]["loading"] == false
            && v["comments"]["page"]["deleted"] == json!([data::RECEIVED_TS])
    })
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
    )
    .await?;
    take(state, &model, d, "readback", &proof, &mut rows).await?;
    reopen_reply(state, &model, d).await?;
    proof["runtimeForgotten"] = json!(true);
    take(state, &model, d, "reopened", &proof, &mut rows).await?;
    Ok(
        json!({"ok":true,"case":"own-delete-storage","fixture":ready["fixture"],"readiness":ready,
        "stages":rows,"final":snapshot(state,&model),"initialRecords":fixture::initial_records(&model),
        "noLiveNetwork":true,"limits":["Closed real SQLite lock only; no live Slack.",
        "Lock-time journal evidence belongs to the runner owning the exclusive transaction.",
        "No process-crash or deletion restart acceptance claimed."]}),
    )
}
