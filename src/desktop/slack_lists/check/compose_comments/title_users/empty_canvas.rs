//! Independent plain-empty reply restoration after a real inserted block-empty title.
use super::*;
use crate::slack_lists::comments::fixture::cold_files as cold;

async fn held(state: Signal<ShellState>, m: &Model) -> Result<usize, String> {
    let v = wait(state, m, |v| {
        v["coldFiles"]["held"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["receiverClosed"] == false)
    })
    .await?;
    v["coldFiles"]["held"][0]["serial"]
        .as_u64()
        .map(|n| n as usize)
        .ok_or("Missing original file serial".into())
}
async fn pending(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    stage: &str,
    p: &mut Value,
    rows: &mut Vec<Value>,
    disk: &Disk,
    loading: bool,
) -> Result<(), String> {
    let model = wait(state, m, |v| {
        v["titleUsers"]["current"] == true
            && v["titleUsers"]["loading"] == loading
            && (loading || v["titleUsers"]["error"].is_string())
    })
    .await?;
    let text = if loading {
        json!("Loading title details...")
    } else {
        model["titleUsers"]["error"].clone()
    };
    wait_dom(d,&format!("(()=>{{const s=document.getElementById('slack-list-comment-title-status'),r=document.getElementById('slack-list-comment-title-retry');return {{ok:!document.getElementById('slack-list-comment-draft')&&s?.textContent==={}&&{}}};}})()",text,if loading{"!r"}else{"!!r&&!r.disabled"})).await?;
    pending_shot(state, m, d, stage, p, rows, disk).await
}
async fn reopen_pending(
    state: Signal<ShellState>,
    d: &Desktop,
    file: &super_platinum_core::slack::models::File,
    p: &mut Value,
) -> Result<(), String> {
    crate::slack_lists::open(state, file.clone());
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    action(d, p, "slack-list-comment-write").await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
    disk: &Disk,
) -> Result<(), String> {
    query(state, m, d, p, "query-ready", "@alex", data::Mode::Success).await?;
    source_ready(state, m).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    shot(state, m, d, "query-ready", p, rows).await?;
    choose(
        state,
        m,
        d,
        p,
        "canvas-inserted",
        data::empty_canvas::LABEL,
        data::files::CANVAS,
        true,
    )
    .await?;
    // Metadata invalidation must not remount or relabel the original native editor.
    let script = |action: &str| format!("({})({})", include_str!("empty_canvas.js"), json!(action));
    let before = checked(d, &script("arm")).await?;
    p["plainInvalidation"] =
        cold::reply::plain_and_invalidate(&mut state.write()).map_err(|e| e.to_string())?;
    let after = wait_dom(d, &script("verify")).await?;
    p["mountedCustody"] = json!({"before":before,"after":after});
    shot(state, m, d, "canvas-inserted", p, rows).await?;
    p["beforeReopen"] = snapshot(state, m);
    let file: super_platinum_core::slack::models::File =
        serde_json::from_value(snapshot(state, m)["initialDocument"].clone())
            .map_err(|_| "Original List file missing")?;
    action(d, p, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Original draft not durably saved".into());
    }
    p["runtimeForgotten"] = json!(true);
    cold::configure(&state.peek(), cold::CANVAS, cold::Mode::Offline).map_err(|e| e.to_string())?;
    reopen_pending(state, d, &file, p).await?;
    wait(state, m, |v| v["coldFiles"]["completed"] == 1).await?;
    pending(state, m, d, "draft-failed", p, rows, disk, false).await?;
    cold::configure(&state.peek(), cold::CANVAS, cold::Mode::Held).map_err(|e| e.to_string())?;
    action(d, p, "slack-list-comment-title-retry").await?;
    let old = held(state, m).await?;
    pending(state, m, d, "retry-held", p, rows, disk, true).await?;
    p["retiredDraft"] = json!({"job":snapshot(state,m)["titleUsers"],"journal":disk.read().await?,"fileSerial":old,"fileSource":snapshot(state,m)["coldFiles"]["source"]});
    action(d, p, "slack-list-comments-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&!!document.getElementById('slack-list-record-comments')})").await?;
    action(d, p, "slack-list-record-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-record-detail')&&!!document.querySelector('.slack-list-grid')})").await?;
    open(d, "R_LIST_001").await?;
    wait(state, m, |v| v["titleUsers"]["current"] != true).await?;
    pending_shot(state, m, d, "draft-replaced", p, rows, disk).await?;
    action(d, p, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    wait(state, m, |v| {
        v["coldFiles"]["held"].as_array().is_some_and(|a| {
            a.len() == 1 && a[0]["serial"] == old && a[0]["receiverClosed"] == true
        })
    })
    .await?;
    pending_shot(state, m, d, "draft-closed", p, rows, disk).await?;
    let release = cold::release(&state.peek(), old).map_err(|e| e.to_string())?;
    if release["delivered"] != false || release["cancelledBeforeReply"] != true {
        return Err("Retired reply accepted held file response".into());
    }
    p["releases"].as_array_mut().unwrap().push(release);
    wait(state, m, |v| {
        v["coldFiles"]["completed"] == 2
            && v["titleUsers"]["current"] != true
            && v["titleUsers"]["observed"].is_null()
    })
    .await?;
    if disk.read().await? != p["retiredDraft"]["journal"]
        || snapshot(state, m)["coldFiles"]["source"] != p["retiredDraft"]["fileSource"]
    {
        return Err("Late file response changed original source or durable draft".into());
    }
    pending_shot(state, m, d, "draft-late", p, rows, disk).await?;
    reopen_pending(state, d, &file, p).await?;
    let fresh = held(state, m).await?;
    let job = snapshot(state, m)["titleUsers"].clone();
    if fresh == old
        || job["current"] != true
        || job["scope"] == p["retiredDraft"]["job"]["scope"]
        || job["blocks"] != p["expectedBlocks"]
    {
        return Err("Fresh reply did not own new exact title acquisition".into());
    }
    p["returnedDraft"] = json!({"job":job,"fileSerial":fresh});
    pending(state, m, d, "return-held", p, rows, disk, true).await?;
    let release = cold::release(&state.peek(), fresh).map_err(|e| e.to_string())?;
    if release["delivered"] != true {
        return Err("Fresh reply file response was retired".into());
    }
    p["releases"].as_array_mut().unwrap().push(release);
    wait(state, m, |v| {
        v["titleUsers"]["current"] == true && v["titleUsers"]["observed"]["current"] == true
    })
    .await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-comment-draft')?.isContentEditable&&!!window.superPlatinumRichEditor?.snapshot(document.getElementById('slack-list-comment-draft'))})").await?;
    if current_blocks(d).await? != p["expectedBlocks"] {
        return Err("Fresh reply mount changed original wire".into());
    }
    shot(state, m, d, "reopened", p, rows).await
}
