//! Actual received whole-file clicks, original native draft capture and disk recovery.
use super::*;
use crate::slack_lists::comments::fixture::{completion as data, file_navigation as nav};
use super_platinum_core::slack_lists::comments::journal::{Journal, Scope};
mod cold;
mod custody;

const SUCCESS: &[&str] = &[
    "draft",
    "canvas-open",
    "canvas-return",
    "list-open",
    "list-return",
];
const CUSTODY: &[&str] = &[
    "composition-refused",
    "composition-recovered",
    "save-refused",
    "save-recovered",
    "retry-open",
    "retry-return",
    "source-refused",
];
const CANVAS_EDIT: &str = "\nCanvas click \u{4e2d}\u{1f642}";
const LIST_EDIT: &str = "\nList click \u{65e5}\u{1f98a}";

struct Disk {
    path: std::path::PathBuf,
    account: String,
    scope: Scope,
}
impl Disk {
    fn capture(shell: &ShellState, seed: &Value) -> Result<Self, String> {
        Ok(Self {
            path: compose::path(shell).ok_or("Isolated journal missing")?,
            account: shell.core.active_account.clone().ok_or("Account absent")?,
            scope: serde_json::from_value(seed["scope"].clone())
                .map_err(|_| "Journal scope malformed")?,
        })
    }
    async fn read(&self) -> Result<Value, String> {
        let path = self.path.clone();
        let account = self.account.clone();
        let scope = self.scope.clone();
        tokio::task::spawn_blocking(move || {
            let journal = Journal::open(path, &account).map_err(|e| e.to_string())?;
            serde_json::to_value(journal.inventory(&scope).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "Independent journal task failed")?
    }
}
fn snapshot(state: Signal<ShellState>, m: &Model) -> Value {
    let mut v = group_completion::snapshot(state, m);
    v["navigation"] = nav::snapshot(&state.peek());
    if crate::slack_lists::comments::fixture::cold_files::named() {
        v["coldFiles"] = crate::slack_lists::comments::fixture::cold_files::snapshot(&state.peek());
        v["coldAcquisition"] =
            crate::slack_lists::comments::file_acquisition::snapshot(&state.peek());
    }
    let shell = state.peek();
    v["route"] = json!({"canvasActive":shell.overlay==Some(crate::model::Overlay::Canvas),
        "listActive":shell.overlay==Some(crate::model::Overlay::SlackList),
        "canvasId":shell.canvas.as_ref().map(|v|&v.id),"listId":shell.slack_lists.view.as_ref().map(|v|&v.id)});
    v
}
fn expression(action: &str, arg: &Value) -> String {
    format!(
        "({})({},{},({}))",
        include_str!("file_navigation.js"),
        json!(action),
        arg,
        include_str!("reaction_actions/text.js")
    )
}
async fn command(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(d, &expression(action, &arg)).await
}
fn target(id: &str) -> Value {
    let canvas = id == nav::CANVAS;
    json!({"id":id,"kind":if canvas{"canvas"}else{"file"},"title":if canvas{"Alex Canvas"}else{"Alex List"},
        "content":if canvas{nav::CANVAS_CONTENT}else{nav::LIST_CONTENT}})
}
async fn click_file(
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    id: &str,
    append: Option<&str>,
) -> Result<(), String> {
    let clicked = command(
        d,
        "click",
        json!({"id":id,"ts":nav::RECEIVED_TS,"append":append}),
    )
    .await?;
    if clicked["control"] != json!({"tag":"BUTTON","type":"button","href":null,"disabled":false}) {
        return Err("File click was not the original native button".into());
    }
    p["expectedBlocks"] = clicked["draft"]["blocks"].clone();
    p["clicks"].as_array_mut().unwrap().push(clicked);
    Ok(())
}
async fn restored(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
) -> Result<(), String> {
    stable_drafts(state, m).await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Original saved draft could not be forgotten".into());
    }
    let prior_generation = state.peek().slack_lists.view.as_ref().map(|v| v.generation);
    nav::return_original(state)?;
    let returned = {
        let shell = state.peek();
        let view = shell
            .slack_lists
            .view
            .as_ref()
            .ok_or("Original List missing after return")?;
        let original = nav::snapshot(&shell);
        if Some(view.generation) == prior_generation
            || original["originalDocument"]["id"] != view.id
        {
            return Err("Original List did not acquire a fresh return visit".into());
        }
        json!({"id":view.id,"generation":view.generation,"record":"R_LIST_000"})
    };
    let ready = wait_dom(d, &expression("returned-ready", &returned)).await?;
    if state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .is_none_or(|v| returned["id"] != v.id || returned["generation"] != v.generation)
    {
        return Err("Returned List changed before opening the original item".into());
    }
    p["returnReady"] = ready;
    open(d, "R_LIST_000").await?;
    write(d).await?;
    stable_drafts(state, m).await?;
    p["runtimeForgotten"] = json!(true);
    Ok(())
}
async fn capture(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    disk: &Disk,
    name: &str,
    p: &Value,
    rows: &mut Vec<Value>,
    target_id: Option<&str>,
    locked: bool,
) -> Result<(), String> {
    stable_drafts(state, m).await?;
    let action = if target_id.is_some() {
        "target"
    } else {
        "source"
    };
    let arg = if let Some(id) = target_id {
        target(id)
    } else {
        let view = crate::slack_lists::comments::draft::presentation(&state.peek())
            .ok_or("Original draft presentation missing before capture")?;
        let notices: Vec<_> = view.status.iter().chain(view.error.iter()).collect();
        json!({"generation":view.binding.generation(),"blocks":p["expectedBlocks"],"notices":notices})
    };
    let cold_geometry = cold::geometry(d, p).await?;
    // Cold title publication may repaint transcript text while the editor is
    // already ready. Sample the shared source geometry only after those exact
    // title/status controls have reached the requested state.
    let geometry = wait_dom(d, &expression(action, &arg)).await?;
    let before = snapshot(state, m);
    let mut proof = p.clone();
    if !cold_geometry.is_null() {
        proof["coldGeometry"] = cold_geometry.clone();
    }
    proof["journal"] = if locked {
        Value::Null
    } else {
        disk.read().await?
    };
    if !locked && proof["journal"]["blocks"] != p["expectedBlocks"] {
        return Err(format!("Original journal lost native draft at {name}"));
    }
    if target_id.is_none() && geometry["native"]["blocks"] != p["expectedBlocks"] {
        return Err(format!("Original editor differs at {name}"));
    }
    let image = harness::capture(d, name).await?;
    let after_geometry = command(d, action, arg).await?;
    let cold_after = cold::geometry(d, p).await?;
    let after = snapshot(state, m);
    let after_journal = if locked {
        Value::Null
    } else {
        disk.read().await?
    };
    let mut changes = Vec::new();
    differences("model", &before, &after, &mut changes);
    differences("geometry", &geometry, &after_geometry, &mut changes);
    differences("journal", &proof["journal"], &after_journal, &mut changes);
    differences("coldGeometry", &cold_geometry, &cold_after, &mut changes);
    if before != after
        || geometry != after_geometry
        || proof["journal"] != after_journal
        || cold_geometry != cold_after
    {
        return Err(format!(
            "File navigation capture changed during {name}: {}",
            json!(changes)
        ));
    }
    rows.push(json!({"stage":name,"model":before,"geometry":geometry,"geometryAfter":after_geometry,"proof":proof,"capture":image}));
    Ok(())
}
fn differences(path: &str, before: &Value, after: &Value, changes: &mut Vec<Value>) {
    if before == after || changes.len() >= 8 {
        return;
    }
    if let (Some(a), Some(b)) = (before.as_object(), after.as_object()) {
        let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for key in keys {
            if changes.len() >= 8 {
                break;
            }
            if a.contains_key(key) != b.contains_key(key) {
                changes.push(json!({"path":format!("{path}.{key}"),"beforePresent":a.contains_key(key),"afterPresent":b.contains_key(key)}));
            } else {
                differences(&format!("{path}.{key}"), &before[key], &after[key], changes);
            }
        }
    } else if let (Some(a), Some(b)) = (before.as_array(), after.as_array()) {
        if a.len() != b.len() {
            changes.push(json!({"path":format!("{path}.length"),"before":a.len(),"after":b.len()}));
        }
        for (i, (a, b)) in a.iter().zip(b).enumerate() {
            differences(&format!("{path}[{i}]"), a, b, changes);
        }
    } else {
        let compact = |v: &Value| v.to_string().chars().take(240).collect::<String>();
        changes.push(json!({"path":path,"before":compact(before),"after":compact(after)}));
    }
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    if matches!(case, "cold-files" | "cold-files-recovery") {
        return cold::run(state, d, case).await;
    }
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Workspace missing")?
        .rt_generation = 7;
    data::seed(state).await?;
    let received = nav::seed(&mut state.write())?;
    checked(d, include_str!("../comments.js")).await?;
    checked(d, include_str!("../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    let disk = Disk::capture(&state.peek(), &seed)?;
    write(d).await?;
    type_text(d, "Final review \u{4e2d}\u{1f642}").await?;
    checked(d, "window.__listCompose.bold()").await?;
    settled(d).await?;
    let mut p = json!({"seed":seed,"received":received,"expectedBlocks":[],"inputs":[],"clicks":[],"runtimeForgotten":false,"storage":null});
    group_completion::query(
        state,
        &m,
        d,
        &mut p,
        "prepare",
        "@alex",
        data::Mode::Success,
    )
    .await?;
    group_completion::wait(state, &m, |v| {
        v["completion"]["settledBatch"] == true
            && v["completion"]["values"]
                .as_array()
                .is_some_and(|r| r.iter().any(|r| r["value"]["file_id"] == nav::CANVAS))
    })
    .await?;
    command(d, "dismiss", Value::Null).await?;
    p["expectedBlocks"] = group_completion::current_blocks(d).await?;
    stable_drafts(state, &m).await?;
    let mut rows = Vec::new();
    if case == "file-navigation" {
        capture(state, &m, d, &disk, "draft", &p, &mut rows, None, false).await?;
        for (id, edit, opened, returned) in [
            (nav::CANVAS, CANVAS_EDIT, "canvas-open", "canvas-return"),
            (nav::TARGET_LIST, LIST_EDIT, "list-open", "list-return"),
        ] {
            p["runtimeForgotten"] = json!(false);
            click_file(d, &mut p, id, Some(edit)).await?;
            capture(state, &m, d, &disk, opened, &p, &mut rows, Some(id), false).await?;
            restored(state, &m, d, &mut p).await?;
            capture(state, &m, d, &disk, returned, &p, &mut rows, None, false).await?;
        }
    } else {
        custody::run(state, &m, d, &disk, &readiness, &mut p, &mut rows).await?;
    }
    let stages = if case == "file-navigation" {
        SUCCESS
    } else {
        CUSTODY
    };
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        != stages
    {
        return Err("Navigation stage order changed".into());
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,
        "final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,
        "limitations":["Closed target responses exercise actual native loaders; no live Slack.","Composition is DOM CompositionEvent, not an OS IME.","Pending-send/account/visit/raw-row retirement covered by controller tests, not these native cases.","Source refusal is actual local file source invalidation, not a server event."]}),
    )
}
