//! Disconnected original-root acquisition, with actual rich draft and SQLite custody.
use super::*;
use data::cold as source;
const PREFIX: &str = "canvas-cold-file-roots";

fn script(command: &str, stage: &str) -> String {
    include_str!("cold.js")
        .replace("__COMMAND__", &json!(command).to_string())
        .replace("__STAGE__", &json!(stage).to_string())
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
    let acquisition = source::snapshot();
    let reply = reply::fixture::attribution_snapshot(state).await?;
    let receipt = json!({"ok":true,"stage":name,"ordinal":stages.len(),
        "noSession":true,"noTransport":true,"layout":layout,"proof":proof,
        "acquisition":acquisition,"reply":reply});
    capture::emit(PREFIX, name, receipt.clone(), stages.len()).await?;
    if data::proof(&state.peek()) != proof
        || source::snapshot() != acquisition
        || eval(desktop, &script("state", name)).await? != layout
        || reply::fixture::attribution_snapshot(state).await? != reply
    {
        return Err(format!(
            "{name}: original metadata, native editor or private draft changed during capture"
        ));
    }
    stages.push(receipt);
    Ok(())
}

async fn phase(desktop: &dioxus::desktop::DesktopContext, name: &str) -> Result<(), String> {
    wait(
        desktop,
        &format!("document.querySelector('[data-canvas-file-acquisition=\"{}\"]')?.getAttribute('data-phase')==={}", data::ROOT, json!(name)),
    ).await
}

async fn requests(count: usize) -> Result<(), String> {
    for _ in 0..120 {
        if source::snapshot()["requests"].as_array().map(Vec::len) == Some(count) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Cold file request {count} missing: {}",
        source::snapshot()
    ))
}

fn configure(state: Signal<ShellState>, mode: source::Mode) -> Result<(), String> {
    source::configure(&state.peek(), data::HOSTED, mode)
        .map_err(|_| "Cannot configure closed metadata source".into())
}

async fn retry(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(
        desktop,
        &format!("[data-canvas-file-retry=\"{}\"]", data::ROOT),
    )
    .await
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if source::target_id() != data::HOSTED {
        return Err("Cold checker requires its named hosted original".into());
    }
    settled(state, desktop, 1, 5).await?;
    phase(desktop, "unavailable").await?;
    requests(1).await?;
    let original_canvas = state
        .peek()
        .canvas
        .clone()
        .ok_or("Original Canvas missing")?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 2, 2).await?;
    phase(desktop, "unavailable").await?;
    requests(2).await?;
    custody::saved(state, desktop).await?;
    let mut stages = Vec::new();
    stage(state, desktop, "offline", 2, &mut stages).await?;
    custody::action(desktop, "append").await?;
    custody::saved(state, desktop).await?;
    custody::action(desktop, "remember").await?;
    stage(state, desktop, "draft", 2, &mut stages).await?;

    configure(state, source::Mode::Held)?;
    retry(desktop).await?;
    phase(desktop, "loading").await?;
    requests(3).await?;
    stage(state, desktop, "loading", 2, &mut stages).await?;
    crate::file_events::invalidate(&mut state.write(), "T1", data::HOSTED);
    // Cancellation drops the exact carrier. Releasing the saved sender must
    // fail; it cannot rebind its payload to a fresh read.
    for _ in 0..120 {
        if source::snapshot()["held"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r["receiverClosed"] == true))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let held = source::snapshot()["held"]
        .as_array()
        .and_then(|rows| rows.last())
        .cloned()
        .ok_or("Original held read missing")?;
    if held["receiverClosed"] != true {
        return Err("Retired metadata carrier did not cancel".into());
    }
    let release = source::release(
        &state.peek(),
        held["serial"].as_u64().ok_or("Held serial missing")? as usize,
    )
    .map_err(|_| "Cannot release original held read")?;
    if release["delivered"] != false {
        return Err("Retired metadata carrier accepted a late reply".into());
    }
    phase(desktop, "retired").await?;
    stage(state, desktop, "retired", 2, &mut stages).await?;

    configure(state, source::Mode::Malformed)?;
    click(desktop, "#canvas-comments-refresh").await?;
    settled(state, desktop, 3, 2).await?;
    phase(desktop, "unavailable").await?;
    requests(4).await?;
    custody::saved(state, desktop).await?;
    stage(state, desktop, "malformed", 3, &mut stages).await?;
    configure(state, source::Mode::RateLimited)?;
    retry(desktop).await?;
    requests(5).await?;
    wait(
        desktop,
        "[...document.querySelectorAll('[data-canvas-file-retry]')].some(e=>e.disabled)",
    )
    .await?;
    stage(state, desktop, "limited", 3, &mut stages).await?;
    click(desktop, ".canvas-dialog button[aria-label='Close canvas']").await?;
    wait(desktop, "!document.querySelector('.canvas-dialog')").await?;
    crate::canvas::open_target(state, original_canvas.clone());
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
    custody::saved(state, desktop).await?;
    wait(
        desktop,
        "[...document.querySelectorAll('[data-canvas-file-retry]')].some(e=>e.disabled)",
    )
    .await?;
    requests(5).await?;
    stage(state, desktop, "limit-reopened", 5, &mut stages).await?;
    // Real deadline expiry: no clock override and no automatic network retry.
    for _ in 0..240 {
        if eval_raw(
            desktop,
            "({ready:!!document.querySelector('[data-canvas-file-retry]:not(:disabled)')})",
        )
        .await?["ready"]
            == true
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let elapsed = source::rate_elapsed_ms().ok_or("Original rate-limit receipt missing")?;
    if elapsed < 15_000 {
        return Err("Original Retry-After was shortened across reopen".into());
    }
    wait(
        desktop,
        "!!document.querySelector('[data-canvas-file-retry]:not(:disabled)')",
    )
    .await?;
    requests(5).await?;
    stage(state, desktop, "limit-expired", 5, &mut stages).await?;
    configure(state, source::Mode::Success)?;
    retry(desktop).await?;
    requests(6).await?;
    wait(
        desktop,
        "!!document.querySelector('[data-canvas-file-id=\"FCANON001\"]')",
    )
    .await?;
    custody::saved(state, desktop).await?;
    stage(state, desktop, "recovered", 5, &mut stages).await?;
    custody::action(desktop, "remember").await?;
    custody::action(desktop, "click").await?;
    wait(desktop,"!!document.querySelector('.file-viewer-dialog .viewer img')&&document.querySelector('.file-viewer-dialog .viewer img').complete&&document.querySelector('.file-viewer-dialog .viewer img').naturalWidth>0").await?;
    stage(state, desktop, "viewer", 5, &mut stages).await?;
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
    settled(state, desktop, 6, 5).await?;
    wait(
        desktop,
        "!!document.querySelector('[data-canvas-file-id=\"FCANON001\"]')",
    )
    .await?;
    requests(7).await?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 7, 2).await?;
    custody::saved(state, desktop).await?;
    stage(state, desktop, "reopened", 7, &mut stages).await?;
    Ok(json!({"ok":true,"noSession":true,"noTransport":true,
        "draftPreserved":true,"freshTarget":true,"staleRefused":true,
        "requestCount":7,"acquisitionCount":7,"resolverCount":1,
        "rateRetained":true,"explicitRetry":true,"rateElapsedMs":elapsed,
        "layout":stages.last().unwrap()["layout"],"proof":data::proof(&state.peek()),
        "acquisition":source::snapshot(),"stages":stages}))
}
