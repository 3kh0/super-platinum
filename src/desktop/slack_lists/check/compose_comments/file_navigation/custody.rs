use super::*;
async fn error(state: Signal<ShellState>, m: &Model) -> Result<(), String> {
    for _ in 0..500 {
        let v = snapshot(state, m);
        let d = &v["draftStore"]["drafts"][0];
        if d["error"].is_string()
            && d["busy"] == false
            && d["saving"] == false
            && d["loading"] == false
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("Guarded departure did not retain visible error".into())
}
async fn retry(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    click(d, "slack-list-comment-retry").await?;
    group_completion::wait(state, m, |v| {
        v["draftStore"]["drafts"][0]["error"].is_null()
            && v["draftStore"]["drafts"][0]["saving"] == false
    })
    .await?;
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    disk: &Disk,
    readiness: &Value,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    command(d, "composition", json!({"active":true})).await?;
    click_file(d, p, nav::CANVAS, None).await?;
    error(state, m).await?;
    capture(
        state,
        m,
        d,
        disk,
        "composition-refused",
        p,
        rows,
        None,
        false,
    )
    .await?;
    command(d, "composition", json!({"active":false})).await?;
    retry(state, m, d).await?;
    capture(
        state,
        m,
        d,
        disk,
        "composition-recovered",
        p,
        rows,
        None,
        false,
    )
    .await?;
    let hold = harness::storage_control(state, readiness, 1, "hold").await?;
    p["storage"] = json!({"hold":hold,"release":null});
    click_file(d, p, nav::CANVAS, Some(CANVAS_EDIT)).await?;
    error(state, m).await?;
    capture(state, m, d, disk, "save-refused", p, rows, None, true).await?;
    let release = harness::storage_control(state, readiness, 2, "release").await?;
    p["storage"]["release"] = release;
    retry(state, m, d).await?;
    capture(state, m, d, disk, "save-recovered", p, rows, None, false).await?;
    click_file(d, p, nav::CANVAS, None).await?;
    capture(
        state,
        m,
        d,
        disk,
        "retry-open",
        p,
        rows,
        Some(nav::CANVAS),
        false,
    )
    .await?;
    restored(state, m, d, p).await?;
    capture(state, m, d, disk, "retry-return", p, rows, None, false).await?;
    let hold = harness::storage_control(state, readiness, 3, "hold").await?;
    p["retirementStorage"] = json!({"hold":hold,"release":null});
    click_file(
        d,
        p,
        nav::CANVAS,
        Some("\nRetained after source retirement C"),
    )
    .await?;
    let pending =
        group_completion::wait(state, m, |v| v["draftStore"]["drafts"][0]["saving"] == true)
            .await?;
    p["retirementPending"] = pending;
    p["retirementNative"] = wait_dom(d, &expression("pending", &Value::Null)).await?;
    if p["retirementNative"]["native"]["blocks"] != p["expectedBlocks"]
        || snapshot(state, m)["draftStore"]["drafts"][0]["saving"] != true
    {
        return Err("Source retirement missed original native lease and pending save".into());
    }
    p["sourceInvalidation"] =
        data::files::invalidate(&mut state.write(), nav::CANVAS).map_err(|e| e.to_string())?;
    p["retirementStorage"]["release"] =
        harness::storage_control(state, readiness, 4, "release").await?;
    stable_drafts(state, m).await?;
    capture(state, m, d, disk, "source-refused", p, rows, None, false).await?;
    Ok(())
}
