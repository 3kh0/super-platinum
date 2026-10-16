//! Actual editable Close pressure and explicit earlier-copy disposal; no send.
use super::*;
use crate::file_share::draft_io::{Command, Outcome, Ticket};
use super_platinum_core::file_sharing::store::drafts::Draft;
mod oracle;
const STAGES: [&str; 7] = [
    "ready",
    "full",
    "capacity",
    "preview",
    "discarded",
    "closed",
    "reopened",
];
fn note(text: &str) -> Value {
    json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text}]}]}])
}
fn earlier(index: usize) -> Value {
    note(&format!("Earlier capture {index}"))
}
fn current_note() -> Value {
    note("Earlier capture 31 Current unsaved edit")
}
fn unused(ledger: &Value) -> bool {
    ledger["case"] == "accepted"
        && ledger["reads"] == json!([])
        && ledger["dispatch_count"] == 0
        && ledger["attempt"].is_null()
        && ledger["fields"].is_null()
        && ledger["begin_attempt"].is_null()
        && ledger["awaiting_release"] == false
        && ledger["awaiting_begin_release"] == false
        && ledger["released"] == false
}
async fn quiet(state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..160 {
        let observation = {
            let shell = state.peek();
            shell
                .file_share
                .custody
                .lock()
                .map_err(|_| "Custody poisoned")?
                .draft_io
                .diagnostic()
                .map_err(|_| "Queue poisoned")?
        };
        if observation.scopes == 0 && observation.running == 0 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Pressure queue did not finish actual I/O".into())
}
pub(super) async fn run(state: Signal<ShellState>) -> Result<Value, String> {
    let cap = state
        .peek()
        .file_share_fixture
        .clone()
        .ok_or("Missing pressure capability")?;
    isolated(state, &cap)?;
    let mut identity = identity()?;
    if cap.case() != fixture::Case::Accepted || identity["case"] != "accepted" {
        return Err("Pressure requires accepted unused capability".into());
    }
    identity["workflow"] = json!("pressure");
    let initial = evidence(&cap)?;
    if !unused(&initial) {
        return Err("Pressure capability used before Open".into());
    }
    let before = durable_draft::read(&cap).await?;
    if before.present || !before.drafts.is_empty() {
        return Err("Pressure database existed before Open".into());
    }
    let mut eval = document::eval(&format!(
        "{};\n{}",
        include_str!("../../view/file_share/check.js"),
        include_str!("pressure.js")
    ));
    eval.send(json!({"identity":identity,"destination":fixture::DESTINATION}))
        .map_err(|_| "Pressure init failed")?;
    let mut oracle = oracle::Oracle::default();
    let mut stages = Vec::new();
    loop {
        let value: Value = tokio::time::timeout(Duration::from_secs(25), eval.recv())
            .await
            .map_err(|_| "Pressure UI timeout")?
            .map_err(|_| "Pressure UI bridge closed")?;
        isolated(state, &cap)?;
        if value["ok"] == false {
            return Err(format!("Pressure UI assertion: {value}"));
        }
        if let Some(index) = value["failed"].as_u64() {
            if stages.len() != 1 || index as usize != oracle.failures.len() || index >= 32 {
                return Err("Unexpected pressure Close command".into());
            }
            let ticket = loop_ticket(state, &oracle).await?;
            let outcome = tokio::time::timeout(Duration::from_secs(10), ticket.wait())
                .await
                .map_err(|_| "Pressure Close did not complete")?;
            if !matches!(outcome, Outcome::Failed(Refusal::Storage)) {
                return Err("Actual Close was not a storage failure".into());
            }
            for _ in 0..160 {
                if visible(&state.peek()).is_some_and(|d| {
                    d.phase == Phase::Editing
                        && !d.closing
                        && d.draft_task.as_ref().is_some_and(|t| t.same(&ticket))
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            quiet(state).await?;
            let snapshot = durable_draft::read(&cap).await?;
            isolated(state, &cap)?;
            let receipt = oracle.failed(&state.peek(), ticket, index as usize, &snapshot)?;
            if evidence(&cap)? != initial {
                return Err("Failed Close reached Slack preparation".into());
            }
            eval.send(json!({"ok":true,"failed":receipt}))
                .map_err(|_| "Failed Close ACK failed")?;
            continue;
        }
        if let Some(stage) = value["stage"].as_str() {
            let ordinal = stages.len();
            if STAGES.get(ordinal) != Some(&stage) || value["layout"]["ok"] != true {
                return Err(format!("Invalid pressure stage: {value}"));
            }
            quiet(state).await?;
            let snapshot = durable_draft::read(&cap).await?;
            isolated(state, &cap)?;
            let ledger = evidence(&cap)?;
            if ledger != initial {
                return Err("Pressure reached preparation or dispatch".into());
            }
            let custody = oracle.stage(&state.peek(), stage, &value, &snapshot)?;
            let receipt = json!({"ok":true,"identity":identity,"ordinal":ordinal,"stage":stage,"layout":value["layout"],"ui":value,"ledger":ledger,"custody":custody,"durableDraft":snapshot,"noSession":true,"noTransport":true,"capabilityCurrent":true});
            capture(&cap, ordinal, &receipt).await?;
            isolated(state, &cap)?;
            if evidence(&cap)? != initial {
                return Err("Pressure changed while awaiting capture".into());
            }
            stages.push(receipt);
            eval.send(json!({"ok":true}))
                .map_err(|_| "Pressure stage ACK failed")?;
            continue;
        }
        if value["ok"] == true
            && stages.len() == STAGES.len()
            && value["actualMenu"] == true
            && value["actualDestination"] == true
            && value["syntheticEditing"] == true
            && value["failedCloseCount"] == 32
            && value["sameEditorSelection"] == true
            && value["undoRedo"] == true
            && value["actualClose"] == true
            && value["actualReopen"] == true
        {
            quiet(state).await?;
            let snapshot = durable_draft::read(&cap).await?;
            isolated(state, &cap)?;
            oracle.stage(&state.peek(), "reopened", &stages[6]["ui"], &snapshot)?;
            if evidence(&cap)? != initial {
                return Err("Pressure terminal capability changed".into());
            }
            return Ok(
                json!({"ok":true,"identity":identity,"stages":stages.iter().map(|s|s["stage"].clone()).collect::<Vec<_>>(),"ui":value,"ledger":initial,"durableDraft":snapshot,"operations":oracle.records,"retained":31,"noSession":true,"noTransport":true,"capabilityCurrent":true}),
            );
        }
        return Err("Unknown pressure checker event".into());
    }
}
async fn loop_ticket(state: Signal<ShellState>, oracle: &oracle::Oracle) -> Result<Ticket, String> {
    for _ in 0..160 {
        if let Some(ticket) = visible(&state.peek())
            .and_then(|d| d.draft_task.clone())
            .filter(|t| !oracle.failures.iter().any(|old| old.same(t)))
        {
            return Ok(ticket);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Actual Close did not reserve a new capture".into())
}
