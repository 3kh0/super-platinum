//! Owned, feature-gated native refresh workflow. All input is synthetic.
use super::*;
use serde_json::{Value, json};
mod capability;
mod harness;
pub(super) use capability::{body, metadata};
pub(crate) use capability::{enabled, install};
const STAGES: [&str; 7] = [
    "selected",
    "loading",
    "failure",
    "retry-loading",
    "stale",
    "final-loading",
    "fresh",
];
static STARTED: AtomicBool = AtomicBool::new(false);
pub(crate) fn start(state: Signal<ShellState>) {
    if !enabled() || STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state).await {
            Ok(v) => v,
            Err(error) => {
                json!({"ok":false,"error":error,"ledger":capability::get(&state.peek()).ok().and_then(|c|c.snapshot().ok())})
            }
        };
        harness::emit("file-refresh-check", receipt)
            .await
            .expect("Refresh terminal output failed");
    });
}
async fn held(state: Signal<ShellState>, attempt: u64) -> Result<(), String> {
    for _ in 0..150 {
        if capability::get(&state.peek())?.snapshot()?["pending"] == attempt {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Actual refresh attempt {attempt} did not hold its body"
    ))
}
fn observed(state: Signal<ShellState>, stage: &str) -> Result<Value, String> {
    let shell = state.peek();
    let cap = capability::get(&shell)?;
    let viewer = shell.viewer.as_ref().ok_or("Refresh viewer disappeared")?;
    let current = lease(&shell, &viewer.id).ok_or("Refresh lease missing")?;
    let bytes = shell
        .media
        .ready_bytes(&viewer.id)
        .ok_or("Refresh bytes missing")?;
    let fresh = stage == "fresh";
    if viewer.file.as_ref().map(|f| f.id.as_str()) != Some(capability::FILE)
        || viewer.name != "Release notes"
        || viewer.download_name != "release-notes.txt"
        || (fresh == (viewer.id == cap.old))
        || bytes.as_ref()
            != if fresh {
                capability::NEW.as_bytes()
            } else {
                capability::OLD.as_bytes()
            }
        || cap.cancel.load(Ordering::Acquire) != fresh
        || current.cancellation().load(Ordering::Acquire)
        || (!fresh && !Arc::ptr_eq(&bytes, &cap.old_bytes))
        || !shell
            .media
            .ready_bytes(&cap.old)
            .is_some_and(|b| Arc::ptr_eq(&b, &cap.old_bytes))
    {
        return Err(format!("{stage}: exact asset/lease/bytes oracle failed"));
    }
    Ok(
        json!({"old":cap.old.uri(),"current":viewer.id.uri(),"fresh":fresh,
        "oldCanceled":cap.cancel.load(Ordering::Acquire),"currentCanceled":false,
        "bytes":String::from_utf8_lossy(&bytes),"oldBytesRetained":cap.old_bytes.as_ref()==capability::OLD.as_bytes()}),
    )
}
fn ledger_at(stage: &str, value: &Value) -> bool {
    let (attempts, pending, releases) = match stage {
        "selected" => (0, None, 0),
        "loading" => (1, Some(1), 0),
        "failure" => (1, None, 1),
        "retry-loading" => (2, Some(2), 1),
        "stale" => (2, None, 2),
        "final-loading" => (3, Some(3), 2),
        "fresh" => (3, None, 3),
        _ => return false,
    };
    value["metadataAttempts"] == json!((1..=attempts).collect::<Vec<u64>>())
        && value["bodyAttempts"] == value["metadataAttempts"]
        && value["pending"] == json!(pending)
        && value["released"]
            == json!(&[(1, "failure"), (2, "stale-body"), (3, "fresh-body")][..releases])
}
async fn run(mut state: Signal<ShellState>) -> Result<Value, String> {
    let h = harness::configuration()?;
    let cap = capability::get(&state.peek())?;
    let mut staged = Some(h.stage_download(cap.old_bytes.clone()).await?);
    capability::get(&state.peek())?;
    let mut eval = document::eval(include_str!("check/check.js"));
    eval.send(json!({"old":capability::OLD,"fresh":capability::NEW}))
        .map_err(|_| "Refresh UI init failed")?;
    let mut stages: Vec<Value> = Vec::new();
    let mut download = Value::Null;
    loop {
        let value: Value = tokio::time::timeout(Duration::from_secs(35), eval.recv())
            .await
            .map_err(|_| "Refresh UI timed out")?
            .map_err(|_| "Refresh UI bridge failed")?;
        capability::get(&state.peek())?;
        if value["ok"] != true {
            return Err(format!("Refresh UI assertion: {value}"));
        }
        if value["complete"] == true {
            if stages.len() != STAGES.len() || value["stages"] != 7 {
                return Err("Incomplete refresh workflow".into());
            }
            let ledger = cap.snapshot()?;
            if ledger != stages.last().ok_or("Missing final stage")?["ledger"] || download.is_null()
            {
                return Err("Final refresh evidence changed".into());
            }
            return Ok(
                json!({"ok":true,"identity":h.identity,"stages":stages,"ledger":ledger,"download":download,
                "syntheticSession":true,"disconnectedTransport":true,"structurallyDisconnected":true,"programmaticActivation":true}),
            );
        }
        let ordinal = stages.len();
        let stage = value["stage"].as_str().ok_or("Missing refresh stage")?;
        if STAGES.get(ordinal) != Some(&stage) || value["layout"]["ok"] != true {
            return Err("Refresh stage order/layout refused".into());
        }
        let pending = match stage {
            "loading" => Some(1),
            "retry-loading" => Some(2),
            "final-loading" => Some(3),
            _ => None,
        };
        if let Some(attempt) = pending {
            held(state, attempt).await?;
        }
        capability::get(&state.peek())?;
        let ledger = cap.snapshot()?;
        if !ledger_at(stage, &ledger) {
            return Err(format!("{stage}: unexpected read ledger {ledger}"));
        }
        let asset = observed(state, stage)?;
        if stage == "fresh" {
            download = h
                .canceled(
                    staged.take().ok_or("Download stage already consumed")?,
                    cap.cancel.clone(),
                )
                .await?;
            capability::get(&state.peek())?;
        }
        let receipt = json!({"ok":true,"identity":h.identity,"ordinal":ordinal,"stage":stage,"ui":value,
            "ledger":ledger,"asset":asset,"download":download,"syntheticSession":true,"disconnectedTransport":true});
        h.capture(ordinal, receipt.clone()).await?;
        capability::get(&state.peek())?;
        if cap.snapshot()? != ledger {
            return Err("Refresh changed while capture was held".into());
        }
        stages.push(receipt);
        if let Some(attempt) = pending {
            if attempt == 2 {
                // Same production invalidation seam used by a confirmed local rename.
                crate::file_events::invalidate(&mut state.write(), "T1", capability::FILE);
            }
            cap.release(attempt)?;
        }
        eval.send(json!({"ok":true}))
            .map_err(|_| "Refresh UI ACK failed")?;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stage_ledger_rejects_extra_read_or_unreleased_completion() {
        let mut v = json!({"metadataAttempts":[1,2,3],"bodyAttempts":[1,2,3],"pending":null,"released":[[1,"failure"],[2,"stale-body"],[3,"fresh-body"]]});
        assert!(ledger_at("fresh", &v));
        v["released"][1] = json!([2, "fresh-body"]);
        assert!(!ledger_at("fresh", &v));
        v["released"][1] = json!([2, "stale-body"]);
        v["pending"] = json!(3);
        assert!(!ledger_at("fresh", &v));
        v["pending"] = Value::Null;
        v["metadataAttempts"] = json!([1, 2, 3, 4]);
        assert!(!ledger_at("fresh", &v));
    }
}
