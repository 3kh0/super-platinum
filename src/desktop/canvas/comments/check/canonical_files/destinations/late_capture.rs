//! Original final capture delivered after its real native request timeout.
use super::*;
mod evidence;
const PREFIX: &str = "canvas-canonical-native-late-capture";
const ERROR: &str = "This file could not be opened. Your reply draft is retained here.";

fn action(command: &str) -> String {
    include_str!("late_capture/actions.js").replace("__COMMAND__", &json!(command).to_string())
}
fn source_script(command: &str, name: &str) -> String {
    script(
        command,
        if matches!(
            name,
            "timed-out" | "duplicate-refused" | "capture-recovered"
        ) {
            "capture-failed"
        } else {
            name
        },
    )
}
fn count(value: &Value) -> usize {
    value.as_array().map_or(0, Vec::len)
}
async fn charged(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    expected: &Value,
    resolves: usize,
) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..220 {
        let runtime = witness.late_runtime(state)?;
        let native = eval_raw(desktop, &source_script("state", "timed-out")).await?;
        let action = eval(desktop, &action("state")).await?;
        let proof = data::proof(&state.peek());
        let target = target_ledger();
        if runtime["nativeBusy"] == true
            && runtime["nativePresent"] == true
            && runtime["owner"]["originalCompletion"] == true
            && runtime["owner"]["failed"] == false
            && runtime["blocks"] == *expected
            && runtime["error"] == ERROR
            && runtime["saving"] == false
            && native["native"]["blocks"] == *expected
            && native["native"]["sameNode"] == true
            && native["native"]["sameMount"] == true
            && native["native"]["running"] == true
            && action["editor"]["contentEditable"] == "false"
            && action["editor"]["finishing"] == true
            && action["bridge"]["phase"] == "held"
            && action["bridge"]["closed"] == false
            && action["bridge"]["finalSends"] == 0
            && count(&action["bridge"]["finishRequests"]) == 1
            && action["controls"]["retry"]["disabled"] == true
            && action["controls"]["send"]["disabled"] == true
            && proof["retained"]["pendingFile"] == false
            && count(&proof["resolver"]["requests"]) == resolves
            && count(&proof["resolver"]["settlements"]) == resolves
            && count(&target["canvas"]["requests"]) == 0
            && count(&target["list"]["requests"]) == 0
        {
            return Ok(());
        }
        last = json!({"lateRuntime":runtime,"native":native,"action":action,"proof":proof,"targetReads":target});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original final timeout/charge did not settle: {last}"
    ))
}
async fn released(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    blocks: &Value,
) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..160 {
        let runtime = witness.late_runtime(state)?;
        let native = eval_raw(desktop, &source_script("state", "capture-recovered")).await?;
        let action = eval(desktop, &action("state")).await?;
        let proof = data::proof(&state.peek());
        if runtime["nativeBusy"] == false
            && runtime["nativePresent"] == true
            && runtime["owner"]["originalCompletion"] == false
            && runtime["owner"]["failed"] == false
            && runtime["saving"] == false
            && runtime["blocks"] == *blocks
            && runtime["error"] == ERROR
            && native["native"]["blocks"] == *blocks
            && native["native"]["sameNode"] == false
            && native["native"]["sameMount"] == false
            && native["native"]["running"] == true
            && action["editor"]["contentEditable"] == "true"
            && action["bridge"]["phase"] == "delivered"
            && action["bridge"]["closed"] == true
            && action["bridge"]["finalSends"] == 1
            && action["bridge"]["releaseCount"] == 1
            && count(&action["bridge"]["finishRequests"]) == 1
            && action["controls"]["retry"]["disabled"] == false
            && proof["retained"]["pendingFile"] == false
        {
            return Ok(());
        }
        last = json!({"lateRuntime":runtime,"native":native,"action":action,"proof":proof});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original final terminal/remount did not settle: {last}"
    ))
}
pub(in crate::canvas) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let id = data::destination_id().ok_or("Original late-capture destination missing")?;
    settled(state, desktop, 1, 5).await?;
    let original = state
        .peek()
        .canvas
        .clone()
        .ok_or("Original Canvas absent")?;
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
    evidence::stage(state, desktop, &witness, "thread", &mut stages).await?;
    eval(desktop, &script("append", "draft")).await?;
    saved(state, desktop).await?;
    eval(desktop, &script("remember", "draft")).await?;
    evidence::stage(state, desktop, &witness, "draft", &mut stages).await?;
    let expected = stages[1]["reply"]["runtime"]["blocks"].clone();
    let arm = eval(desktop, &action("arm")).await?;
    let start = std::time::Instant::now();
    let first_click = eval(desktop, &script("click", "timed-out")).await?;
    charged(state, desktop, &witness, &expected, 1).await?;
    let elapsed = u64::try_from(start.elapsed().as_millis()).map_err(|_| "Elapsed overflow")?;
    if elapsed < 6000 {
        return Err(format!(
            "Native timeout occurred before six seconds: {elapsed}"
        ));
    }
    evidence::stage(state, desktop, &witness, "timed-out", &mut stages).await?;
    let duplicate_click = eval(desktop, &script("click", "duplicate-refused")).await?;
    charged(state, desktop, &witness, &expected, 2).await?;
    evidence::stage(state, desktop, &witness, "duplicate-refused", &mut stages).await?;
    let before_release = eval(desktop, &action("state")).await?;
    if stages[2]["bridge"] != stages[3]["bridge"]
        || stages[2]["lateRuntime"]["owner"] != stages[3]["lateRuntime"]["owner"]
        || before_release["bridge"] != stages[3]["bridge"]
    {
        return Err(format!(
            "Competing activation changed original capture: {}",
            json!({
            "timeout":stages[2],"duplicate":stages[3],"beforeRelease":before_release})
        ));
    }
    let release = eval(desktop, &action("release")).await?;
    released(state, desktop, &witness, &expected).await?;
    let after_release = eval(desktop, &action("state")).await?;
    evidence::stage(state, desktop, &witness, "capture-recovered", &mut stages).await?;
    click(desktop, "#canvas-reply-retry").await?;
    saved(state, desktop).await?;
    evidence::stage(state, desktop, &witness, "retry-recovered", &mut stages).await?;
    let departure = json!({"native":eval(desktop,&script("state","retry-recovered")).await?,
        "reply":witness.snapshot(state).await?});
    eval(desktop, &script("click", "destination")).await?;
    wait(
        desktop,
        &format!("({}).ready===true", destination_script("state")),
    )
    .await?;
    evidence::stage(state, desktop, &witness, "destination", &mut stages).await?;
    click(
        desktop,
        if id == data::CANVAS {
            ".canvas-dialog button[aria-label='Close canvas']"
        } else {
            "#slack-list-close"
        },
    )
    .await?;
    wait(
        desktop,
        "!document.querySelector('.canvas-dialog,.slack-list-dialog')",
    )
    .await?;
    crate::canvas::open_target(state, original);
    wait(desktop, "document.querySelector('.canvas-dialog')?.getAttribute('aria-label')==='Canvas: Project handbook'").await?;
    if eval_raw(
        desktop,
        "({open:!!document.getElementById('canvas-comments')})",
    )
    .await?["open"]
        != true
    {
        click(desktop, "#canvas-comments-open").await?;
    }
    settled(state, desktop, 3, 5).await?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, 4, 2).await?;
    saved(state, desktop).await?;
    evidence::stage(state, desktop, &witness, "reopened", &mut stages).await?;
    for stage in &stages[2..] {
        if stage["reply"]["journal"] != stages[1]["reply"]["journal"]
            || stage["reply"]["runtime"]["blocks"] != expected
        {
            return Err(format!(
                "Late capture changed durable A: {}",
                json!({"draft":stages[1],"actual":stage})
            ));
        }
    }
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"destinationId":id,
        "timedOut":true,"competingFinishRefused":true,"lateCaptureRecovered":true,
        "draftPreserved":true,"freshTarget":true,"nativeClose":true,
        "requestCount":4,"resolverCount":3,"targetReadCount":1,
        "late":{"arm":arm,"firstClick":first_click,"timeoutElapsedMs":elapsed,
            "duplicateClick":duplicate_click,"beforeRelease":before_release,
            "release":release,"afterRelease":after_release},
        "departure":departure,"layout":stages.last().unwrap()["layout"],
        "proof":data::proof(&state.peek()),"stages":stages}),
    )
}
