use super::*;
async fn query_failed(state: Signal<ShellState>, m: &Model, d: &Desktop) -> Result<(), String> {
    wait(state, m, |v| {
        v["completion"]["current"] == true
            && v["completion"]["batchCurrent"] == false
            && v["completionSource"]["titleUsers"]["pending"] == json!([])
    })
    .await?;
    wait_dom(d,"({ok:document.querySelector('.slack-list-channel-completions [role=status]')?.textContent==='Mention suggestions could not load. Edit your query to try again.'})").await?;
    Ok(())
}
pub(super) async fn query_recovery(
    state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    query(state, m, d, p, "discovery", "@alex", data::Mode::Success).await?;
    let first = next(state, m, "discovery").await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    shot(state, m, d, "discovery", p, rows).await?;
    release(state, p, first, "discovery", true)?;
    let member = next(state, m, "member").await?;
    shot(state, m, d, "member-held", p, rows).await?;
    release(state, p, member, "member", false)?;
    query_failed(state, m, d).await?;
    shot(state, m, d, "failure", p, rows).await?;
    query(
        state,
        m,
        d,
        p,
        "retry-discovery",
        "@al",
        data::Mode::Success,
    )
    .await?;
    let first = next(state, m, "discovery").await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    shot(state, m, d, "retry-discovery", p, rows).await?;
    release(state, p, first, "discovery", true)?;
    let member = next(state, m, "member").await?;
    shot(state, m, d, "retry-member", p, rows).await?;
    release(state, p, member, "member", true)?;
    source_ready(state, m).await?;
    shot(state, m, d, "query-ready", p, rows).await?;
    choose(
        state,
        m,
        d,
        p,
        "canvas-inserted",
        source::label(),
        data::files::CANVAS,
        true,
    )
    .await?;
    shot(state, m, d, "canvas-inserted", p, rows).await?;
    p["beforeReopen"] = snapshot(state, m);
    reload(state, m, d, p).await?;
    shot(state, m, d, "reopened", p, rows).await
}
pub(super) async fn draft_recovery(
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
        source::label(),
        data::files::CANVAS,
        true,
    )
    .await?;
    shot(state, m, d, "canvas-inserted", p, rows).await?;
    p["beforeReopen"] = snapshot(state, m);
    let file: super_platinum_core::slack::models::File =
        serde_json::from_value(snapshot(state, m)["initialDocument"].clone())
            .map_err(|_| "Original file missing")?;
    action(d, p, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Original draft not saved".into());
    }
    p["runtimeForgotten"] = json!(true);
    source::invalidate(&mut state.write()).map_err(|e| e.to_string())?;
    p["afterInvalidation"] = snapshot(state, m);
    crate::slack_lists::open(state, file.clone());
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    // Actual Write action; deliberately do not use helper that waits for editor before title recovery.
    action(d, p, "slack-list-comment-write").await?;
    let discovery = next(state, m, "discovery").await?;
    pending_shot(state, m, d, "draft-discovery", p, rows, disk).await?;
    release(state, p, discovery, "discovery", true)?;
    let member = next(state, m, "member").await?;
    pending_shot(state, m, d, "draft-member", p, rows, disk).await?;
    release(state, p, member, "member", false)?;
    wait(state, m, |v| {
        v["titleUsers"]["current"] == true
            && v["titleUsers"]["loading"] == false
            && v["titleUsers"]["error"].is_string()
    })
    .await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-title-retry')&&!document.getElementById('slack-list-comment-title-retry').disabled})").await?;
    pending_shot(state, m, d, "draft-failed", p, rows, disk).await?;
    action(d, p, "slack-list-comment-title-retry").await?;
    let discovery = next(state, m, "discovery").await?;
    pending_shot(state, m, d, "retry-discovery", p, rows, disk).await?;
    release(state, p, discovery, "discovery", true)?;
    let member = next(state, m, "member").await?;
    pending_shot(state, m, d, "retry-member", p, rows, disk).await?;
    // This held callback belongs to the independently owned restored-draft job,
    // not the autocomplete query which originally inserted the Canvas token.
    p["retiredDraft"] = json!({"job":snapshot(state,m)["titleUsers"],"journal":disk.read().await?,"memberSerial":member});
    action(d, p, "slack-list-comments-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comments')&&document.activeElement===document.getElementById('slack-list-record-comments')})").await?;
    action(d, p, "slack-list-record-back").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-record-detail')&&!!document.querySelector('.slack-list-grid')})").await?;
    open(d, "R_LIST_001").await?;
    wait(state, m, |v| v["titleUsers"]["current"] != true).await?;
    pending_shot(state, m, d, "draft-replaced", p, rows, disk).await?;
    action(d, p, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    pending_shot(state, m, d, "draft-closed", p, rows, disk).await?;
    release(state, p, member, "member", true)?;
    wait(state, m, |v| {
        v["completionSource"]["titleUsers"]["knownDriver"]["active"] == 0
            && v["completionSource"]["titleUsers"]["sourcePresent"] == true
            && v["completionSource"]["titleUsers"]["sourceReady"] == false
            && v["completionSource"]["titleUsers"]["sourceRaw"]
                == p["afterInvalidation"]["completionSource"]["titleUsers"]["sourceRaw"]
            && v["completionSource"]["titleUsers"]["native"]
                == p["afterInvalidation"]["completionSource"]["titleUsers"]["native"]
            && v["titleUsers"]["current"] != true
            && v["titleUsers"]["observed"].is_null()
    })
    .await?;
    if disk.read().await? != p["retiredDraft"]["journal"] {
        return Err("Late title callback changed original durable draft".into());
    }
    pending_shot(state, m, d, "draft-late", p, rows, disk).await?;
    // The new visible visit creates its own Binding and independent title job.
    // The old original response has already settled before a successor subscribes.
    crate::slack_lists::open(state, file);
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    action(d, p, "slack-list-comment-write").await?;
    let discovery = next(state, m, "discovery").await?;
    let fresh = snapshot(state, m)["titleUsers"].clone();
    if fresh["current"] != true
        || fresh["scope"] == p["retiredDraft"]["job"]["scope"]
        || fresh["blocks"] != p["expectedBlocks"]
    {
        return Err("Returned draft did not acquire a fresh exact Binding".into());
    }
    p["returnedDraft"] = fresh;
    pending_shot(state, m, d, "return-discovery", p, rows, disk).await?;
    release(state, p, discovery, "discovery", true)?;
    let member = next(state, m, "member").await?;
    pending_shot(state, m, d, "return-member", p, rows, disk).await?;
    release(state, p, member, "member", true)?;
    // Write was already activated before the held title acquisition. Wait for
    // that original compose to mount; sampling early must not request Write again.
    wait(state, m, |v| {
        v["titleUsers"]["current"] == true && v["titleUsers"]["observed"]["current"] == true
    })
    .await?;
    wait_dom(d,"({ok:!!window.superPlatinumRichEditor?.snapshot(document.getElementById('slack-list-comment-draft'))&&document.getElementById('slack-list-comment-draft')?.isContentEditable})").await?;
    shot(state, m, d, "reopened", p, rows).await
}
