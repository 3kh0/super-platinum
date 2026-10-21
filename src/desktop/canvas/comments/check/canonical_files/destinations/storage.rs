//! Real isolated SQLite write refusal, explicit Retry, then native destination.
use super::*;
mod control;
const PREFIX: &str = "canvas-canonical-native-storage";

fn action(command: &str) -> String {
    include_str!("storage/actions.js").replace("__COMMAND__", &json!(command).to_string())
}
fn source_script(command: &str, name: &str) -> String {
    script(
        command,
        if name == "save-refused" {
            "capture-failed"
        } else {
            name
        },
    )
}
fn count(v: &Value) -> usize {
    v.as_array().map_or(0, Vec::len)
}
fn counts(proof: &Value, target: &Value, index: usize) -> Result<(), String> {
    let reads = [2, 2, 2, 2, 2, 4][index];
    let resolves = [0, 0, 1, 1, 2, 2][index];
    let batches = [0, 0, 0, 0, 1, 1][index];
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
        || (index != 4 && proof["retained"]["pendingFile"] != false)
    {
        return Err(format!(
            "Storage stage {index} original request counts differ: {}",
            json!({"proof":proof,"targetReads":target})
        ));
    }
    Ok(())
}
async fn raw_layout(
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
) -> Result<Value, String> {
    if name == "destination" {
        return eval_raw(desktop, &destination_script("state")).await;
    }
    let mut value = eval_raw(desktop, &source_script("state", name)).await?;
    value["controls"] = eval_raw(desktop, &action("state")).await?;
    Ok(value)
}
async fn layout(desktop: &dioxus::desktop::DesktopContext, name: &str) -> Result<Value, String> {
    let value = raw_layout(desktop, name).await?;
    if value["ok"] != true {
        return Err(format!("{name}: storage layout unavailable: {value}"));
    }
    if name == "save-refused" {
        let c = &value["controls"];
        if c["send"]["disabled"] != true
            || c["retry"]["disabled"] != false
            || c["retry"]["text"] != "Retry local save"
            || c["retry"]["inView"] != true
            || c["retry"]["hit"] != true
            || c["alerts"].as_array().is_none_or(|a| {
                !a.iter().any(|a| a["inView"] == true && a["hit"] == true
                    && a["text"] == "The reply is retained here but could not be saved locally. Retry local save before leaving the app.")
            })
        {
            return Err(format!(
                "Save refusal controls not visibly available: {value}"
            ));
        }
    }
    Ok(value)
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    name: &str,
    storage: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let index = stages.len();
    let target = name == "destination";
    let locked = name == "save-refused";
    let scroll = if target {
        destination_script("scroll")
    } else if locked {
        action("scroll-refused")
    } else {
        source_script("scroll", name)
    };
    eval(desktop, &scroll).await?;
    let layout = layout(desktop, name).await?;
    let proof = data::proof(&state.peek());
    capture::verify_snapshot(&proof, [2, 2, 2, 2, 2, 4][index], !target)?;
    // Deliberately never open Journal while Python owns the exclusive lock.
    let reply = if locked {
        json!({"runtime":witness.runtime(state)?,"journal":null})
    } else {
        witness.snapshot(state).await?
    };
    let target_reads = target_ledger();
    counts(&proof, &target_reads, index)?;
    let receipt = json!({"ok":true,"stage":name,"ordinal":index,"noSession":true,
        "noTransport":true,"layout":layout,"proof":proof,"reply":reply,
        "targetReads":target_reads,"storage":storage});
    capture::emit(PREFIX, name, receipt.clone(), index).await?;
    let after = json!({"layout":raw_layout(desktop,name).await?,
        "proof":data::proof(&state.peek()),"reply":if locked {
            json!({"runtime":witness.runtime(state)?,"journal":null})
        } else {witness.snapshot(state).await?},"targetReads":target_ledger()});
    let changed = ["layout", "proof", "reply", "targetReads"]
        .into_iter()
        .filter(|key| receipt[*key] != after[*key])
        .collect::<Vec<_>>();
    if !changed.is_empty() {
        return Err(format!(
            "{name}: capture mutated: {}",
            json!({"fields":changed,"before":receipt,"after":after})
        ));
    }
    stages.push(receipt);
    Ok(())
}
async fn refused(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    blocks: &Value,
) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..400 {
        let native = eval_raw(desktop, &source_script("state", "save-refused")).await?;
        let controls = eval_raw(desktop, &action("state")).await?;
        let runtime = witness.runtime(state)?;
        let proof = data::proof(&state.peek());
        let targets = target_ledger();
        if runtime["saving"] == false
            && runtime["loading"] == false
            && runtime["sending"] == false
            && runtime["error"].is_string()
            && runtime["nativePresent"] == true
            && runtime["blocks"] == *blocks
            && native["native"]["blocks"] == *blocks
            && native["native"]["running"] == true
            && native["native"]["sameMount"] == false
            && native["native"]["sameNode"] == false
            && controls["retry"]["disabled"] == false
            && controls["send"]["disabled"] == true
            && proof["retained"]["pendingFile"] == false
            && count(&proof["resolver"]["settlements"]) == 1
            && count(&targets["canvas"]["requests"]) == 0
            && count(&targets["list"]["requests"]) == 0
        {
            return Ok(());
        }
        last = json!({"runtime":runtime,"native":native,"controls":controls,"proof":proof,"targetReads":targets});
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Actual locked save/refused departure did not settle (no Journal read): {last}"
    ))
}
pub(in crate::canvas) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let id = data::destination_id().ok_or("Original storage destination missing")?;
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
    let control = control::Control::capture(state, &witness).await?;
    let mut storage = json!({"hold":null,"release":null});
    let mut stages = Vec::new();
    stage(state, desktop, &witness, "thread", &storage, &mut stages).await?;
    eval(desktop, &script("append", "draft")).await?;
    saved(state, desktop).await?;
    eval(desktop, &script("remember", "draft")).await?;
    stage(state, desktop, &witness, "draft", &storage, &mut stages).await?;
    let hold = control.command(state, &witness, 1).await?;
    if hold["journal"] != stages[1]["reply"]["journal"] {
        return Err(format!(
            "Held database differs from durable A: {}",
            json!({"hold":hold,"draft":stages[1]["reply"]})
        ));
    }
    storage["hold"] = hold.clone();
    let locked_edit = eval(desktop, &action("append")).await?;
    if locked_edit["blocks"] == stages[1]["reply"]["runtime"]["blocks"] {
        return Err("Locked edit did not change rich content".into());
    }
    let refused_click = eval(desktop, &script("click", "save-refused")).await?;
    refused(state, desktop, &witness, &locked_edit["blocks"]).await?;
    stage(
        state,
        desktop,
        &witness,
        "save-refused",
        &storage,
        &mut stages,
    )
    .await?;
    let release = control.command(state, &witness, 2).await?;
    for key in ["journal", "revision", "captureIds", "contentSha256"] {
        if release[key] != hold[key] {
            return Err(format!(
                "Locked database changed at {key}: {}",
                json!({"hold":hold,"release":release})
            ));
        }
    }
    storage["release"] = release;
    click(desktop, "#canvas-reply-retry").await?;
    saved(state, desktop).await?;
    let recovered = witness.snapshot(state).await?;
    if recovered["runtime"]["blocks"] != locked_edit["blocks"]
        || recovered["journal"]["drafts"][0]["blocks"] != locked_edit["blocks"]
    {
        return Err(format!("Explicit Retry did not save B: {recovered}"));
    }
    stage(
        state,
        desktop,
        &witness,
        "save-recovered",
        &storage,
        &mut stages,
    )
    .await?;
    let departure = json!({"native":eval(desktop,&script("state","save-recovered")).await?,
        "reply":witness.snapshot(state).await?});
    eval(desktop, &script("click", "destination")).await?;
    wait(
        desktop,
        &format!("({}).ready===true", destination_script("state")),
    )
    .await?;
    stage(
        state,
        desktop,
        &witness,
        "destination",
        &storage,
        &mut stages,
    )
    .await?;
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
    stage(state, desktop, &witness, "reopened", &storage, &mut stages).await?;
    if stages[5]["reply"]["journal"] != stages[3]["reply"]["journal"]
        || stages[5]["reply"]["runtime"]["blocks"] != locked_edit["blocks"]
    {
        return Err("Reopened reply B differs from recovered inventory".into());
    }
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"destinationId":id,
        "localSaveRefused":true,"localSaveRecovered":true,"draftPreserved":true,
        "freshTarget":true,"nativeClose":true,"requestCount":4,"resolverCount":2,"targetReadCount":1,
        "lockedEdit":locked_edit,"refusedClick":refused_click,"departure":departure,"storage":storage,
        "layout":stages.last().unwrap()["layout"],"proof":data::proof(&state.peek()),"stages":stages}),
    )
}
