//! Explicit synthetic visit replacement; ordinary editing navigation stays blocked.
use super::*;
pub(super) async fn prepare(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    editor(d, true).await?;
    ready(state, m, d, p).await?;
    let typed = command(
        d,
        "append",
        json!({"text":" Durable title edit B \u{65e5}\u{1f98a}"}),
    )
    .await?;
    p["editedBlocks"] = typed["blocks"].clone();
    p["durableBeforeRetirement"] = durable(state, &p["editedBlocks"]).await?;
    p["editRemembered"] = command(d, "remember-edit", Value::Null).await?;
    capture(state, m, d, "editing", p, rows).await?;
    let (replacement, binding) = data::title_users::replace_visit(&mut state.write())?;
    p["visitReplacement"] = replacement;
    let geometry = wait_dom(d, &probe("detached")).await?;
    for _ in 0..300 {
        if editing::native(&state.peek(), &binding).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if editing::native(&state.peek(), &binding).is_some() {
        return Err("Actual detached edit capture did not finish".into());
    }
    p["detachedFinished"] = json!({"current":binding.current(&state.peek()),"visible":binding.visible_mount(&state.peek()),"retained":binding.retained_mount(&state.peek()),"geometry":geometry});
    let inventory = durable(state, &p["editedBlocks"]).await?;
    if inventory != p["durableBeforeRetirement"] {
        return Err("Detached capture changed saved edit bytes".into());
    }
    let before = snapshot(state, m);
    let journal = compose::journal_snapshot(state).await?;
    let png = harness::capture(d, "visit-retired").await?;
    if before != snapshot(state, m) {
        return Err("Retired owner changed during capture".into());
    }
    rows.push(json!({"stage":"visit-retired","geometry":geometry,"journal":journal,"editInventory":inventory,"model":before,"proof":p,"capture":png}));
    action(d, p, "slack-list-record-comments").await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-comment-write')&&!document.getElementById('slack-list-comments-refresh')?.disabled})").await?;
    write(d).await?;
    p["replyRemounted"] = command(d, "remember", Value::Null).await?;
    if p["replyRemounted"]["blocks"] != p["replyBlocks"] {
        return Err("Replacement lost independent reply".into());
    }
    // Finish the independent transcript source before invalidating the one required peer.
    wait(state, m, |v| {
        v["transcriptTitles"]["rows"].as_array().is_some_and(|a| {
            a.iter().any(|r| {
                r["id"] == source_file()
                    && r["current"] == true
                    && r["titleUsers"]["current"] == true
            })
        })
    })
    .await?;
    p["returnTranscript"] = snapshot(state, m)["transcriptTitles"].clone();
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &Desktop,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    prepare(state, m, d, p, rows).await?;
    source::invalidate(&mut state.write()).map_err(|e| e.to_string())?;
    p["invalidated"] = source::snapshot(&state.peek());
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    let n = next(state, m, "discovery").await?;
    pending(state, m, d, "restored-discovery", p, rows).await?;
    release(state, p, n, "discovery", true)?;
    let held = next(state, m, "member").await?;
    if snapshot(state, m)["edit"]["active"]["blocks"] != p["editedBlocks"] {
        return Err("New original edit did not load durable modified blocks".into());
    }
    p["heldOriginal"] = snapshot(state, m)["edit"]["active"].clone();
    pending(state, m, d, "restored-member", p, rows).await?;
    action(d, p, "slack-list-comment-edit-cancel").await?;
    wait(state, m, |v| {
        v["edit"]["active"].is_null() && v["titleSource"]["knownDriver"]["active"] == 0
    })
    .await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-comment-edit')&&document.getElementById('slack-list-comment-draft')?.isContentEditable})").await?;
    if !data::restart::snapshot(state).await?["seed"].is_null() {
        return Err("Cancel did not clear original durable edit".into());
    }
    capture(state, m, d, "cancelled", p, rows).await?;
    release(state, p, held, "member", true)?;
    wait(state, m, |v| {
        v["titleSource"]["knownDriver"]["active"] == 0 && v["titleSource"]["pending"] == json!([])
    })
    .await?;
    if snapshot(state, m)["titleSource"]["sourceReady"] != false
        || !snapshot(state, m)["edit"]["active"].is_null()
    {
        return Err("Cancelled title work adopted late member".into());
    }
    capture(state, m, d, "late", p, rows).await?;
    command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
    let n = next(state, m, "discovery").await?;
    pending(state, m, d, "fresh-discovery", p, rows).await?;
    release(state, p, n, "discovery", true)?;
    let n = next(state, m, "member").await?;
    pending(state, m, d, "fresh-member", p, rows).await?;
    release(state, p, n, "member", true)?;
    editor(d, true).await?;
    ready(state, m, d, p).await?;
    p["freshSeed"] = snapshot(state, m)["edit"]["active"].clone();
    if p["freshSeed"]["blocks"] == p["editedBlocks"] {
        return Err("Intentional Cancel resurrected modified edit".into());
    }
    durable(state, &p["freshSeed"]["blocks"]).await?;
    capture(state, m, d, "fresh-edit", p, rows).await?;
    action(d, p, "slack-list-comment-edit-cancel").await?;
    editor(d, false).await?;
    wait(state, m, |v| v["edit"]["active"].is_null()).await?;
    capture(state, m, d, "final-cancel", p, rows).await
}
