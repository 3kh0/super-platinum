//! Separate pre-admission Close workflow. The six outcome cases are unchanged.
use super::*;
use std::sync::Arc;
const STAGES: [&str; 4] = ["note-ready", "read-failed", "closed", "reopened"];

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
fn no_activity(cap: &fixture::Capability, initial: &Value) -> Result<Value, String> {
    let ledger = evidence(cap)?;
    if !unused(&ledger) || &ledger != initial {
        return Err("Editing Close reached preparation/admission/dispatch".into());
    }
    Ok(ledger)
}
pub(super) async fn run(state: Signal<ShellState>) -> Result<Value, String> {
    let cap = state
        .peek()
        .file_share_fixture
        .clone()
        .ok_or("Missing startup capability")?;
    isolated(state, &cap)?;
    if cap.case() != fixture::Case::Accepted {
        return Err("Editing Close requires accepted capability without submission".into());
    }
    let mut identity = identity()?;
    if identity["case"] != "accepted" {
        return Err("Editing Close case identity mismatch".into());
    }
    identity["workflow"] = json!("editing-close");
    let initial_ledger = evidence(&cap)?;
    no_activity(&cap, &initial_ledger)?;
    let before = durable_draft::read(&cap).await?;
    if before.present || !before.drafts.is_empty() {
        return Err("Close database existed before Open".into());
    }
    let expected = expected_note();
    let mut eval = document::eval(&format!(
        "{};\n{};\n{}",
        include_str!("../../view/file_share/check.js"),
        include_str!("close_reader.js"),
        include_str!("close.js")
    ));
    eval.send(json!({"identity":identity,"note":expected,"destination":fixture::DESTINATION}))
        .map_err(|_| "Close check init failed")?;
    let mut observed = Vec::new();
    let mut initial_owner = String::new();
    let mut initial_destination: Option<Destination> = None;
    let mut initial_visit: Option<Arc<drafts::Visit>> = None;
    let mut claim = None;
    let mut committed = None;
    loop {
        let value: Value = tokio::time::timeout(Duration::from_secs(20), eval.recv())
            .await
            .map_err(|_| "Close UI response timeout")?
            .map_err(|_| "Close UI bridge closed")?;
        isolated(state, &cap)?;
        if value["ok"] == false {
            return Err(format!("Close UI assertion: {value}"));
        }
        let ledger = no_activity(&cap, &initial_ledger)?;
        let snapshot = durable_draft::read(&cap).await?;
        isolated(state, &cap)?;
        if let Some(stage) = value["stage"].as_str() {
            let ordinal = observed.len();
            if STAGES.get(ordinal) != Some(&stage) || value["layout"]["ok"] != true {
                return Err(format!("Invalid Close capture stage: {value}"));
            }
            if stage != "closed" && value["note"] != expected {
                return Err("Close UI note differs from independent expected blocks".into());
            }
            let row = durable_draft::one(&snapshot)?;
            let custody_evidence = {
                let shell = state.peek();
                let held = shell
                    .file_share
                    .custody
                    .lock()
                    .map_err(|_| "Draft custody poisoned")?;
                if !held.pending.is_empty() || !held.drafts.is_empty() {
                    return Err("Durable Close created admission or legacy process draft".into());
                }
                if ordinal < 2 {
                    let dialog = visible(&shell).ok_or("Editable Close dialog missing")?;
                    if dialog.phase != Phase::Editing
                        || dialog.closing
                        || dialog.receipt.is_some()
                        || value["retained"]["ok"] != true
                        || dialog.durable.as_ref() != Some(row)
                        || row.revision != 1
                        || !row.content.empty()
                    {
                        return Err("Unsubmitted typing changed durable claim or editor".into());
                    }
                    if ordinal == 0 {
                        initial_owner = dialog.id.clone();
                        initial_destination = dialog.destination.clone();
                        initial_visit = dialog.draft_visit.clone();
                        claim = Some(row.clone());
                        if initial_destination.as_ref().map(|d| d.id.as_str())
                            != Some(fixture::DESTINATION)
                            || initial_visit.as_ref().map(|v| &v.key)
                                != Some(&(cap.key(), fixture::FILE.into()))
                            || dialog.error.is_some()
                        {
                            return Err("Initial draft owner/destination mismatch".into());
                        }
                    } else if dialog.id != initial_owner
                        || dialog.destination != initial_destination
                        || claim.as_ref() != Some(row)
                        || dialog.error != Some(Refusal::Note.message())
                        || value["error"] != Refusal::Note.message()
                        || value["failure"] != json!({"calls":1,"unexpected":0,"restored":true})
                        || !dialog
                            .draft_visit
                            .as_ref()
                            .zip(initial_visit.as_ref())
                            .is_some_and(|(now, old)| Arc::ptr_eq(now, old))
                    {
                        return Err(
                            "Failed Close changed exact claim/visit or missed actual read failure"
                                .into(),
                        );
                    }
                    if value["owner"] != dialog.id {
                        return Err("JS and parent editor owners differ".into());
                    }
                } else if stage == "closed" {
                    let before = claim.as_ref().ok_or("Initial durable claim absent")?;
                    if !durable_draft::saved_from(before, row, &expected)
                        || visible(&shell).is_some()
                        || shell.overlay.is_some()
                        || value["returnedFocus"] != true
                        || value["oldDetached"] != true
                        || value["owner"] != initial_owner
                    {
                        return Err("Close hid before exact saved note commit or lost focus".into());
                    }
                    committed = Some(row.clone());
                } else {
                    let saved = committed.as_ref().ok_or("Close committed row absent")?;
                    let dialog = visible(&shell).ok_or("Reopened actual dialog absent")?;
                    let visit = dialog.draft_visit.as_ref().ok_or("Reopened visit absent")?;
                    let old_visit = initial_visit.as_ref().ok_or("Original visit absent")?;
                    if !durable_draft::reclaimed(saved, row)
                        || dialog.durable.as_ref() != Some(row)
                        || dialog.id == initial_owner
                        || value["owner"] != dialog.id
                        || dialog.phase != Phase::Editing
                        || dialog.closing
                        || dialog.error.is_some()
                        || dialog.receipt.is_some()
                        || dialog.saved_draft_stale
                        || json!(dialog.note) != expected
                        || dialog.destination != initial_destination
                        || Arc::ptr_eq(visit, old_visit)
                        || visit.source != old_visit.source
                        || value["freshOwner"] != true
                    {
                        return Err(
                            "Reopen did not claim fresh token preserving exact saved body".into(),
                        );
                    }
                }
                json!({"processDrafts":0,"pending":0,"sameVisit":stage!="reopened","durableBound":true})
            };
            let receipt = json!({"ok":true,"identity":identity,"ordinal":ordinal,"stage":stage,"layout":value["layout"],"ui":value,"ledger":ledger,"custody":custody_evidence,"durableDraft":snapshot,"noSession":true,"noTransport":true,"capabilityCurrent":true});
            capture(&cap, ordinal, &receipt).await?;
            isolated(state, &cap)?;
            no_activity(&cap, &initial_ledger)?;
            observed.push(receipt);
            eval.send(json!({"ok":true}))
                .map_err(|_| "Close stage ACK failed")?;
            continue;
        }
        if value["ok"] != true
            || observed.len() != STAGES.len()
            || json!(snapshot) != observed.last().unwrap()["durableDraft"]
        {
            return Err("Incomplete Editing Close workflow or changed final saved draft".into());
        }
        let _ = eval.send(json!({"ok":true}));
        return Ok(
            json!({"ok":true,"identity":identity,"initialNote":expected,"stages":observed,"finalLedger":ledger,"finalDraft":snapshot,"noSession":true,"noTransport":true,"capabilityCurrent":true,"noBegin":true,"noDispatch":true,"ui":value}),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_ledger_refuses_reads_begin_dispatch_and_release() {
        let empty = json!({"case":"accepted","reads":[],"dispatch_count":0,"attempt":null,"fields":null,"begin_attempt":null,"awaiting_release":false,"awaiting_begin_release":false,"released":false});
        assert!(unused(&empty));
        for (field, value) in [
            ("reads", json!(["files.info"])),
            ("begin_attempt", json!("attempt")),
            ("dispatch_count", json!(1)),
            ("attempt", json!("attempt")),
            ("fields", json!([])),
            ("awaiting_release", json!(true)),
            ("awaiting_begin_release", json!(true)),
            ("released", json!(true)),
            ("case", json!("processing")),
        ] {
            let mut changed = empty.clone();
            changed[field] = value;
            assert!(!unused(&changed), "accepted unexpected {field}");
        }
    }
}
