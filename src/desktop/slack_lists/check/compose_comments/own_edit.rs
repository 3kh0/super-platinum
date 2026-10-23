//! Native own-comment edit, independently retained reply and real journal readback.
use super::*;
use crate::slack_lists::comments::{fixture::own_edit as data, own_edit as editing};
mod custody;
mod noop;
mod ordered;
mod plain;
mod recovery;
mod restart;
mod storage;
mod title_users;

const STAGES: &[&str] = &[
    "draft",
    "editing",
    "cancelled",
    "saving",
    "saved",
    "readback",
    "reopened",
];

fn script(action: &str, argument: Value) -> String {
    format!(
        "({})({},{},({}))",
        include_str!("own_edit.js"),
        json!(action),
        argument,
        include_str!("reaction_actions/text.js")
    )
}
async fn command(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    argument: Value,
) -> Result<Value, String> {
    checked(d, &script(action, argument)).await
}
async fn button(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<Value, String> {
    command(d, "click", json!({"id":id})).await
}
async fn editor(d: &dioxus::desktop::DesktopContext, present: bool) -> Result<(), String> {
    wait_dom(d,&format!("(()=>{{const e=document.getElementById('slack-list-comment-edit-draft');return {{ok:{present}?!!e&&!!window.superPlatinumRichEditor?.snapshot(e):!e}};}})()")).await?;
    Ok(())
}
async fn wait_fixture(
    state: Signal<ShellState>,
    expected: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..400 {
        let snapshot = data::snapshot(&state.peek());
        if expected(&snapshot) {
            return Ok(snapshot);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Own edit fixture did not settle: {}",
        data::snapshot(&state.peek())
    ))
}
fn snapshot(state: Signal<ShellState>, model: &Model) -> Value {
    let mut value = json!({"comments":compose::snapshot(&state.peek(),model),
        "fixture":data::snapshot(&state.peek()),"edit":editing::snapshot(&state.peek())});
    if fixture::plain::named()
        || fixture::ordered::named()
        || data::noop_named()
        || data::runs_named()
        || matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-recovery"
                | "own-edit-storage"
                | "own-edit-restart-unsent-exercise"
                | "own-edit-restart-unsent-restore"
                | "own-edit-restart-unknown-exercise"
                | "own-edit-restart-unknown-restore")
        )
    {
        let shell = state.peek();
        value["capture"] = editing::presentation(&shell).map(|view| {
            let native = editing::native(&shell, &view.binding);
            let mut capture = json!({"scope":view.binding.scope(),"generation":view.binding.generation(),
                "mount":native.as_ref().map(|m|m.mount()),"failed":native.as_ref().is_some_and(|m|m.failed()),
                "canSave":view.can_save,"canCancel":view.can_cancel,"canCheck":view.can_check,
                "canRetry":view.can_retry_capture,"busy":view.busy});
            if fixture::plain::named() || fixture::ordered::named() || data::noop_named() || data::runs_named() || std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref() == Ok("own-edit-storage") {
                capture["finishing"] = json!(view.finishing);
            }
            capture
        }).unwrap_or(Value::Null);
    }
    if data::title_users::named() {
        value["titleSource"] = fixture::completion::title_users::snapshot(&state.peek());
        value["transcriptTitles"] =
            crate::slack_lists::comments::file_acquisition::snapshot(&state.peek());
    }
    if fixture::completion::empty_canvas::own_edit_named() {
        value["coldFiles"] = fixture::cold_files::snapshot(&state.peek());
    }
    value
}
async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    stable_drafts(state, model).await?;
    let edit_inventory = if data::title_users::named() {
        Some(data::restart::snapshot(state).await?)
    } else {
        None
    };
    let geometry = wait_dom(d, &script("state", json!({"ts":data::RECEIVED_TS}))).await?;
    let journal = compose::journal_snapshot(state).await?;
    if geometry["reply"]["blocks"] != proof["replyBlocks"]
        || journal["blocks"] != proof["replyBlocks"]
    {
        return Err(format!("Independent reply changed at {name}: {journal}"));
    }
    if name != "reopened" && geometry["reply"]["sameNode"] != true {
        return Err("Original reply node was replaced during editing".into());
    }
    let before = snapshot(state, model);
    let png = harness::capture(d, name).await?;
    if before != snapshot(state, model) {
        return Err(format!("Own edit model changed during {name} capture"));
    }
    stages.push(json!({"stage":name,"geometry":geometry,"journal":journal,"model":before,"proof":proof,"capture":png}));
    if let Some(inventory) = edit_inventory {
        if inventory != data::restart::snapshot(state).await? {
            return Err("Edit journal changed during capture".into());
        }
        stages.last_mut().unwrap()["editInventory"] = inventory;
    }
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if fixture::plain::named() {
        return plain::run(state, d).await;
    }
    if fixture::ordered::named() {
        return ordered::run(state, d).await;
    }
    if data::noop_named() || data::runs_named() {
        return noop::run(state, d).await;
    }
    if std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
        == Ok("own-edit-storage")
    {
        return storage::run(state, d).await;
    }
    if restart::named() {
        return restart::run(state, d).await;
    }
    let readiness = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let source = data::seed(state)?;
    let empty_seed = if fixture::completion::empty_canvas::own_edit_named() {
        let seed = fixture::cold_files::reply::seed(&state.peek()).map_err(|e| e.to_string())?;
        fixture::cold_files::configure(
            &state.peek(),
            fixture::cold_files::CANVAS,
            fixture::cold_files::Mode::Success,
        )
        .map_err(|e| e.to_string())?;
        seed
    } else {
        Value::Null
    };
    checked(d, include_str!("../comments.js")).await?;
    checked(d, include_str!("../compose_comments.js")).await?;
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
    let mut proof =
        json!({"source":source,"replySeed":seed,"replyBlocks":remembered["blocks"],"actions":[]});
    if fixture::completion::empty_canvas::own_edit_named() {
        proof["emptyCanvas"] = json!({"initialResponses":empty_seed,"plain":fixture::completion::empty_canvas::plain_seed(),"label":"Untitled"});
    }
    if data::title_users::named() {
        title_users::initial(state, &model, d, &mut proof).await?;
    }
    let mut stages = Vec::new();
    capture(state, &model, d, "draft", &proof, &mut stages).await?;
    let case = std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE")
        .map_err(|_| "Own edit case missing")?;
    if data::title_users::named()
        && (case == "own-edit-title-recovery"
            || case == "own-edit-title-held-cancel"
            || case == "own-edit-empty-recovery")
    {
        title_users::run(state, &model, d, &mut proof, &mut stages).await?;
    } else if case == "own-edit-recovery" {
        recovery::run(state, &model, d, &mut proof, &mut stages).await?;
    } else if case == "own-edit-custody" {
        custody::run(state, &model, d, &mut proof, &mut stages).await?;
    } else if case == "own-edit" || data::title_users::named() {
        command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
        editor(d, true).await?;
        if data::title_users::named() {
            title_users::ready(state, &model, d, &mut proof).await?;
        }
        let typed = command(d, "append", json!({"text":" Cancel this edit"})).await?;
        proof["cancelledBlocks"] = typed["blocks"].clone();
        if data::title_users::named() {
            title_users::durable(state, &proof["cancelledBlocks"]).await?;
        }
        capture(state, &model, d, "editing", &proof, &mut stages).await?;
        button(d, "slack-list-comment-edit-cancel").await?;
        editor(d, false).await?;
        capture(state, &model, d, "cancelled", &proof, &mut stages).await?;
        command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
        editor(d, true).await?;
        if data::title_users::named() {
            title_users::ready(state, &model, d, &mut proof).await?;
        }
        let typed = command(
            d,
            "append",
            json!({"text":" Saved change B \u{65e5}\u{1f98a}"}),
        )
        .await?;
        proof["editedBlocks"] = typed["blocks"].clone();
        if data::title_users::named() {
            title_users::durable(state, &proof["editedBlocks"]).await?;
        }
        data::mode(&state.peek(), data::Mode::HeldSuccess)?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comment-edit-save')?.disabled===false})",
        )
        .await?;
        button(d, "slack-list-comment-edit-save").await?;
        wait_fixture(state, |v| v["held"] == true).await?;
        proof["doubleSave"] = command(d, "double-save", Value::Null).await?;
        capture(state, &model, d, "saving", &proof, &mut stages).await?;
        proof["release"] = data::release(&state.peek())?;
        editor(d, false).await?;
        capture(state, &model, d, "saved", &proof, &mut stages).await?;
        data::publish(&state.peek())?;
        button(d, "slack-list-comments-refresh").await?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        capture(state, &model, d, "readback", &proof, &mut stages).await?;
        button(d, "slack-list-comments-back").await?;
        wait_dom(d, "({ok:!document.getElementById('slack-list-comments')})").await?;
        stable_drafts(state, &model).await?;
        if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
            return Err("Original reply was not safely forgettable".into());
        }
        click(d, "slack-list-record-comments").await?;
        wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
        write(d).await?;
        proof["runtimeForgotten"] = json!(true);
        capture(state, &model, d, "reopened", &proof, &mut stages).await?;
        if stages
            .iter()
            .map(|stage| stage["stage"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            != STAGES
        {
            return Err("Own edit stage order".into());
        }
    } else {
        return Err("Unknown own edit case".into());
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,
        "stages":stages,"final":snapshot(state,&model),"initialRecords":fixture::initial_records(&model),"noLiveNetwork":true,
        "limits":if fixture::completion::empty_canvas::own_edit_named() {vec!["Closed native interaction, no live Slack writes.","No-user empty Canvas titles use independently acquired metadata and explicit English application presentation.","Forced retirement is an explicit synthetic List visit replacement; durable reopen is same-process private SQLite, not a process restart."]}else if data::title_users::named() {vec!["Closed native interaction, no live Slack writes.","Known member title recovery; absent member recovery belongs to the prior accepted milestone.","Forced retirement is an explicit synthetic List visit replacement, not human navigation or process restart."]}else{vec!["Closed native interaction, no live Slack writes.","Mounted edit custody only; edit restart durability is not claimed."]}}),
    )
}
