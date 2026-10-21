use super::*;
pub(super) fn script(command: &str, stage: &str) -> String {
    include_str!("custody.js")
        .replace("__COMMAND__", &json!(command).to_string())
        .replace("__STAGE__", &json!(stage).to_string())
}
pub(super) async fn action(
    desktop: &dioxus::desktop::DesktopContext,
    command: &str,
) -> Result<Value, String> {
    eval(desktop, &script(command, "observe")).await
}
pub(super) async fn saved(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..160 {
        let native = eval_raw(desktop, &script("state", "observe")).await?;
        let reply = reply::fixture::attribution_snapshot(state).await?;
        if native["native"]["blocks"].is_array()
            && reply["runtime"]["loading"] == false
            && reply["runtime"]["saving"] == false
            && reply["runtime"]["error"].is_null()
            && native["native"]["blocks"] == reply["runtime"]["blocks"]
            && native["native"]["blocks"] == reply["journal"]["drafts"][0]["blocks"]
        {
            return Ok(());
        }
        last = json!({"native":native,"reply":reply});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original native rich reply did not reach SQLite; last original observation: {last}"
    ))
}
async fn issued(state: Signal<ShellState>, count: usize, settled: bool) -> Result<(), String> {
    for _ in 0..120 {
        let proof = data::proof(&state.peek());
        if proof["resolver"]["requests"].as_array().map(Vec::len) == Some(count)
            && (!settled
                || proof["resolver"]["settlements"].as_array().map(Vec::len) == Some(count))
            && (!settled || proof["retained"]["pendingFile"] == false)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original resolver {count} did not reach expected phase"
    ))
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    reads: usize,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    eval(desktop, &script("scroll", name)).await?;
    let layout = eval(desktop, &script("state", name)).await?;
    let proof = data::proof(&state.peek());
    capture::verify_snapshot(&proof, reads, name != "viewer")?;
    if name == "viewer" && state.peek().overlay != Some(crate::state::Overlay::Viewer) {
        return Err("Original target did not replace Canvas with native viewer".into());
    }
    let reply = reply::fixture::attribution_snapshot(state).await?;
    let receipt = json!({"ok":true,"stage":name,"ordinal":stages.len(),"noSession":true,"noTransport":true,"layout":layout,"proof":proof,"reply":reply});
    capture::emit(
        "canvas-canonical-file-custody",
        name,
        receipt.clone(),
        stages.len(),
    )
    .await?;
    if data::proof(&state.peek()) != proof
        || eval(desktop, &script("state", name)).await? != layout
        || reply::fixture::attribution_snapshot(state).await? != reply
    {
        return Err(format!("{name}: native/source/disk changed during capture"));
    }
    stages.push(receipt);
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    settled(state, desktop, 1, 5).await?;
    let original_canvas = state
        .peek()
        .canvas
        .clone()
        .ok_or("Original canvas missing")?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 2, 2).await?;
    saved(state, desktop).await?;
    let mut stages = Vec::new();
    stage(state, desktop, "thread", 2, &mut stages).await?;
    action(desktop, "append").await?;
    saved(state, desktop).await?;
    action(desktop, "remember").await?;
    stage(state, desktop, "draft", 2, &mut stages).await?;
    action(desktop, "composition-start").await?;
    wait(desktop,"window.superPlatinumRichEditor?.snapshot(document.getElementById('canvas-comment-reply'))?.composing===true").await?;
    action(desktop, "click").await?;
    issued(state, 1, true).await?;
    wait(desktop,"[...document.querySelectorAll('#canvas-comments [role=alert]')].some(e=>e.textContent.includes('could not be opened'))").await?;
    stage(state, desktop, "composition-refused", 2, &mut stages).await?;
    action(desktop, "composition-end").await?;
    saved(state, desktop).await?;
    action(desktop, "fail-snapshot").await?;
    wait(desktop,"!!document.getElementById('canvas-reply-retry')&&!document.getElementById('canvas-reply-retry').disabled").await?;
    stage(state, desktop, "capture-failed", 2, &mut stages).await?;
    click(desktop, "#canvas-reply-retry").await?;
    saved(state, desktop).await?;
    stage(state, desktop, "capture-recovered", 2, &mut stages).await?;
    data::resolver::configure(&state.peek(), data::resolver::ResolveMode::Held)
        .map_err(|_| "Cannot hold resolver")?;
    action(desktop, "double-click").await?;
    issued(state, 2, false).await?;
    stage(state, desktop, "held", 2, &mut stages).await?;
    data::configure(&state.peek(), data::Mode::Renamed)
        .map_err(|_| "Cannot replace original source")?;
    click(desktop, "#canvas-comments-refresh").await?;
    settled(state, desktop, 3, 2).await?;
    saved(state, desktop).await?;
    let released = data::resolver::release(&state.peek()).map_err(|_| "Held resolver missing")?;
    if released["delivered"] != true {
        return Err("Expected delivered original held result".into());
    }
    issued(state, 2, true).await?;
    wait(desktop, "!document.querySelector('.file-viewer-dialog')").await?;
    stage(state, desktop, "source-retired", 3, &mut stages).await?;
    action(desktop, "click").await?;
    wait(desktop,"!!document.querySelector('.file-viewer-dialog .viewer img')&&document.querySelector('.file-viewer-dialog .viewer img').complete&&document.querySelector('.file-viewer-dialog .viewer img').naturalWidth>0").await?;
    stage(state, desktop, "viewer", 3, &mut stages).await?;
    click(
        desktop,
        ".file-viewer-dialog header button[aria-label='Close']",
    )
    .await?;
    wait(desktop, "!document.querySelector('.file-viewer-dialog')").await?;
    crate::canvas::open_target(state, original_canvas);
    wait(desktop, "!!document.querySelector('.canvas-dialog')").await?;
    if eval_raw(
        desktop,
        "({open:!!document.getElementById('canvas-comments')})",
    )
    .await?["open"]
        != true
    {
        click(desktop, "#canvas-comments-open").await?;
    }
    settled(state, desktop, 4, 5).await?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 5, 2).await?;
    saved(state, desktop).await?;
    stage(state, desktop, "reopened", 5, &mut stages).await?;
    let final_native = eval_raw(desktop, &script("state", "observe")).await?;
    if final_native["native"]["sameBlocks"] != true || final_native["native"]["sameNode"] != false {
        return Err("Fresh reopened editor did not restore exact saved blocks".into());
    }
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"draftPreserved":true,"freshTarget":true,
        "compositionRefused":true,"captureRecovered":true,"staleRefused":true,"noDuplicateDispatch":true,
        "requestCount":5,"resolverCount":3,"layout":stages.last().unwrap()["layout"],
        "proof":data::proof(&state.peek()),"stages":stages}),
    )
}
