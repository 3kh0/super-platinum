//! Actual UI + common Worker through the exact disconnected capability.
use super::{fixture, *};
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
mod close;
mod durable_draft;
mod journal;
mod pressure;
pub(crate) mod restart;
pub(crate) mod seed;
mod storage_retry;
static STARTED: AtomicBool = AtomicBool::new(false);
pub(super) fn expected_note() -> Value {
    json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":" Share note Café"}]}]}])
}
fn stages(case: fixture::Case) -> [&'static str; 7] {
    [
        "menu",
        "dialog",
        "note-ready",
        if case == fixture::Case::SourceStale {
            "begun"
        } else {
            "sending"
        },
        "outcome",
        "recovery",
        "final",
    ]
}
fn identity() -> Result<Value, String> {
    Ok(
        json!({"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_|"Missing fixture")?,"case":std::env::var("SUPER_PLATINUM_FILE_SHARE_CASE").map_err(|_|"Missing case")?,"nonce":std::env::var("SUPER_PLATINUM_FILE_SHARE_NONCE").map_err(|_|"Missing nonce")?,"pid":std::process::id()}),
    )
}
fn isolated(state: Signal<ShellState>, cap: &fixture::Capability) -> Result<(), String> {
    let shell = state.peek();
    if cap.current(&shell) && shell.core.session.is_none() && shell.core.transport.is_none() {
        Ok(())
    } else {
        Err("Fixture capability/owner changed".into())
    }
}
fn evidence(cap: &fixture::Capability) -> Result<Value, String> {
    serde_json::to_value(cap.evidence().map_err(|_| "Capability ledger refused")?)
        .map_err(|_| "Ledger encoding refused".into())
}
pub(crate) fn start(state: Signal<ShellState>) {
    if restart::start_if_requested(state) {
        return;
    }
    if !crate::fixture::is_fixture()
        || std::env::var("SUPER_PLATINUM_FILE_SHARE_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("file-share"
                | "file-share-narrow"
                | "file-share-pressure"
                | "file-share-pressure-narrow")
        )
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let pressure_alias = matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("file-share-pressure" | "file-share-pressure-narrow")
        );
        let result = match std::env::var("SUPER_PLATINUM_FILE_SHARE_CHECK_MODE") {
            Ok(mode) if mode == "pressure" && pressure_alias => pressure::run(state).await,
            _ if pressure_alias => Err("Pressure fixture requires exact check mode".into()),
            Err(std::env::VarError::NotPresent) => run(state, false).await,
            Ok(mode) if mode == "storage-retry" => run(state, true).await,
            Ok(mode) if mode == "editing-close" => close::run(state).await,
            _ => Err("Unknown Share check mode".into()),
        }
        .unwrap_or_else(|error| json!({"ok":false,"error":error}));
        eprintln!("file-share-check: {result}");
    });
}
async fn run(mut state: Signal<ShellState>, storage_retry: bool) -> Result<Value, String> {
    let cap = state
        .peek()
        .file_share_fixture
        .clone()
        .ok_or("Missing startup capability")?;
    isolated(state, &cap)?;
    let mut identity = identity()?;
    if storage_retry {
        if cap.case() != fixture::Case::StorageFailure {
            return Err("Storage retry requires exact storage-failure capability".into());
        }
        identity["workflow"] = json!("storage-retry");
    }
    if identity["case"] != serde_json::to_value(cap.case()).map_err(|_| "Case encoding")? {
        return Err("Case identity changed".into());
    }
    let expected = expected_note();
    let mut expected_stages = stages(cap.case()).to_vec();
    if storage_retry {
        expected_stages.extend(storage_retry::STAGES);
    }
    let mut retry_oracle = storage_retry::Oracle::default();
    let mut draft_oracle = durable_draft::Oracle::default();
    let mut eval = document::eval(&format!(
        "{};\n{};\n{}",
        include_str!("../view/file_share/check.js"),
        include_str!("check/retry_editor.js"),
        include_str!("check/ui.js")
    ));
    eval.send(json!({"identity":identity,"note":expected,"destination":fixture::DESTINATION,"storageRetry":storage_retry}))
        .map_err(|_| "Init send failed")?;
    let mut observed = Vec::new();
    let mut initial_owner = String::new();
    let mut ready = false;
    let mut source_mutated = false;
    let mut source_removed = false;
    loop {
        let value: Value = tokio::time::timeout(Duration::from_secs(20), eval.recv())
            .await
            .map_err(|_| "Share UI response timeout")?
            .map_err(|_| "Share UI bridge closed")?;
        isolated(state, &cap)?;
        if value["ok"] == false {
            return Err(format!("Share UI assertion: {value}"));
        }
        if let Some(command) = value["command"].as_str() {
            match command {
                "ready" if observed.len() == 3 && !ready => {
                    ready = true;
                    let mut available = false;
                    for _ in 0..160 {
                        isolated(state, &cap)?;
                        let ledger = evidence(&cap)?;
                        if ledger[if cap.case() == fixture::Case::SourceStale {
                            "awaiting_begin_release"
                        } else {
                            "awaiting_release"
                        }] == true
                        {
                            available = true;
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    if !available {
                        return Err("Worker did not reach exact response gate".into());
                    }
                }
                "remove-source"
                    if observed.len() == 5
                        && !source_removed
                        && !matches!(
                            cap.case(),
                            fixture::Case::Accepted | fixture::Case::SourceStale
                        ) =>
                {
                    seed::mutate_source(&mut state.write(), true)?;
                    source_removed = true;
                }
                _ => return Err("Out-of-order fixture command".into()),
            }
            eval.send(json!({"ok":true}))
                .map_err(|_| "Command acknowledgement failed")?;
            continue;
        }
        if let Some(stage) = value["stage"].as_str() {
            let ordinal = observed.len();
            if expected_stages.get(ordinal) != Some(&stage) || value["layout"]["ok"] != true {
                return Err(format!("Invalid capture stage: {value}"));
            }
            let ledger = evidence(&cap)?;
            let draft_snapshot = durable_draft::read(&cap).await?;
            isolated(state, &cap)?;
            draft_oracle.observe(stage, cap.case(), &draft_snapshot, &expected)?;
            let mut durable = Value::Null;
            if ordinal < 3 {
                if ledger["dispatch_count"] != 0
                    || !ledger["begin_attempt"].is_null()
                    || ledger["reads"] != json!([])
                {
                    return Err("Preparation occurred before explicit Share".into());
                }
            }
            if stage == "note-ready" {
                initial_owner = value["owner"]
                    .as_str()
                    .ok_or("Missing editor owner")?
                    .into();
                let actual = delivery::read_note(&initial_owner)
                    .await
                    .ok_or("Exact mounted note unavailable")?;
                if json!(actual) != expected
                    || value["note"] != expected
                    || value["retained"]["ok"] != true
                {
                    return Err(
                        "Typed native note differs from deterministic expected blocks".into(),
                    );
                }
            }
            if ordinal >= 3 {
                durable = journal::read(&cap).await?;
                if durable["note"] != expected
                    || durable["destination"] != fixture::DESTINATION
                    || durable["file"] != fixture::FILE
                    || durable["attempt"] != ledger["begin_attempt"]
                    || ledger["reads"] != json!(["files.info", "conversations.info"])
                {
                    return Err("Admitted source/note/read ledger mismatch".into());
                }
                if cap.case() == fixture::Case::SourceStale {
                    if ledger["dispatch_count"] != 0
                        || !ledger["attempt"].is_null()
                        || !ledger["fields"].is_null()
                        || ledger["released"] != false
                    {
                        return Err("Stale source reached dispatch".into());
                    }
                } else {
                    if ledger["dispatch_count"] != 1
                        || ledger["attempt"] != durable["attempt"]
                        || !valid_form(&ledger["fields"], &expected)
                    {
                        return Err("Exact files.share form or dispatch count mismatch".into());
                    }
                }
                if ordinal == 3 {
                    let gate = if cap.case() == fixture::Case::SourceStale {
                        "awaiting_begin_release"
                    } else {
                        "awaiting_release"
                    };
                    if !ready
                        || ledger[gate] != true
                        || durable["state"] != "unconfirmed"
                        || value["retained"]["ok"] != true
                    {
                        return Err("Capture did not retain actual admitted pause".into());
                    }
                } else if durable["state"]
                    != if storage_retry && ordinal >= 8 {
                        "accepted"
                    } else {
                        journal::expected(cap.case())
                    }
                {
                    return Err("Actual journal outcome differs from case".into());
                }
                if stage == "outcome" {
                    let shell = state.peek();
                    let dialog = visible(&shell).ok_or("Outcome dialog absent")?;
                    if dialog.id != initial_owner
                        || json!(dialog.note) != expected
                        || value["retained"]["ok"] != true
                    {
                        return Err("Outcome lost mounted note identity".into());
                    }
                    let retained = retained_status(&shell, durable["attempt"].as_str().unwrap());
                    if cap.case() == fixture::Case::StorageFailure {
                        if dialog.phase != Phase::StorageFailed
                            || retained.as_ref().map(journal::label) != Some("accepted")
                        {
                            return Err("Storage failure lost retained accepted result".into());
                        }
                    } else if dialog.phase != Phase::Outcome || retained.is_some() {
                        return Err("Unexpected outcome phase/custody".into());
                    }
                }
                if stage == "recovery" {
                    let shell = state.peek();
                    let recovery = recovery_visible(&shell).ok_or("Actual recovery absent")?;
                    let count = usize::from(!matches!(
                        cap.case(),
                        fixture::Case::Accepted | fixture::Case::SourceStale
                    ));
                    if recovery.loading
                        || recovery.error.is_some()
                        || recovery.receipts.len() != count
                        || durable["unresolved"] != count
                        || recovery.drafts != draft_snapshot.drafts
                        || !recovery.captured.is_empty()
                    {
                        return Err("Recovery count differs from durable disposition".into());
                    }
                    if count == 1
                        && (recovery.receipts[0].attempt_id != durable["attempt"].as_str().unwrap()
                            || json!(recovery.receipts[0].intent.note()) != expected)
                    {
                        return Err("Recovered exact note/attempt mismatch".into());
                    }
                }
                if stage == "final" {
                    let shell = state.peek();
                    let dialog = visible(&shell).ok_or("Final real dialog absent")?;
                    if dialog.id == initial_owner {
                        return Err("Final dialog did not remount authoritative visit".into());
                    }
                    if cap.case() == fixture::Case::Accepted {
                        if dialog.phase != Phase::Editing
                            || !dialog.note.is_empty()
                            || dialog.receipt.is_some()
                            || value["note"] != json!([])
                        {
                            return Err("Accepted reopen is not fresh empty Editing".into());
                        }
                    } else if json!(dialog.note) != expected
                        || value["note"] != expected
                        || value["readonly"] != true
                    {
                        return Err("Final retained note differs".into());
                    }
                    if cap.case() == fixture::Case::SourceStale && !dialog.saved_draft_stale {
                        return Err("Changed-source saved note not reviewed".into());
                    }
                    if !matches!(
                        cap.case(),
                        fixture::Case::Accepted | fixture::Case::SourceStale
                    ) && (!source_removed
                        || dialog.receipt.as_ref().map(|r| r.attempt_id.as_str())
                            != durable["attempt"].as_str())
                    {
                        return Err("Source-free durable review mismatch".into());
                    }
                }
            }
            let mut receipt = json!({"ok":true,"identity":identity,"ordinal":ordinal,"stage":stage,"layout":value["layout"],"ui":value,"ledger":ledger,"journal":durable,"durableDraft":draft_snapshot,"noSession":true,"noTransport":true,"capabilityCurrent":true});
            if storage_retry && ordinal >= 7 {
                receipt["retry"] = retry_oracle
                    .observe(stage, state, &cap, &value, &ledger, &expected)
                    .await?;
            }
            capture(&cap, ordinal, &receipt).await?;
            isolated(state, &cap)?;
            if ordinal == 3 {
                let attempt = durable["attempt"]
                    .as_str()
                    .ok_or("Missing admitted attempt")?;
                if cap.case() == fixture::Case::SourceStale {
                    seed::mutate_source(&mut state.write(), false)?;
                    source_mutated = true;
                    cap.release_begin(attempt)
                        .map_err(|_| "Begin release refused")?;
                } else {
                    cap.release(attempt)
                        .map_err(|_| "Response release refused")?;
                }
            }
            observed.push(receipt);
            eval.send(json!({"ok":true}))
                .map_err(|_| "Stage acknowledgement failed")?;
            continue;
        }
        if value["ok"] != true || observed.len() != expected_stages.len() || !ready {
            return Err("Incomplete Share workflow".into());
        }
        let final_ledger = evidence(&cap)?;
        let final_journal = journal::read(&cap).await?;
        let final_draft = durable_draft::read(&cap).await?;
        if final_ledger != observed.last().unwrap()["ledger"]
            || final_journal != observed.last().unwrap()["journal"]
            || json!(final_draft) != observed.last().unwrap()["durableDraft"]
        {
            return Err("Replay or journal change after final capture".into());
        }
        if storage_retry {
            retry_oracle.verify_final(&cap).await?;
        }
        isolated(state, &cap)?;
        let _ = eval.send(json!({"ok":true}));
        return Ok(
            json!({"ok":true,"identity":identity,"noSession":true,"noTransport":true,"capabilityCurrent":true,"initialNote":expected,"stages":observed,"finalLedger":final_ledger,"finalJournal":final_journal,"finalDraft":final_draft,"sourceMutated":source_mutated,"sourceRemoved":source_removed,"noReplay":true,"ui":value}),
        );
    }
}
fn valid_form(fields: &Value, note: &Value) -> bool {
    fields
        == &json!([
            ["token", ""],
            ["files", fixture::FILE],
            ["channel", fixture::DESTINATION],
            ["blocks", serde_json::to_string(note).unwrap()],
            ["resharing_aware", "true"],
            ["skip_dlp_user_warning", "false"],
            ["from_share_modal", "false"]
        ])
}
async fn capture(cap: &fixture::Capability, ordinal: usize, value: &Value) -> Result<(), String> {
    let path = cap.ack_directory().join(format!("{ordinal}.ack"));
    let before = path.clone();
    if tokio::task::spawn_blocking(move || before.exists())
        .await
        .map_err(|_| "ACK check failed")?
    {
        return Err("Reused ACK refused".into());
    }
    eprintln!("file-share-stage: {value}");
    for _ in 0..600 {
        let path = path.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "ACK read failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Unreadable ACK".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Capture ACK timeout".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn form_rejects_added_permission_and_changed_flags() {
        let note = expected_note();
        let mut fields = json!([
            ["token", ""],
            ["files", fixture::FILE],
            ["channel", fixture::DESTINATION],
            ["blocks", serde_json::to_string(&note).unwrap()],
            ["resharing_aware", "true"],
            ["skip_dlp_user_warning", "false"],
            ["from_share_modal", "false"]
        ]);
        assert!(valid_form(&fields, &note));
        fields[5][1] = json!("true");
        assert!(!valid_form(&fields, &note));
        fields[5][1] = json!("false");
        fields
            .as_array_mut()
            .unwrap()
            .push(json!(["permission", "write"]));
        assert!(!valid_form(&fields, &note));
    }
}
