//! Two fresh processes recover only through the closed, byte-identical database.
//! The runner owns process closure, SQLite transfer and cross-process comparison.
use super::*;
use data::restart as disk;

const EDIT_B: &str = " Persisted edit B \u{65e5}\u{1f98a}";
const NEWER_C: &str = " Newer edit C \u{65e5}\u{1f642}";
const NEGATIVE: &str = "The edited content could not be confirmed. Both drafts are retained; this save will not be sent again.";
const NEWER: &str = "The saved content was found. Your newer edit is retained; Cancel discards that edit and returns to your reply.";

pub(super) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
        Ok("own-edit-restart-unsent-exercise"
            | "own-edit-restart-unsent-restore"
            | "own-edit-restart-unknown-exercise"
            | "own-edit-restart-unknown-restore")
    )
}
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
        "Restart state did not settle: {}",
        snapshot(state, model)
    ))
}
async fn durable_blocks(state: Signal<ShellState>, blocks: &Value) -> Result<Value, String> {
    for _ in 0..300 {
        let inventory = disk::snapshot(state).await?;
        if inventory["blocks"] == *blocks && !inventory["seed"].is_null() {
            return Ok(inventory);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Native edit was not committed to its independent SQLite draft".into())
}
async fn ready(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    unknown: bool,
) -> Result<(), String> {
    wait(state, model, |v| {
        v["capture"]["failed"] == false
            && v["edit"]["active"]["nativeReady"] == true
            && v["capture"]["busy"] == false
            && if unknown {
                v["capture"]["canCheck"] == true
                    && v["capture"]["canSave"] == false
                    && v["capture"]["canCancel"] == false
            } else {
                v["capture"]["canSave"] == true && v["capture"]["canCancel"] == true
            }
    })
    .await?;
    let control = if unknown { "check" } else { "save" };
    wait_dom(d,&format!("({{ok:document.getElementById('slack-list-comment-edit-{control}')?.disabled===false}})")).await?;
    Ok(())
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
    counts: (u64, usize, usize),
    proof: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    wait(state, model, |v| {
        v["comments"]["ledger"]["completed"] == counts.0
            && v["fixture"]["policy"]
                .as_array()
                .is_some_and(|v| v.len() == counts.1)
            && v["fixture"]["updates"]
                .as_array()
                .is_some_and(|v| v.len() == counts.2)
    })
    .await?;
    let inventory = disk::snapshot(state).await?;
    capture(state, model, d, name, proof, stages).await?;
    if inventory != disk::snapshot(state).await? {
        return Err(format!("Persisted edit changed during {name} capture"));
    }
    let stage = stages.last_mut().ok_or("Restart capture missing")?;
    stage["editInventory"] = inventory;
    if !stage["model"]["edit"]["active"].is_null() {
        if stage["geometry"]["edit"]["sameNode"] != true
            || stage["geometry"]["edit"]["blocks"] != stage["editInventory"]["blocks"]
            || stage["model"]["edit"]["active"]["blocks"] != stage["editInventory"]["blocks"]
        {
            return Err("Original mounted edit and durable draft diverged".into());
        }
        let witness = &proof["editRemembered"];
        if stage["model"]["capture"]["scope"] != witness["scope"]
            || stage["model"]["capture"]["mount"] != witness["mount"]
            || witness["generation"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                != stage["model"]["capture"]["generation"].as_u64()
        {
            return Err("Original edit mount changed during this process".into());
        }
    }
    Ok(())
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let case = std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE")
        .map_err(|_| "Restart case missing")?;
    let restore = case.ends_with("-restore");
    let unknown = case.contains("-unknown-");
    let readiness = harness::wait_runner(state).await?;
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
    let initial_edit = disk::snapshot(state).await?;
    let initial_reply = compose::journal_snapshot(state).await?;
    if restore {
        if initial_edit["seed"].is_null()
            || initial_edit["blocks"].as_array().is_none_or(Vec::is_empty)
            || initial_reply["blocks"].as_array().is_none_or(Vec::is_empty)
            || (unknown && initial_edit["attempt"]["phase"] != "Uncertain")
            || (!unknown && !initial_edit["attempt"].is_null())
        {
            return Err("Restore requires original persisted reply, edit and disposition".into());
        }
    } else if initial_edit["revision"] != 0 || !initial_edit["seed"].is_null() {
        return Err("Exercise requires a new isolated edit journal".into());
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
        "initialEditInventory":initial_edit,"initialReplyInventory":initial_reply,
        "processId":std::process::id(),"scenario":if unknown{"unknown"}else{"unsent"},
        "phase":if restore{"restore"}else{"exercise"},"editSuffix":EDIT_B,"newerSuffix":NEWER_C});
    let mut stages = Vec::new();
    if restore {
        if disk::snapshot(state).await? != initial_edit
            || proof["replyBlocks"] != initial_reply["blocks"]
        {
            return Err("Opening the reply changed the persisted recovery source".into());
        }
        take(
            state,
            &model,
            d,
            "restored-reply",
            (4, 0, 0),
            &proof,
            &mut stages,
        )
        .await?;
    } else {
        take(
            state,
            &model,
            d,
            "exercise-draft",
            (4, 0, 0),
            &proof,
            &mut stages,
        )
        .await?;
    }
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    ready(state, &model, d, restore && unknown).await?;
    if restore {
        proof["editedBlocks"] = initial_edit["blocks"].clone();
    } else {
        let typed = command(d, "append", json!({"text":EDIT_B})).await?;
        proof["editedBlocks"] = typed["blocks"].clone();
    }
    durable_blocks(state, &proof["editedBlocks"]).await?;
    ready(state, &model, d, restore && unknown).await?;
    proof["editRemembered"] = command(d, "remember-edit", Value::Null).await?;
    if proof["editRemembered"]["blocks"] != proof["editedBlocks"] {
        return Err("Native recovered edit differs from original persisted content".into());
    }
    if restore {
        if disk::snapshot(state).await? != initial_edit {
            return Err("Loading the original edit rewrote its recovery inventory".into());
        }
        if unknown {
            proof["blocked"] = command(d, "blocked-controls", Value::Null).await?;
        }
        take(
            state,
            &model,
            d,
            "restored-edit",
            (4, 0, 0),
            &proof,
            &mut stages,
        )
        .await?;
        if unknown {
            button(d, "slack-list-comment-edit-check").await?;
            wait(state, &model, |v| {
                v["comments"]["ledger"]["completed"] == 8
                    && v["capture"]["canCheck"] == true
                    && v["edit"]["attempts"][0]["phase"] == "unknown"
            })
            .await?;
            notice(d, NEGATIVE).await?;
            take(
                state,
                &model,
                d,
                "unconfirmed-checked",
                (8, 0, 0),
                &proof,
                &mut stages,
            )
            .await?;
            let typed = command(d, "append", json!({"text":NEWER_C})).await?;
            proof["newerBlocks"] = typed["blocks"].clone();
            durable_blocks(state, &proof["newerBlocks"]).await?;
            ready(state, &model, d, true).await?;
            if proof["newerBlocks"] == proof["editedBlocks"] {
                return Err("Newer edit was not made".into());
            }
            take(
                state,
                &model,
                d,
                "newer-edit",
                (8, 0, 0),
                &proof,
                &mut stages,
            )
            .await?;
            proof["publication"] = disk::publish_saved(state).await?;
            button(d, "slack-list-comment-edit-check").await?;
            wait(state, &model, |v| {
                v["comments"]["ledger"]["completed"] == 12
                    && v["edit"]["attempts"][0]["phase"] == "observed"
                    && v["capture"]["canCancel"] == true
                    && v["capture"]["busy"] == false
            })
            .await?;
            notice(d, NEWER).await?;
            let observed = durable_blocks(state, &proof["newerBlocks"]).await?;
            if observed["attempt"]["phase"] != "Observed"
                || !observed["attempt"]["raw"].is_null()
                || observed["attempt"]["readback"].is_null()
            {
                return Err(
                    "Fresh readback did not durably resolve the original unknown attempt".into(),
                );
            }
            take(
                state,
                &model,
                d,
                "observed-newer",
                (12, 0, 0),
                &proof,
                &mut stages,
            )
            .await?;
        }
        button(d, "slack-list-comment-edit-cancel").await?;
        editor(d, false).await?;
        wait(state, &model, |v| v["edit"]["active"].is_null()).await?;
        let cleared = disk::snapshot(state).await?;
        if !cleared["seed"].is_null()
            || !cleared["attempt"].is_null()
            || cleared["blocks"] != json!([])
        {
            return Err("Explicit Cancel did not clear only the edit draft".into());
        }
        take(
            state,
            &model,
            d,
            "cancelled",
            (if unknown { 12 } else { 4 }, 0, 0),
            &proof,
            &mut stages,
        )
        .await?;
        button(d, "slack-list-comments-back").await?;
        wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
        stable_drafts(state, &model).await?;
        if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
            return Err("Original durable reply was not safely forgettable".into());
        }
        click(d, "slack-list-record-comments").await?;
        wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
        write(d).await?;
        proof["runtimeForgotten"] = json!(true);
        take(
            state,
            &model,
            d,
            "reopened",
            (if unknown { 16 } else { 8 }, 0, 0),
            &proof,
            &mut stages,
        )
        .await?;
    } else {
        take(
            state,
            &model,
            d,
            "exercise-edit",
            (8, 4, 0),
            &proof,
            &mut stages,
        )
        .await?;
        if unknown {
            data::mode(&state.peek(), data::Mode::Unconfirmed)?;
            button(d, "slack-list-comment-edit-save").await?;
            wait(state, &model, |v| {
                v["edit"]["attempts"][0]["phase"] == "unknown"
                    && v["capture"]["canCheck"] == true
                    && v["capture"]["busy"] == false
            })
            .await?;
            ready(state, &model, d, true).await?;
            proof["blocked"] = command(d, "blocked-controls", Value::Null).await?;
            let saved = durable_blocks(state, &proof["editedBlocks"]).await?;
            if saved["attempt"]["phase"] != "Uncertain" || !saved["attempt"]["raw"].is_null() {
                return Err("Unknown attempt was not committed before process exit".into());
            }
            take(
                state,
                &model,
                d,
                "exercise-unconfirmed",
                (12, 8, 1),
                &proof,
                &mut stages,
            )
            .await?;
        }
    }
    let terminal_inventory = disk::snapshot(state).await?;
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,
        "stages":stages,"final":snapshot(state,&model),"editInventory":terminal_inventory,
        "initialRecords":fixture::initial_records(&model),"noLiveNetwork":true,
        "limits":["Fresh-process recovery from exact closed SQLite bytes; not OS-crash or in-place restart acceptance.",
            "The restore process reads an inert original record and never reconstructs an update intent.",
            "No new storage-fault variant; native capture and storage failure acceptance are separate."]}),
    )
}
