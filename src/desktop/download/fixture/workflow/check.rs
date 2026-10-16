//! Native DOM/return/outcome observations; acknowledgements never inject results.
use super::*;
use serde_json::Value;
use std::time::Duration;

const STAGES: [&str; 5] = ["initial", "canceled", "preserved", "busy", "saved"];
const PRESERVED: &str =
    "The existing file was preserved. Download again and choose a new filename.";

async fn ack(run: &Run, stage: &str) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    let expected = json!({"version":1,"nonce":run.nonce,"pid":std::process::id(),"fixture":run.alias,"stage":stage});
    let path = run.root.join(format!("ack-{stage}.json"));
    for _ in 0..15000 {
        match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err("linked workflow ack".into());
                    }
                }
                if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096
                {
                    return Err("invalid workflow ack".into());
                }
                let mut bytes = Vec::new();
                tokio::fs::File::open(&path)
                    .await
                    .map_err(|_| "workflow ack open")?
                    .take(4097)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| "workflow ack read")?;
                if bytes.len() > 4096 {
                    return Err("oversize workflow ack".into());
                }
                let value: Value =
                    serde_json::from_slice(&bytes).map_err(|_| "workflow ack json")?;
                return if value == expected {
                    Ok(())
                } else {
                    Err("wrong exact workflow ack".into())
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("workflow ack metadata".into()),
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err("workflow acknowledgement expired".into())
}

fn expected(name: &str, ledger: &Ledger) -> bool {
    let (issued, returns, outcomes, active) = match name {
        "initial" => (0, 0, 0, None),
        "canceled" => (1, 1, 0, None),
        "preserved" => (2, 2, 1, None),
        "busy" => (3, 3, 1, Some((3, true))),
        "saved" => (3, 3, 2, None),
        _ => return false,
    };
    let exact_returns = [(1, "none"), (2, "existing"), (3, "new")];
    let exact_outcomes = [(2, "destination-exists"), (3, "saved")];
    !ledger.failed
        && ledger.issued == issued
        && ledger.active == active
        && ledger.returns.len() == returns
        && ledger.outcomes.len() == outcomes
        && ledger
            .returns
            .iter()
            .zip(exact_returns)
            .all(|(row, (attempt, kind))| {
                row.attempt == attempt && row.kind == kind && row.accepted
            })
        && ledger
            .outcomes
            .iter()
            .zip(exact_outcomes)
            .all(|(row, (attempt, kind))| row.attempt == attempt && row.kind == kind)
}

async fn stage(
    name: &str,
    dom: Value,
    owner: &Value,
    state: Signal<ShellState>,
) -> Result<Value, String> {
    let label = if name == "busy" { "Cancel" } else { "Download" };
    if dom["ok"] != true
        || dom["stage"] != name
        || dom["label"] != label
        || dom["width"].as_f64().is_none_or(|width| width < 96.0)
        || dom["viewport"] != json!([1280, 780])
    {
        return Err("workflow DOM stage contract".into());
    }
    // Busy DOM appears while rfd is still open; wait for its actual accepted return.
    // The server holds the first body chunk until the runner captures this stage.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(900);
    loop {
        let ledger = snapshot()?;
        if ledger.failed {
            return Err("workflow ledger rejected an actual result".into());
        }
        if expected(name, &ledger) {
            let shell = state.peek();
            if shell.core.session.is_some()
                || shell.core.transport.is_some()
                || shell.core.active_account.is_some()
            {
                return Err("workflow acquired a session/transport/account".into());
            }
            let toast = shell.toast.as_ref().map(|toast| toast.text.clone());
            let wanted = match name {
                "preserved" => Some(PRESERVED),
                "saved" => Some("Saved original-fixture.bin"),
                _ => None,
            };
            if toast.as_deref() != wanted
                || wanted
                    .is_some_and(|text| dom["toast"]["ok"] != true || dom["toast"]["text"] != text)
            {
                return Err("workflow actual toast mismatch".into());
            }
            return Ok(
                json!({"ok":true,"stage":name,"owner":owner,"dom":dom,"issued":ledger.issued,"returns":ledger.returns,"outcomes":ledger.outcomes,"noSession":true,"noTransport":true,"noAccount":true,"toast":toast}),
            );
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("workflow actual result deadline".into());
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

async fn exercise(mut state: Signal<ShellState>) -> Result<Value, String> {
    let run = install_workflow(state).await?;
    let owner = json!({"version":1,"nonce":run.nonce,"pid":std::process::id(),"fixture":run.alias});
    eprintln!("workflow-waiting: {owner}");
    ack(&run, "ready").await?;
    let mut eval = document::eval(&format!(
        "{}\n{}",
        include_str!("../toast_check.js"),
        include_str!("check.js")
    ));
    let mut stages = Vec::new();
    for name in STAGES {
        let dom: Value = tokio::time::timeout(Duration::from_secs(900), eval.recv())
            .await
            .map_err(|_| "workflow DOM deadline")?
            .map_err(|_| "workflow DOM ended")?;
        let record = stage(name, dom, &owner, state).await?;
        eprintln!("workflow-stage: {record}");
        stages.push(record);
        ack(&run, name).await?;
        if name == "preserved" {
            state.write().toast = None;
        }
        eval.send(name)
            .map_err(|_| "workflow DOM acknowledgement")?;
    }
    let terminal: Value = tokio::time::timeout(Duration::from_secs(5), eval.recv())
        .await
        .map_err(|_| "workflow terminal deadline")?
        .map_err(|_| "workflow terminal DOM")?;
    if terminal != json!({"ok":true}) {
        return Err("workflow terminal DOM assertion".into());
    }
    Ok(
        json!({"ok":true,"owner":owner,"stages":stages,"actualRfd":true,"noSession":true,"noTransport":true,"noAccount":true}),
    )
}

pub(crate) async fn run(state: Signal<ShellState>) {
    let result = match exercise(state).await {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":error}),
    };
    eprintln!("workflow-check: {result}");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stages_require_actual_ordered_returns_and_outcomes() {
        let mut ledger = Ledger::default();
        assert!(expected("initial", &ledger));
        ledger.begin();
        assert!(!expected("canceled", &ledger));
        ledger.returned(1, "none");
        assert!(expected("canceled", &ledger));
        ledger.begin();
        ledger.returned(2, "existing");
        assert!(!expected("preserved", &ledger));
        ledger.completed(2, "destination-exists");
        assert!(expected("preserved", &ledger));
        ledger.begin();
        assert!(!expected("busy", &ledger));
        ledger.returned(3, "new");
        assert!(expected("busy", &ledger));
        ledger.completed(3, "saved");
        assert!(expected("saved", &ledger));
        assert!(!expected("busy", &ledger));
        ledger.failed = true;
        assert!(!expected("saved", &ledger));
    }
}
