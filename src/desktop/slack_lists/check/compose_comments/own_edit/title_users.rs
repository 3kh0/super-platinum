//! Named own-edit title proof. All edits, Retry, Save and Cancel use native controls.
use super::*;
use crate::slack_lists::comments::fixture::completion::title_users as source;
use super_platinum_core::slack::Error;
mod empty_canvas;
mod held;
mod recovery;
type Desktop = dioxus::desktop::DesktopContext;

async fn wait(
    state: Signal<ShellState>,
    m: &Model,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..500 {
        let v = snapshot(state, m);
        if predicate(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Own-edit title state did not settle: {}",
        snapshot(state, m)
    ))
}
fn probe(action: &str) -> String {
    format!(
        "({})({})",
        include_str!("title_users/native.js"),
        json!(action)
    )
}
pub(super) async fn initial(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
) -> Result<(), String> {
    let original = wait(state, m, |v| {
        v["transcriptTitles"]["rows"].as_array().is_some_and(|a| {
            a.iter().filter(|r| r["id"] == source_file()).count() > 0
                && a.iter()
                    .filter(|r| r["id"] == source_file())
                    .all(|r| r["titleUsers"]["current"] == true)
        })
    })
    .await?;
    p["initialTranscript"] = original["transcriptTitles"].clone();
    p["initialTitleSource"] = original["titleSource"].clone();
    p["titleReady"] = json!([]);
    p["titleActions"] = json!([]);
    p["releases"] = json!([]);
    let expected = json!({"ts":data::RECEIVED_TS,"files":[{"id":source_file(),"kind":"canvas","label":source::label()},{"id":source_file(),"kind":"canvas","label":source::label()}]});
    p["initialTranscriptNative"] = wait_dom(
        d,
        &format!(
            "({})({},true)",
            include_str!("../file_completion/transcript.js"),
            expected
        ),
    )
    .await?;
    Ok(())
}
fn source_file() -> &'static str {
    fixture::completion::files::CANVAS
}
pub(super) async fn ready(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
) -> Result<(), String> {
    let before = wait(state, m, |v| {
        v["edit"]["active"]["titleUsers"]["current"] == true
            && v["edit"]["active"]["titleUsers"]["status"] == "ready"
            && v["edit"]["active"]["titleUsers"]["observed"]["current"] == true
            && v["edit"]["active"]["nativeReady"] == true
    })
    .await?;
    let native=wait_dom(d,&format!("(()=>{{const e=document.getElementById('slack-list-comment-edit-draft');const v=e&&window.superPlatinumRichEditor?.resolved(e);return {{ok:!!e?.isContentEditable&&v?.tokens?.filter(t=>t.value.file_id==={}).length===2&&v.tokens.filter(t=>t.value.file_id==={}).every(t=>t.label==={}),native:v}}}})()",json!(source_file()),json!(source_file()),json!(source::label()))).await?;
    let after = snapshot(state, m);
    if before["edit"]["active"]["titleUsers"] != after["edit"]["active"]["titleUsers"] {
        return Err("Original ready title changed before input".into());
    }
    p["titleReady"]
        .as_array_mut()
        .unwrap()
        .push(json!({"job":after["edit"]["active"]["titleUsers"],"native":native["native"]}));
    Ok(())
}
async fn action(d: &Desktop, p: &mut Value, id: &str) -> Result<(), String> {
    let native = button(d, id).await?;
    p["titleActions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":id,"native":native}));
    Ok(())
}
async fn next(state: Signal<ShellState>, m: &Model, kind: &str) -> Result<usize, String> {
    let v = wait(state, m, |v| {
        v["titleSource"]["pending"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["kind"] == kind && a[0]["lane"] == "known")
    })
    .await?;
    v["titleSource"]["pending"][0]["serial"]
        .as_u64()
        .map(|n| n as usize)
        .ok_or("Original held title serial missing".into())
}
fn release(
    state: Signal<ShellState>,
    p: &mut Value,
    serial: usize,
    kind: &str,
    success: bool,
) -> Result<(), String> {
    let result = if success {
        Ok((
            if kind == "discovery" {
                source::discovery_raw(&state.peek()).map_err(|e| e.to_string())?
            } else {
                source::success()
            },
            None,
        ))
    } else {
        Err(Error::Offline(
            "Synthetic List title member unavailable".into(),
        ))
    };
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(source::release(serial, result)?);
    Ok(())
}
async fn pending(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    name: &str,
    p: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stable_drafts(state, m).await?;
    let expected = snapshot(state, m)["edit"]["active"]["titleUsers"].clone();
    if expected["current"] != true {
        return Err("Original pending title job is not current".into());
    }
    let pending_probe = format!(
        "({})(\"pending\",{})",
        include_str!("title_users/native.js"),
        expected
    );
    let geometry = wait_dom(d, &pending_probe).await?;
    let before = snapshot(state, m);
    if before["edit"]["active"]["titleUsers"] != expected {
        return Err(format!(
            "Pending title changed before settled paint: {}",
            json!({"expected":expected,"actual":before,"geometry":geometry})
        ));
    }
    let journal = compose::journal_snapshot(state).await?;
    let inventory = data::restart::snapshot(state).await?;
    if geometry["reply"]["blocks"] != p["replyBlocks"] || journal["blocks"] != p["replyBlocks"] {
        return Err("Title lookup changed independent reply".into());
    }
    let png = harness::capture(d, name).await?;
    let after = checked(d, &pending_probe).await?;
    let after_model = snapshot(state, m);
    let after_inventory = data::restart::snapshot(state).await?;
    if before != after_model || geometry != after || inventory != after_inventory {
        return Err(format!(
            "Pending title custody changed across capture: {}",
            json!({"stage":name,
            "beforeModel":before,"afterModel":after_model,"beforeGeometry":geometry,"afterGeometry":after,
            "beforeInventory":inventory,"afterInventory":after_inventory})
        ));
    }
    rows.push(json!({"stage":name,"geometry":geometry,"journal":journal,"editInventory":inventory,"model":before,"proof":p,"capture":png}));
    Ok(())
}
pub(super) async fn durable(state: Signal<ShellState>, blocks: &Value) -> Result<Value, String> {
    for _ in 0..300 {
        let v = data::restart::snapshot(state).await?;
        if v["blocks"] == *blocks && !v["seed"].is_null() {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Original native edit was not saved to SQLite".into())
}
async fn save(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    data::mode(&state.peek(), data::Mode::HeldSuccess)?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comment-edit-save')?.disabled===false})",
    )
    .await?;
    action(d, p, "slack-list-comment-edit-save").await?;
    wait_fixture(state, |v| v["held"] == true).await?;
    p["doubleSave"] = command(d, "double-save", Value::Null).await?;
    capture(state, m, d, "saving", p, rows).await?;
    p["release"] = data::release(&state.peek())?;
    editor(d, false).await?;
    wait(state, m, |v| v["edit"]["active"].is_null()).await?;
    capture(state, m, d, "saved", p, rows).await?;
    data::publish(&state.peek())?;
    action(d, p, "slack-list-comments-refresh").await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
    )
    .await?;
    wait(state, m, |v| v["comments"]["comments"]["loading"] == false).await?;
    capture(state, m, d, "readback", p, rows).await
}
pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let expected: &[&str] = if source::case() == "own-edit-empty-recovery"
        && fixture::completion::empty_canvas::own_edit_named()
    {
        empty_canvas::run(state, m, d, p, rows).await?;
        &[
            "draft",
            "editing",
            "visit-retired",
            "restored-failed",
            "retry-held",
            "cancelled",
            "late",
            "fresh-held",
            "fresh-edit",
            "final-cancel",
        ]
    } else if source::case() == "own-edit-title-recovery" {
        recovery::run(state, m, d, p, rows).await?;
        &[
            "draft",
            "discovery",
            "member-held",
            "failure",
            "retry-discovery",
            "retry-member",
            "editing",
            "saving",
            "saved",
            "readback",
        ]
    } else {
        held::run(state, m, d, p, rows).await?;
        &[
            "draft",
            "editing",
            "visit-retired",
            "restored-discovery",
            "restored-member",
            "cancelled",
            "late",
            "fresh-discovery",
            "fresh-member",
            "fresh-edit",
            "final-cancel",
        ]
    };
    if rows
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != expected
    {
        return Err("Own-edit title stage order changed".into());
    }
    Ok(())
}
