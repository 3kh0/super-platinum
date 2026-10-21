//! Actual native Canvas/List destinations, retaining the original reply witness.
use super::*;
pub(super) mod late_capture;
pub(super) mod retirement;
pub(super) mod storage;
const PREFIX: &str = "canvas-canonical-native-destinations";
pub(super) fn late_capture_requested() -> bool {
    data::destinations_requested()
        && std::env::var("SUPER_PLATINUM_CANVAS_DESTINATION_LATE_CAPTURE_CHECK").as_deref()
            == Ok("1")
}
pub(super) fn storage_requested() -> bool {
    data::destinations_requested()
        && std::env::var("SUPER_PLATINUM_CANVAS_DESTINATION_STORAGE_CHECK").as_deref() == Ok("1")
}
pub(super) fn retirement_requested() -> bool {
    data::destinations_requested()
        && std::env::var("SUPER_PLATINUM_CANVAS_DESTINATION_RETIREMENT_CHECK").as_deref() == Ok("1")
}
fn script(command: &str, stage: &str) -> String {
    include_str!("custody.js")
        .replace("FCANON001", data::destination_id().unwrap_or(""))
        .replace("__COMMAND__", &json!(command).to_string())
        .replace("__STAGE__", &json!(stage).to_string())
}
fn destination_script(command: &str) -> String {
    include_str!("destinations/target.js")
        .replace("__COMMAND__", &json!(command).to_string())
        .replace("__ID__", &json!(data::destination_id()).to_string())
}
fn target_ledger() -> Value {
    json!({"canvas":data::destination_canvas::snapshot(),
        "list":crate::slack_lists::fixture::canvas_navigation::snapshot()})
}
async fn saved(
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
            && reply["runtime"]["sending"] == false
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
        "Original destination reply did not reach SQLite: {last}"
    ))
}
async fn issued(state: Signal<ShellState>, count: usize, complete: bool) -> Result<(), String> {
    for _ in 0..120 {
        let proof = data::proof(&state.peek());
        if proof["resolver"]["requests"].as_array().map(Vec::len) == Some(count)
            && (!complete
                || proof["resolver"]["settlements"].as_array().map(Vec::len) == Some(count))
            && (!complete || proof["retained"]["pendingFile"] == false)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Original destination resolver did not settle".into())
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    name: &str,
    reads: usize,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let target = name == "destination";
    let js = if target {
        destination_script("state")
    } else {
        script("state", name)
    };
    eval(
        desktop,
        &if target {
            destination_script("scroll")
        } else {
            script("scroll", name)
        },
    )
    .await?;
    let layout = eval(desktop, &js).await?;
    let proof = data::proof(&state.peek());
    capture::verify_snapshot(&proof, reads, !target)?;
    let reply = witness.snapshot(state).await?;
    let target_reads = target_ledger();
    let receipt = json!({"ok":true,"stage":name,"ordinal":stages.len(),"noSession":true,
        "noTransport":true,"layout":layout,"proof":proof,"reply":reply,"targetReads":target_reads});
    capture::emit(PREFIX, name, receipt.clone(), stages.len()).await?;
    if data::proof(&state.peek()) != proof
        || eval(desktop, &js).await? != layout
        || witness.snapshot(state).await? != reply
        || target_ledger() != target_reads
    {
        return Err(format!(
            "{name}: original native/source/disk/target changed during capture"
        ));
    }
    stages.push(receipt);
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let id = data::destination_id().ok_or("Original destination case missing")?;
    settled(state, desktop, 1, 5).await?;
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
    saved(state, desktop).await?;
    let witness = reply::fixture::DestinationWitness::capture(state)?;
    let mut stages = Vec::new();
    stage(state, desktop, &witness, "thread", 2, &mut stages).await?;
    eval(desktop, &script("append", "draft")).await?;
    saved(state, desktop).await?;
    eval(desktop, &script("remember", "draft")).await?;
    stage(state, desktop, &witness, "draft", 2, &mut stages).await?;
    data::resolver::configure(&state.peek(), data::resolver::ResolveMode::Held)
        .map_err(|_| "Cannot hold original destination resolver")?;
    eval(desktop, &script("double-click", "held")).await?;
    issued(state, 1, false).await?;
    stage(state, desktop, &witness, "held", 2, &mut stages).await?;
    data::configure(&state.peek(), data::Mode::Renamed)
        .map_err(|_| "Cannot refresh original source")?;
    click(desktop, "#canvas-comments-refresh").await?;
    settled(state, desktop, 3, 2).await?;
    saved(state, desktop).await?;
    let release = data::resolver::release(&state.peek()).map_err(|_| "Held resolver missing")?;
    if release["delivered"] != true {
        return Err("Original held resolver was not delivered".into());
    }
    issued(state, 1, true).await?;
    stage(state, desktop, &witness, "source-retired", 3, &mut stages).await?;
    // Remember the actual holder immediately before departure, not the pre-refresh holder.
    eval(desktop, &script("remember", "source-retired")).await?;
    let departure = json!({"native":eval(desktop,&script("state","source-retired")).await?,
        "reply":witness.snapshot(state).await?});
    eval(desktop, &script("click", "destination")).await?;
    for _ in 0..200 {
        if eval_raw(desktop, &destination_script("state")).await?["ready"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    stage(state, desktop, &witness, "destination", 3, &mut stages).await?;
    let close = if id == data::CANVAS {
        ".canvas-dialog button[aria-label='Close canvas']"
    } else {
        "#slack-list-close"
    };
    click(desktop, close).await?;
    wait(
        desktop,
        "!document.querySelector('.canvas-dialog,.slack-list-dialog')",
    )
    .await?;
    crate::canvas::open_target(state, original_canvas);
    wait(desktop,"document.querySelector('.canvas-dialog')?.getAttribute('aria-label')==='Canvas: Project handbook'").await?;
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
    stage(state, desktop, &witness, "reopened", 5, &mut stages).await?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"destinationId":id,
        "draftPreserved":true,"freshTarget":true,"staleRefused":true,"noDuplicateDispatch":true,
        "requestCount":5,"resolverCount":2,"departure":departure,
        "layout":stages.last().unwrap()["layout"],"proof":data::proof(&state.peek()),"stages":stages}),
    )
}
