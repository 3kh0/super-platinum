//! Exact List-only rich-title lifecycle through original editor and journal.
use super::group_completion::{
    capture, choose, command, current_blocks, query, reload, snapshot, wait,
};
use super::*;
use crate::slack_lists::comments::fixture::completion::{self as data, title_users as source};
use super_platinum_core::slack::Error;
mod empty_canvas;
mod lifecycle;
mod recovery;
mod retirement;
type Desktop = dioxus::desktop::DesktopContext;

async fn source_ready(state: Signal<ShellState>, m: &Model) -> Result<(), String> {
    wait(state, m, |v| {
        v["completion"]["current"] == true
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completion"]["labels"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(source::label())))
    })
    .await?;
    Ok(())
}
async fn next(state: Signal<ShellState>, m: &Model, kind: &str) -> Result<usize, String> {
    let v = wait(state, m, |v| {
        v["completionSource"]["titleUsers"]["pending"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["kind"] == kind)
    })
    .await?;
    v["completionSource"]["titleUsers"]["pending"][0]["serial"]
        .as_u64()
        .map(|v| v as usize)
        .ok_or("Missing held serial".into())
}
fn release(
    state: Signal<ShellState>,
    p: &mut Value,
    serial: usize,
    kind: &str,
    success: bool,
) -> Result<(), String> {
    let result = if !success {
        Err(Error::Offline(
            "Synthetic List title member unavailable".into(),
        ))
    } else {
        Ok((
            if kind == "discovery" {
                source::discovery_raw(&state.peek()).map_err(|e| e.to_string())?
            } else {
                source::success()
            },
            None,
        ))
    };
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(source::release(serial, result)?);
    Ok(())
}
async fn shot(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    name: &str,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    p["sourceFiles"] =
        crate::profile_edit::sections::rich::search::file_source::fixture_completion_proof(
            &state.peek(),
        )
        .unwrap_or_else(|_| json!({"unavailable":true}));
    // Reuse original strict geometry/model/journal capture for mounted editor stages.
    capture(state, m, d, name, p, rows, name == "sending").await
}
async fn action(d: &Desktop, p: &mut Value, id: &str) -> Result<(), String> {
    if id == "slack-list-comment-send" {
        settled(d).await?;
    }
    let native = checked(d, &format!("window.__listCompose.click({})", json!(id))).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"click","id":id,"value":native}));
    Ok(())
}
async fn pending_shot(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    name: &str,
    p: &mut Value,
    rows: &mut Vec<Value>,
    original: &Disk,
) -> Result<(), String> {
    stable_drafts(state, m).await?;
    p["sourceFiles"] =
        crate::profile_edit::sections::rich::search::file_source::fixture_completion_proof(
            &state.peek(),
        )
        .unwrap_or_else(|_| json!({"unavailable":true}));
    let before = snapshot(state, m);
    let journal = original.read().await?;
    let native = wait_dom(d, &format!("({})()", include_str!("title_users/native.js"))).await?;
    if before != snapshot(state, m) {
        return Err(format!("Title state changed before {name}"));
    }
    let capture = harness::capture(d, name).await?;
    let after = checked(d, &format!("({})()", include_str!("title_users/native.js"))).await?;
    if before != snapshot(state, m) || native != after || journal != original.read().await? {
        return Err(format!("Title state changed during {name}"));
    }
    let mut proof = p.clone();
    proof["journal"] = journal;
    rows.push(
        json!({"stage":name,"model":before,"geometry":native,"proof":proof,"capture":capture}),
    );
    Ok(())
}
struct Disk {
    path: std::path::PathBuf,
    account: String,
    scope: super_platinum_core::slack_lists::comments::journal::Scope,
}
impl Disk {
    fn capture(s: &ShellState, seed: &Value) -> Result<Self, String> {
        Ok(Self {
            path: compose::path(s).ok_or("Missing isolated journal")?,
            account: s.core.active_account.clone().ok_or("Missing account")?,
            scope: serde_json::from_value(seed["scope"].clone())
                .map_err(|_| "Malformed original scope")?,
        })
    }
    async fn read(&self) -> Result<Value, String> {
        let (path, account, scope) = (self.path.clone(), self.account.clone(), self.scope.clone());
        tokio::task::spawn_blocking(move || {
            let j =
                super_platinum_core::slack_lists::comments::journal::Journal::open(path, &account)
                    .map_err(|e| e.to_string())?;
            serde_json::to_value(j.inventory(&scope).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "Journal read failed")?
    }
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &Desktop,
    case: &str,
) -> Result<Value, String> {
    if !source::named() && !data::empty_canvas::named() {
        return Err("Exact List title fixture required".into());
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
    let empty_seed = if data::empty_canvas::named() {
        crate::slack_lists::comments::fixture::cold_files::reply::seed(&state.peek())
            .map_err(|e| e.to_string())?
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
    let disk = Disk::capture(&state.peek(), &seed)?;
    write(d).await?;
    type_text(
        d,
        "Final review \u{4e2d}\u{1f642}\nSecond line <literal> & *punctuation*.",
    )
    .await?;
    checked(d, "window.__listCompose.bold()").await?;
    settled(d).await?;
    edits::witness(d).await?;
    let original = current_blocks(d).await?;
    let mut p = json!({"seed":seed,"originalA":original,"expectedBlocks":original,"inputs":[],"releases":[],"runtimeForgotten":false});
    if data::empty_canvas::named() {
        p["emptyCanvas"] = json!({"case":case,"label":source::label(),"fileSeed":empty_seed});
    }
    let mut rows = vec![];
    shot(state, &m, d, "draft", &mut p, &mut rows).await?;
    let result = match case {
        "user-title-known"
        | "user-title-hidden"
        | "empty-canvas-lifecycle"
        | "link-title-lifecycle" => lifecycle::run(state, &m, d, &mut p, &mut rows).await,
        "user-title-recovery" => recovery::query_recovery(state, &m, d, &mut p, &mut rows).await,
        "user-title-draft-recovery" => {
            recovery::draft_recovery(state, &m, d, &mut p, &mut rows, &disk).await
        }
        "user-title-retirement" => retirement::run(state, &m, d, &mut p, &mut rows, &disk).await,
        "empty-canvas-recovery" => empty_canvas::run(state, &m, d, &mut p, &mut rows, &disk).await,
        _ => Err("Unknown List title case".into()),
    };
    if let Err(error) = result {
        return Err(format!(
            "{error}; retained title evidence: {}",
            json!({"stages":rows,"model":snapshot(state,&m),"proof":p})
        ));
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limitations":["Exact isolated List fixture uses synthetic request callbacks with no HTTP capability.","Native DOM keyboard/pointer events are not trusted OS input or IME.","Durable reopen occurs within this process; no restart-offline title claim.","Popup screenshots cover visible rows; the complete ordered catalog is retained separately."]}),
    )
}
