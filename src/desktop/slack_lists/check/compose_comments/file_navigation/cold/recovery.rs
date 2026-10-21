use super::*;
async fn cancelled(state: Signal<ShellState>, serial: usize) -> Result<Value, String> {
    wait_cold(state, |v| {
        v["held"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|r| r["serial"] == serial && r["receiverClosed"] == true)
        })
    })
    .await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    disk: &Disk,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    wait_cold(state, |v| v["completed"] == 2).await?;
    expected(
        p,
        ("unavailable", "Original Canvas fallback", true),
        ("unavailable", "Original List fallback", true),
    );
    capture(state, m, d, disk, "cold-failed", p, rows, None, false).await?;
    for id in [nav::CANVAS, nav::TARGET_LIST] {
        data_cold::configure(&state.peek(), id, data_cold::Mode::Held)
            .map_err(|e| e.to_string())?;
        retry(d, p, id).await?;
    }
    let old = held(state, nav::CANVAS).await?;
    expected(
        p,
        ("loading", "Original Canvas fallback", false),
        ("loading", "Original List fallback", false),
    );
    capture(state, m, d, disk, "retry-held", p, rows, None, false).await?;
    p["invalidation"] = data_cold::invalidate(&mut state.write(), nav::CANVAS, RENAMED)
        .map_err(|e| e.to_string())?;
    p["cancelledAbsentRead"] = cancelled(state, old).await?;
    release(state, old, false, p).await?;
    let list = held(state, nav::TARGET_LIST).await?;
    release(state, list, true, p).await?;
    expected(
        p,
        ("unavailable", "Original Canvas fallback", true),
        ("ready", "Alex List", false),
    );
    capture(state, m, d, disk, "source-retired", p, rows, None, false).await?;
    data_cold::configure(&state.peek(), nav::CANVAS, data_cold::Mode::Success)
        .map_err(|e| e.to_string())?;
    retry(d, p, nav::CANVAS).await?;
    expected(p, ("ready", RENAMED, false), ("ready", "Alex List", false));
    capture(state, m, d, disk, "cold-recovered", p, rows, None, false).await?;
    p["visitInvalidation"] =
        data_cold::invalidate(&mut state.write(), nav::CANVAS, "Next visit Canvas")
            .map_err(|e| e.to_string())?;
    data_cold::configure(&state.peek(), nav::CANVAS, data_cold::Mode::Held)
        .map_err(|e| e.to_string())?;
    expected(
        p,
        ("unavailable", "Original Canvas fallback", true),
        ("ready", "Alex List", false),
    );
    geometry(d, p).await?;
    retry(d, p, nav::CANVAS).await?;
    let pending = held(state, nav::CANVAS).await?;
    expected(
        p,
        ("loading", "Original Canvas fallback", false),
        ("ready", "Alex List", false),
    );
    capture(state, m, d, disk, "visit-held", p, rows, None, false).await?;
    click(d, "slack-list-comments-back").await?;
    p["cancelledVisitRead"] = cancelled(state, pending).await?;
    release(state, pending, false, p).await?;
    stable_drafts(state, m).await?;
    p["coldExpected"] = Value::Null;
    let script = "(()=>{const detail=document.getElementById('slack-list-record-detail'),editor=document.getElementById('slack-list-comment-draft'),comments=document.getElementById('slack-list-record-comments');return {ok:!!detail&&detail.dataset.recordId==='R_LIST_000'&&!editor&&!!comments,record:detail?.dataset.recordId,editorAbsent:!editor,viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1};})()";
    let g = wait_dom(d, script).await?;
    let before = snapshot(state, m);
    p["journal"] = disk.read().await?;
    if p["journal"]["blocks"] != p["expectedBlocks"] {
        return Err("Visit cancellation lost saved draft".into());
    }
    let image = harness::capture(d, "visit-retired").await?;
    let after = checked(d, script).await?;
    if snapshot(state, m) != before || g != after || disk.read().await? != p["journal"] {
        return Err("Visit retirement changed during capture".into());
    }
    rows.push(json!({"stage":"visit-retired","model":before,"proof":p,"geometry":g,"geometryAfter":after,"capture":image}));
    Ok(())
}
