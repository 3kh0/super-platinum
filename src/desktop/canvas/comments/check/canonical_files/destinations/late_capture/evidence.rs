use super::*;

fn counts(proof: &Value, targets: &Value, index: usize) -> Result<(), String> {
    let reads = [2, 2, 2, 2, 2, 2, 2, 4][index];
    let resolves = [0, 0, 1, 2, 2, 2, 3, 3][index];
    let batches = [0, 0, 0, 0, 0, 0, 1, 1][index];
    let canvas = data::destination_id() == Some(data::CANVAS);
    let (selected, unused) = if canvas {
        (&targets["canvas"], &targets["list"])
    } else {
        (&targets["list"], &targets["canvas"])
    };
    if count(&proof["requests"]) != reads
        || count(&proof["resolver"]["requests"]) != resolves
        || count(&proof["resolver"]["settlements"]) != resolves
        || count(&selected[if canvas { "pairs" } else { "requests" }]) != batches
        || count(&selected["requests"]) != if canvas { batches * 5 } else { batches }
        || count(&unused["requests"]) != 0
        || (index != 6 && proof["retained"]["pendingFile"] != false)
    {
        return Err(format!(
            "Late-capture stage {index} request counts differ: {}",
            json!({"proof":proof,"targetReads":targets})
        ));
    }
    Ok(())
}
fn error_stage(name: &str) -> bool {
    matches!(
        name,
        "timed-out" | "duplicate-refused" | "capture-recovered"
    )
}
async fn layout(desktop: &dioxus::desktop::DesktopContext, name: &str) -> Result<Value, String> {
    let mut value = if name == "destination" {
        eval_raw(desktop, &destination_script("state")).await?
    } else {
        eval_raw(desktop, &source_script("state", name)).await?
    };
    let action = eval(desktop, &action("state")).await?;
    value["controls"] = action["controls"].clone();
    value["editor"] = action["editor"].clone();
    Ok(value)
}
async fn snapshot(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    name: &str,
) -> Result<Value, String> {
    Ok(json!({"layout":layout(desktop,name).await?,
        "proof":data::proof(&state.peek()),"reply":witness.snapshot(state).await?,
        "lateRuntime":witness.late_runtime(state)?,
        "bridge":eval(desktop,&action("state")).await?["bridge"],
        "targetReads":target_ledger()}))
}
pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    witness: &reply::fixture::DestinationWitness,
    name: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let index = stages.len();
    let scroll = if name == "destination" {
        destination_script("scroll")
    } else if error_stage(name) {
        action("scroll-error")
    } else {
        source_script("scroll", name)
    };
    eval(desktop, &scroll).await?;
    let mut receipt = snapshot(state, desktop, witness, name).await?;
    if receipt["layout"]["ok"] != true {
        return Err(format!(
            "{name}: original source/destination layout unavailable: {receipt}"
        ));
    }
    if error_stage(name) {
        let controls = &receipt["layout"]["controls"];
        let retry = &controls["retry"];
        if retry["disabled"] != (name != "capture-recovered")
            || retry["text"] != "Retry local save"
            || retry["inView"] != true
            || retry["inScroll"] != true
            || retry["hit"] != true
            || controls["send"]["disabled"] != true
            || controls["alerts"].as_array().is_none_or(|alerts| {
                !alerts.iter().any(|a| {
                    a["text"] == ERROR
                        && a["inView"] == true
                        && a["inScroll"] == true
                        && a["hit"] == true
                })
            })
        {
            return Err(format!(
                "{name}: timeout/refusal controls not readable: {receipt}"
            ));
        }
    }
    capture::verify_snapshot(
        &receipt["proof"],
        [2, 2, 2, 2, 2, 2, 2, 4][index],
        name != "destination",
    )?;
    counts(&receipt["proof"], &receipt["targetReads"], index)?;
    receipt["ok"] = json!(true);
    receipt["stage"] = json!(name);
    receipt["ordinal"] = json!(index);
    receipt["noSession"] = json!(true);
    receipt["noTransport"] = json!(true);
    capture::emit(PREFIX, name, receipt.clone(), index).await?;
    let after = snapshot(state, desktop, witness, name).await?;
    let changed = [
        "layout",
        "proof",
        "reply",
        "lateRuntime",
        "bridge",
        "targetReads",
    ]
    .into_iter()
    .filter(|key| receipt[*key] != after[*key])
    .collect::<Vec<_>>();
    if !changed.is_empty() {
        return Err(format!(
            "{name}: capture mutated original custody: {}",
            json!({"fields":changed,"before":receipt,"after":after})
        ));
    }
    stages.push(receipt);
    Ok(())
}
