//! Actual controls recover an inert original deletion from the closed SQLite DB.
use super::*;
use data::restart as disk;

const EXERCISE: &[&str] = &[
    "exercise-draft",
    "exercise-confirming",
    "exercise-unresolved",
];
const RESTORE: &[&str] = &[
    "restored-reply",
    "restored-delete",
    "unconfirmed-checked",
    "cancelled",
    "reopened-delete",
    "observed",
    "reopened",
];
const NEGATIVE: &str = "Deletion is still unconfirmed. Missing or changed comments are not proof; the original attempt will not be sent again.";

pub(super) fn named(case: &str) -> bool {
    matches!(
        case,
        "own-delete-restart-pending-exercise"
            | "own-delete-restart-pending-restore"
            | "own-delete-restart-unknown-exercise"
            | "own-delete-restart-unknown-restore"
    )
}

async fn take(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    counts: (u64, usize, usize),
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    wait_state(state, |_| {
        let v = snapshot(state, model);
        v["comments"]["ledger"]["completed"] == counts.0
            && v["fixture"]["policy"]
                .as_array()
                .is_some_and(|v| v.len() == counts.1)
            && v["fixture"]["deletes"]
                .as_array()
                .is_some_and(|v| v.len() == counts.2)
    })
    .await?;
    let before = disk::snapshot(state).await?;
    capture(state, model, d, name, proof, rows).await?;
    if disk::snapshot(state).await? != before {
        return Err(format!("Original persisted deletion changed during {name}"));
    }
    let row = rows.last_mut().ok_or("Restart capture missing")?;
    if !row["deleteInventory"].is_null() && row["deleteInventory"] != before {
        return Err("Independent deletion inventories disagree".into());
    }
    row["deleteInventory"] = before;
    Ok(())
}

async fn recovered(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    phase: &str,
) -> Result<(), String> {
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    wait_state(state, |v| {
        v["active"]["phase"] == phase
            && v["active"]["busy"] == false
            && v["active"]["canCheck"] == true
            && v["active"]["canConfirm"] == false
    })
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comment-delete-check')?.disabled===false})",
    )
    .await?;
    Ok(())
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    if !named(case) {
        return Err("Exact deletion restart case required".into());
    }
    let restore = case.ends_with("-restore");
    let pending = case.contains("-pending-");
    let phase = if pending { "Pending" } else { "Uncertain" };
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
    let initial_delete = disk::snapshot(state).await?;
    let initial_reply = compose::journal_snapshot(state).await?;
    if restore {
        if initial_delete["attempt"]["phase"] != phase
            || !initial_delete["attempt"]["raw"].is_null()
            || initial_reply["blocks"].as_array().is_none_or(Vec::is_empty)
        {
            return Err("Restore requires the original unresolved deletion and rich reply".into());
        }
    } else if initial_delete["revision"] != 0 || !initial_delete["attempt"].is_null() {
        return Err("Exercise requires a fresh isolated deletion journal".into());
    }
    let seed = if restore {
        Value::Null
    } else {
        compose::seed_draft(state).await?
    };
    write(d).await?;
    if !restore {
        type_text(d, "Independent reply A \u{4e2d}\u{1f642}").await?;
    }
    settled(d).await?;
    stable_drafts(state, &model).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let mut proof = json!({"source":source,"replySeed":seed,"replyBlocks":remembered["blocks"],
        "initialDeleteInventory":initial_delete,"initialReplyInventory":initial_reply,
        "processId":std::process::id(),"scenario":if pending{"pending"}else{"unknown"},
        "phase":if restore{"restore"}else{"exercise"}});
    let mut rows = Vec::new();
    if restore {
        if proof["replyBlocks"] != initial_reply["blocks"]
            || disk::snapshot(state).await? != initial_delete
        {
            return Err("Reply reopening changed original recovery bytes".into());
        }
        take(
            state,
            &model,
            d,
            "restored-reply",
            (4, 0, 0),
            &proof,
            &mut rows,
        )
        .await?;
        recovered(state, d, phase).await?;
        if disk::snapshot(state).await? != initial_delete {
            return Err("Recovery open rewrote the original attempt".into());
        }
        proof["blockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        take(
            state,
            &model,
            d,
            "restored-delete",
            (4, 0, 0),
            &proof,
            &mut rows,
        )
        .await?;
        button(d, "check").await?;
        wait_dom(d,&format!("({{ok:document.getElementById('slack-list-comment-delete-status')?.textContent==={}}})",json!(NEGATIVE))).await?;
        wait_state(state, |v| {
            v["active"]["canCheck"] == true && v["active"]["phase"] == phase
        })
        .await?;
        if disk::snapshot(state).await? != initial_delete {
            return Err("Negative readback rewrote unresolved evidence".into());
        }
        take(
            state,
            &model,
            d,
            "unconfirmed-checked",
            (8, 0, 0),
            &proof,
            &mut rows,
        )
        .await?;
        close_delete(state, d).await?;
        take(state, &model, d, "cancelled", (8, 0, 0), &proof, &mut rows).await?;
        recovered(state, d, phase).await?;
        proof["reopenedBlockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        take(
            state,
            &model,
            d,
            "reopened-delete",
            (8, 0, 0),
            &proof,
            &mut rows,
        )
        .await?;
        proof["publication"] = disk::publish_saved(state).await?;
        button(d, "check").await?;
        wait_state(state, |v| {
            v["active"]["phase"] == "Observed"
                && v["active"]["busy"] == false
                && v["active"]["localPending"] == false
        })
        .await?;
        close_delete(state, d).await?;
        take(state, &model, d, "observed", (12, 0, 0), &proof, &mut rows).await?;
        reopen_reply(state, &model, d).await?;
        proof["runtimeForgotten"] = json!(true);
        take(state, &model, d, "reopened", (16, 0, 0), &proof, &mut rows).await?;
    } else {
        take(
            state,
            &model,
            d,
            "exercise-draft",
            (4, 0, 0),
            &proof,
            &mut rows,
        )
        .await?;
        open_delete(state, d).await?;
        take(
            state,
            &model,
            d,
            "exercise-confirming",
            (8, 2, 0),
            &proof,
            &mut rows,
        )
        .await?;
        data::mode(
            &state.peek(),
            if pending {
                data::Mode::HeldSuccess
            } else {
                data::Mode::Unconfirmed
            },
        )?;
        button(d, "confirm").await?;
        wait_state(state, |v| {
            if pending {
                data::snapshot(&state.peek())["held"] == true
            } else {
                v["active"]["phase"] == phase && v["active"]["canCheck"] == true
            }
        })
        .await?;
        let inventory = disk::snapshot(state).await?;
        if inventory["attempt"]["phase"] != phase || !inventory["attempt"]["raw"].is_null() {
            return Err("Original unresolved attempt was not durably recorded".into());
        }
        proof["blockedConfirm"] = command(d, "blocked-confirm", Value::Null).await?;
        take(
            state,
            &model,
            d,
            "exercise-unresolved",
            (12, 4, 1),
            &proof,
            &mut rows,
        )
        .await?;
    }
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != if restore { RESTORE } else { EXERCISE }
    {
        return Err("Deletion restart stages changed".into());
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":ready["fixture"],"readiness":ready,"stages":rows,
        "final":snapshot(state,&model),"deleteInventory":disk::snapshot(state).await?,
        "initialRecords":fixture::initial_records(&model),"noLiveNetwork":true,
        "limits":["Fresh-process recovery from exact closed fixture SQLite bytes; not an OS-crash or in-place restart.",
            "Restore uses the inert original record and never reconstructs a deletion intent or ACK."]}),
    )
}
