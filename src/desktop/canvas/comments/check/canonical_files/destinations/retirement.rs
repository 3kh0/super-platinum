//! Actual Close while a destination reader is held; late delivery cannot reopen it.
use super::*;
const PREFIX: &str = "canvas-canonical-native-retirement";

fn pending_script(command: &str) -> String {
    include_str!("retirement/pending.js")
        .replace("__COMMAND__", &json!(command).to_string())
        .replace("__ID__", &json!(data::destination_id()).to_string())
}
fn route(shell: &ShellState) -> Value {
    let canvas_read = crate::canvas::checklist::presentation(shell).map(|v| {
        json!({"visit":v.visit,"read":v.read,"loading":v.loading,"stale":v.stale,
            "error":v.error,"file":v.snapshot.as_ref().map(|s|s.file_id())})
    });
    let list = crate::slack_lists::presentation(shell).map(
        |v| json!({"id":v.list_id,"generation":v.generation,"loading":v.loading,"stale":v.stale}),
    );
    let overlay = match shell.overlay {
        Some(crate::state::Overlay::Canvas) => Some("canvas"),
        Some(crate::state::Overlay::SlackList) => Some("list"),
        None => None,
        _ => Some("other"),
    };
    json!({"overlay":overlay,"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "team":shell.core.active_team,"user":shell.self_account.user_id,
        "shortcutGeneration":shell.shortcut_generation,
        "canvas":shell.canvas.as_ref().map(|c|json!({"id":c.id,"title":c.title})),
        "canvasRead":canvas_read,"list":list})
}
fn selected_ledger() -> Value {
    let all = target_ledger();
    all[if data::destination_id() == Some(data::CANVAS) {
        "canvas"
    } else {
        "list"
    }]
    .clone()
}
fn count(value: &Value) -> usize {
    value.as_array().map_or(0, Vec::len)
}
fn verify_counts(
    proof: &Value,
    target: &Value,
    reads: usize,
    resolves: usize,
    batches: usize,
) -> Result<(), String> {
    let canvas = data::destination_id() == Some(data::CANVAS);
    let (selected, unused) = if canvas {
        (&target["canvas"], &target["list"])
    } else {
        (&target["list"], &target["canvas"])
    };
    if count(&proof["requests"]) != reads
        || count(&proof["resolver"]["requests"]) != resolves
        || count(&proof["resolver"]["settlements"]) != resolves
        || count(&selected[if canvas { "pairs" } else { "requests" }]) != batches
        || count(&selected["requests"]) != if canvas { batches * 5 } else { batches }
        || count(&unused["requests"]) != 0
    {
        return Err(format!(
            "Unexpected original read counts: {}",
            json!({"reads":reads,"resolves":resolves,"batches":batches,"proof":proof,"targetReads":target})
        ));
    }
    Ok(())
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    name: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let index = stages.len();
    let reads = [2, 2, 2, 4, 4, 4, 6][index];
    let resolves = [0, 0, 1, 1, 1, 2, 2][index];
    let batches = [0, 0, 1, 1, 1, 2, 2][index];
    let target = matches!(name, "target-pending" | "destination");
    let js = if name == "target-pending" {
        pending_script("state")
    } else if target {
        destination_script("state")
    } else {
        script("state", name)
    };
    let scroll = if name == "target-pending" {
        pending_script("scroll")
    } else if target {
        destination_script("scroll")
    } else {
        script("scroll", name)
    };
    eval(desktop, &scroll)
        .await
        .map_err(|e| format!("{name} scroll: {e}"))?;
    let layout = eval(desktop, &js)
        .await
        .map_err(|e| format!("{name} layout: {e}"))?;
    let proof = data::proof(&state.peek());
    capture::verify_snapshot(&proof, reads, !target).map_err(|e| format!("{name}: {e}"))?;
    let reply = witness.snapshot(state).await?;
    if index >= 2
        && (reply["journal"] != stages[1]["reply"]["journal"]
            || reply["runtime"]["blocks"] != stages[1]["reply"]["runtime"]["blocks"]
            || reply["runtime"]["text"] != stages[1]["reply"]["runtime"]["text"])
    {
        return Err(format!(
            "{name}: original saved rich reply changed: {}",
            json!({"original":stages[1]["reply"],"current":reply})
        ));
    }
    let target_reads = target_ledger();
    verify_counts(&proof, &target_reads, reads, resolves, batches)?;
    let route = route(&state.peek());
    let receipt = json!({"ok":true,"stage":name,"ordinal":index,"noSession":true,"noTransport":true,
        "layout":layout,"proof":proof,"reply":reply,"targetReads":target_reads,"route":route});
    capture::emit(PREFIX, name, receipt.clone(), index).await?;
    let after = json!({"layout":eval_raw(desktop,&js).await?,"proof":data::proof(&state.peek()),
        "reply":witness.snapshot(state).await?,"targetReads":target_ledger(),"route":self::route(&state.peek())});
    let changed = ["layout", "proof", "reply", "targetReads", "route"]
        .into_iter()
        .filter(|key| receipt[*key] != after[*key])
        .collect::<Vec<_>>();
    if !changed.is_empty() {
        return Err(format!(
            "{name}: capture changed: {}",
            json!({"fields":changed,"before":receipt,"after":after})
        ));
    }
    stages.push(receipt);
    Ok(())
}
async fn wait_target(
    desktop: &dioxus::desktop::DesktopContext,
    pending: bool,
) -> Result<(), String> {
    let script = if pending {
        pending_script("state")
    } else {
        destination_script("state")
    };
    let key = if pending { "pending" } else { "ready" };
    let mut last = Value::Null;
    for _ in 0..120 {
        last = eval_raw(desktop, &script).await?;
        let ledger = selected_ledger();
        if last[key] == true && (!pending || ledger["held"] == 1) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Target {key} did not settle: {}",
        json!({"layout":last,"targetReads":target_ledger()})
    ))
}
async fn wait_retired(released: bool) -> Result<(), String> {
    let canvas = data::destination_id() == Some(data::CANVAS);
    let mut last = Value::Null;
    for _ in 0..120 {
        last = selected_ledger();
        let settlements = &last["settlements"];
        let done = if canvas {
            count(settlements) == 1 && settlements[0] == json!({"serial":1,"status":"cancelled"})
        } else if released {
            count(settlements) == 1
                && settlements[0]
                    == json!({"serial":1,"delivered":true,
                "current":false,"parsed":false,"refreshAgain":false})
        } else {
            count(settlements) == 0 && last["held"] == 1
        };
        if done {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original reader retirement did not settle (released={released}): {last}"
    ))
}
async fn reopen(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    original: crate::state::CanvasTarget,
    reads: usize,
) -> Result<(), String> {
    let close = if data::destination_id() == Some(data::CANVAS) {
        ".canvas-dialog button[aria-label='Close canvas']"
    } else {
        "#slack-list-close"
    };
    click(desktop, close)
        .await
        .map_err(|e| format!("Native destination Close: {e}"))?;
    wait(
        desktop,
        "!document.querySelector('.canvas-dialog,.slack-list-dialog')",
    )
    .await?;
    let closed = route(&state.peek());
    if !closed["overlay"].is_null()
        || (data::destination_id() == Some(data::CANVAS) && !closed["canvasRead"].is_null())
        || (data::destination_id() == Some(data::LIST) && !closed["list"].is_null())
    {
        return Err(format!("Native Close retained destination: {closed}"));
    }
    crate::canvas::open_target(state, original);
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
    settled(state, desktop, reads - 1, 5).await?;
    click(
        desktop,
        &format!(
            ".canvas-comment[data-comment-ts='{}'] .canvas-comment-open",
            data::ROOT
        ),
    )
    .await?;
    settled(state, desktop, reads, 2).await?;
    saved(state, desktop).await
}
async fn original_snapshot(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
) -> Result<Value, String> {
    Ok(
        json!({"route":route(&state.peek()),"layout":eval(desktop,&script("state","late-refused")).await?,
        "proof":data::proof(&state.peek()),"reply":witness.snapshot(state).await?}),
    )
}
pub(in crate::canvas) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let id = data::destination_id().ok_or("Original destination missing")?;
    settled(state, desktop, 1, 5).await?;
    let original = state
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
    stage(state, desktop, &witness, "thread", &mut stages).await?;
    eval(desktop, &script("append", "draft")).await?;
    saved(state, desktop).await?;
    eval(desktop, &script("remember", "draft")).await?;
    stage(state, desktop, &witness, "draft", &mut stages).await?;
    let departure = json!({"native":eval(desktop,&script("state","draft")).await?,
        "reply":witness.snapshot(state).await?,"route":route(&state.peek())});
    if id == data::CANVAS {
        data::destination_canvas::hold_next(&state.peek())
    } else {
        crate::slack_lists::fixture::canvas_navigation::hold_next(&state.peek())
    }
    .map_err(|e| format!("Hold original target reader: {e}"))?;
    eval(desktop, &script("click", "target-pending")).await?;
    wait_target(desktop, true).await?;
    stage(state, desktop, &witness, "target-pending", &mut stages).await?;
    reopen(state, desktop, original.clone(), 4).await?;
    wait_retired(false).await?;
    stage(state, desktop, &witness, "source-reopened", &mut stages).await?;
    let before = original_snapshot(state, desktop, &witness).await?;
    let release = if id == data::CANVAS {
        data::destination_canvas::release(&state.peek())
    } else {
        crate::slack_lists::fixture::canvas_navigation::release(&state.peek())
    }
    .map_err(|e| format!("Release original reader: {e}"))?;
    if release != json!({"serial":1,"delivered":id==data::LIST}) {
        return Err(format!("Original late delivery differs: {release}"));
    }
    wait_retired(true).await?;
    let after = original_snapshot(state, desktop, &witness).await?;
    if before != after {
        return Err(format!(
            "Late reader overwrote current source: {}",
            json!({"before":before,"release":release,"after":after})
        ));
    }
    let late = json!({"before":before,"release":release,"after":after});
    stage(state, desktop, &witness, "late-refused", &mut stages).await?;
    eval(desktop, &script("click", "destination")).await?;
    wait_target(desktop, false).await?;
    stage(state, desktop, &witness, "destination", &mut stages).await?;
    reopen(state, desktop, original, 6).await?;
    stage(state, desktop, &witness, "reopened", &mut stages).await?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"destinationId":id,
        "draftPreserved":true,"lateRefused":true,"freshTarget":true,"nativeClose":true,
        "requestCount":6,"resolverCount":2,"targetReadCount":2,
        "departure":departure,"late":late,"layout":stages.last().unwrap()["layout"],
        "proof":data::proof(&state.peek()),"stages":stages}),
    )
}
