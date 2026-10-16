//! Separate two-process workflow. Existing Share/Close/retry oracles stay intact.
use super::*;
use fixture::reopen::{Input, Variant};
use std::path::PathBuf;

pub(crate) fn install(shell: &mut ShellState) {
    seed::prepare(shell);
    let result = (|| {
        let cap = fixture::reopen::load(shell).map_err(|_| "Restart handoff ownership rejected")?;
        match cap.restart().ok_or("Restart seal missing")?.variant {
            Variant::Exact => (),
            Variant::Changed => seed::mutate_source(shell, false)?,
            Variant::Missing => seed::mutate_source(shell, true)?,
        }
        shell.file_share_fixture = Some(cap);
        Ok::<(), String>(())
    })();
    if result.is_err() {
        eprintln!(
            "file-share-restart-check: {{\"ok\":false,\"error\":\"Restart startup refused\"}}"
        );
    }
}

/// Returns true for this workflow even when invalid, prohibiting legacy fallback.
pub(super) fn start_if_requested(state: Signal<ShellState>) -> bool {
    let mode = std::env::var("SUPER_PLATINUM_FILE_SHARE_CHECK_MODE").unwrap_or_default();
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if !matches!(mode.as_str(), "durable-close-capture" | "durable-reopen")
        && alias != "file-share-draft-reopen"
    {
        return false;
    }
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::Relaxed) {
        return true;
    }
    let valid = crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_FILE_SHARE_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_FILE_SHARE_CASE").as_deref() == Ok("accepted")
        && matches!(
            (alias.as_str(), mode.as_str()),
            ("file-share", "durable-close-capture") | ("file-share-draft-reopen", "durable-reopen")
        );
    if !valid {
        eprintln!(
            "file-share-restart-check: {{\"ok\":false,\"error\":\"Exact restart workflow required\"}}"
        );
        return true;
    }
    dioxus::core::spawn_forever(async move {
        let result = run(state)
            .await
            .unwrap_or_else(|error| json!({"ok":false,"error":error}));
        eprintln!("file-share-restart-check: {result}");
    });
    true
}
use super_platinum_core::file_sharing::store::{
    Journal,
    drafts::{Draft, DraftScope},
};

fn empty_ledger(value: &Value) -> bool {
    value["case"] == "accepted"
        && value["reads"] == json!([])
        && value["dispatch_count"] == 0
        && value["attempt"].is_null()
        && value["fields"].is_null()
        && value["begin_attempt"].is_null()
        && value["awaiting_release"] == false
        && value["awaiting_begin_release"] == false
        && value["released"] == false
}
fn idle(state: Signal<ShellState>, cap: &fixture::Capability) -> Result<Value, String> {
    isolated(state, cap)?;
    let shell = state.peek();
    let held = shell
        .file_share
        .custody
        .lock()
        .map_err(|_| "Custody poisoned")?;
    let queue = held
        .draft_io
        .diagnostic()
        .map_err(|_| "Queue diagnostic refused")?;
    if !queue.idle() || !held.pending.is_empty() || !held.drafts.is_empty() {
        return Err("Draft queue/custody is not quiescent".into());
    }
    let ledger = evidence(cap)?;
    if !empty_ledger(&ledger) {
        return Err("Restart workflow reached read/admission/dispatch".into());
    }
    Ok(json!({"queue":queue,"pending":0,"legacyDrafts":0,"ledger":ledger}))
}
async fn settled_idle(
    state: Signal<ShellState>,
    cap: &fixture::Capability,
) -> Result<Value, String> {
    for _ in 0..100 {
        if let Ok(v) = idle(state, cap) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    idle(state, cap)
}
async fn row(cap: &fixture::Capability) -> Result<Draft, String> {
    let path = cap.storage_path().to_owned();
    tokio::task::spawn_blocking(move || {
        let journal =
            Journal::open(path, fixture::ACCOUNT).map_err(|_| "Private draft open failed")?;
        let scope = DraftScope {
            team: fixture::TEAM.into(),
            user: fixture::USER.into(),
            file: fixture::FILE.into(),
        };
        let row = journal
            .draft(&scope)
            .map_err(|_| "Private draft read failed")?
            .ok_or("Draft row missing")?;
        if journal
            .drafts(fixture::TEAM, fixture::USER)
            .map_err(|_| "Draft inventory failed")?
            != vec![row.clone()]
            || !journal
                .unresolved(fixture::TEAM, fixture::USER)
                .map_err(|_| "Receipt read failed")?
                .is_empty()
            || journal
                .retry_at(fixture::TEAM, fixture::USER)
                .map_err(|_| "Cooldown read failed")?
                != 0
        {
            return Err("Unexpected durable inventory");
        }
        Ok(row)
    })
    .await
    .map_err(|_| "Draft reader task failed")?
    .map_err(str::to_owned)
}
fn source(state: Signal<ShellState>) -> Option<Value> {
    state
        .peek()
        .core
        .workspaces
        .get(fixture::TEAM)?
        .messages
        .get(fixture::DESTINATION)?
        .messages
        .iter()
        .flat_map(|m| &m.files)
        .find(|f| f.id.as_deref() == Some(fixture::FILE))
        .and_then(|f| serde_json::to_value(f).ok())
}
async fn capture_stage(
    cap: &fixture::Capability,
    ordinal: usize,
    value: &Value,
) -> Result<(), String> {
    let ack = cap.ack_directory().join(format!("{ordinal}.ack"));
    let prior = ack.clone();
    if tokio::task::spawn_blocking(move || std::fs::symlink_metadata(prior).is_ok())
        .await
        .map_err(|_| "ACK preflight task failed")?
    {
        return Err("Reused ACK".into());
    }
    eprintln!("file-share-restart-stage: {value}");
    for _ in 0..600 {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "ACK task failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Invalid ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Unreadable ACK".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Capture ACK timeout".into())
}
fn note(input: Input) -> Value {
    if input == Input::Text {
        expected_note()
    } else {
        json!([])
    }
}
fn stored_content(draft: &Draft, input: Input) -> bool {
    json!(draft.content.note) == note(input)
        && draft.content.destination.as_ref().is_some_and(|d| {
            d.id == fixture::DESTINATION && d.name == "share-fixture" && !d.private
        })
}
async fn artifact(path: PathBuf, draft: Draft) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let bytes = serde_json::to_vec(&draft).map_err(|_| "Draft encode failed")?;
        if bytes.len() > 1024 * 1024 {
            return Err("Draft artifact exceeds bound");
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "Draft artifact already exists or cannot open")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Draft artifact write failed")
    })
    .await
    .map_err(|_| "Artifact task failed")?
    .map_err(str::to_owned)
}
pub(super) async fn run(state: Signal<ShellState>) -> Result<Value, String> {
    let cap = state
        .peek()
        .file_share_fixture
        .clone()
        .ok_or("Missing restart capability")?;
    isolated(state, &cap)?;
    let reopened = cap.restart().is_some();
    let mode = std::env::var("SUPER_PLATINUM_FILE_SHARE_CHECK_MODE")
        .map_err(|_| "Missing restart mode")?;
    if (reopened && mode != "durable-reopen")
        || (!reopened && mode != "durable-close-capture")
        || cap.case() != fixture::Case::Accepted
    {
        return Err("Wrong workflow capability".into());
    }
    let input = if let Some(seal) = cap.restart() {
        seal.input
    } else {
        match std::env::var("SUPER_PLATINUM_FILE_SHARE_DRAFT_INPUT").as_deref() {
            Ok("text") => Input::Text,
            Ok("destination-only") => Input::DestinationOnly,
            _ => return Err("Exact draft input required".into()),
        }
    };
    let mut identity = identity()?;
    identity["workflow"] = json!(mode);
    identity["input"] = json!(input);
    let initial = settled_idle(state, &cap).await?;
    if visible(&state.peek()).is_some() || recovery_visible(&state.peek()).is_some() {
        return Err("Restart began with reused presentation".into());
    }
    let reference = if let Some(seal) = cap.restart() {
        let row = row(&cap).await?;
        isolated(state, &cap)?;
        if row != seal.draft {
            return Err("Reopen row changed before UI".into());
        }
        let canonical = source(state);
        let valid = match seal.variant {
            Variant::Exact => canonical.as_ref() == Some(&row.content.source),
            Variant::Changed => canonical.as_ref().is_some_and(|c| c != &row.content.source),
            Variant::Missing => canonical.is_none(),
        };
        if !valid {
            return Err("Canonical restart variant mismatch".into());
        }
        identity["variant"] = json!(seal.variant);
        identity["run"] = json!(seal.run);
        identity["markerSha256"] = json!(seal.marker_sha256);
        Some(row)
    } else {
        if cap.ack_directory().file_name().and_then(|s| s.to_str()) != Some("capture") {
            return Err("Fresh capture root required".into());
        }
        None
    };
    let stages: Vec<&str> = if !reopened {
        vec!["claimed", "input", "closed"]
    } else if cap.restart().unwrap().variant == Variant::Exact {
        vec!["validated", "restored"]
    } else {
        vec!["validated", "list", "restored"]
    };
    let mut eval = document::eval(&format!(
        "{};\n{}",
        include_str!("../../view/file_share/check.js"),
        include_str!("restart/ui.js")
    ));
    eval.send(json!({"identity":identity,"note":note(input),"destination":fixture::DESTINATION,"reopened":reopened,"variant":cap.restart().map(|s|s.variant)})).map_err(|_|"Init send failed")?;
    let mut observed = Vec::new();
    let mut claimed: Option<Draft> = None;
    let mut owner = String::new();
    loop {
        let value: Value = tokio::time::timeout(Duration::from_secs(20), eval.recv())
            .await
            .map_err(|_| "Restart UI timeout")?
            .map_err(|_| "Restart UI closed")?;
        isolated(state, &cap)?;
        if value["ok"] == false {
            return Err(format!("Restart UI assertion: {value}"));
        }
        if let Some(stage) = value["stage"].as_str() {
            let ordinal = observed.len();
            if stages.get(ordinal) != Some(&stage) || value["layout"]["ok"] != true {
                return Err("Invalid restart stage/layout".into());
            }
            let idle = settled_idle(state, &cap).await?;
            let stored = row(&cap).await?;
            isolated(state, &cap)?;
            if matches!(stage, "claimed" | "input") {
                let shell = state.peek();
                let dialog = visible(&shell).ok_or("Actual capture dialog missing")?;
                if dialog.phase != Phase::Editing
                    || dialog.closing
                    || !dialog.editor_ready
                    || dialog.error.is_some()
                    || dialog.durable.as_ref() != Some(&stored)
                    || dialog.admission.is_none()
                    || dialog.draft_storage.is_none()
                    || value["owner"] != dialog.id
                    || dialog.receipt.is_some()
                    || dialog.draft_local
                {
                    return Err("Capture dialog/claim ownership mismatch".into());
                }
                if stage == "input"
                    && dialog.destination.as_ref().is_none_or(|d| {
                        d.id != fixture::DESTINATION || d.name != "share-fixture" || d.private
                    })
                {
                    return Err("Actual chosen destination mismatch".into());
                }
            }
            if stage == "claimed" {
                if stored.revision != 1 || !stored.content.empty() {
                    return Err("Initial Open did not create exact empty reservation".into());
                }
                claimed = Some(stored.clone());
            } else if stage == "input" {
                if claimed.as_ref() != Some(&stored) || value["note"] != note(input) {
                    return Err("Input autosaved or captured wrong note".into());
                }
                owner = value["owner"]
                    .as_str()
                    .ok_or("Missing editor owner")?
                    .into();
                if json!(
                    delivery::read_note(&owner)
                        .await
                        .ok_or("Mounted note read failed")?
                ) != note(input)
                {
                    return Err("Actual editor read mismatch".into());
                }
            } else if stage == "closed" {
                let before = claimed.as_ref().ok_or("Missing initial claim")?;
                if stored.token != before.token
                    || stored.revision != 2
                    || stored.content.source != before.content.source
                    || stored.content.title != before.content.title
                    || !stored_content(&stored, input)
                    || visible(&state.peek()).is_some()
                    || value["owner"] != owner
                    || value["returnedFocus"] != true
                    || value["oldDetached"] != true
                {
                    return Err("Close did not leave exact committed row and released UI".into());
                }
                artifact(
                    cap.ack_directory()
                        .parent()
                        .ok_or("Pair root absent")?
                        .join("closed-draft.json"),
                    stored.clone(),
                )
                .await?;
            } else if stage == "validated" || stage == "list" {
                if reference.as_ref() != Some(&stored) {
                    return Err("Readonly startup/list claimed or changed draft".into());
                }
                if stage == "list" {
                    let shell = state.peek();
                    let recovery =
                        recovery_visible(&shell).ok_or("Actual saved draft list absent")?;
                    if value["token"] != stored.token
                        || value["revision"] != stored.revision
                        || recovery.loading
                        || recovery.error.is_some()
                        || recovery.drafts != vec![stored.clone()]
                        || !recovery.receipts.is_empty()
                        || !recovery.captured.is_empty()
                    {
                        return Err("Unexpected recovery inventory".into());
                    }
                }
            } else if stage == "restored" {
                let before = reference.as_ref().ok_or("Missing handoff draft")?;
                let exact = cap.restart().unwrap().variant == Variant::Exact;
                if stored.content != before.content
                    || stored.scope != before.scope
                    || (exact && (stored.token == before.token || stored.revision != 3))
                    || (!exact && stored != *before)
                {
                    return Err("Restore changed body or wrong claim transition".into());
                }
                let shell = state.peek();
                let dialog = visible(&shell).ok_or("Restored dialog absent")?;
                if value["owner"] != dialog.id
                    || dialog.destination.as_ref().is_none_or(|d| {
                        d.id != fixture::DESTINATION || d.name != "share-fixture" || d.private
                    })
                    || dialog.durable.as_ref() != Some(&stored)
                    || json!(dialog.note) != note(input)
                    || dialog.receipt.is_some()
                    || dialog.closing
                    || !dialog.editor_ready
                    || dialog.draft_local
                    || (exact && dialog.error.is_some())
                    || dialog.phase
                        != if exact {
                            Phase::Editing
                        } else {
                            Phase::DraftReview
                        }
                    || (!exact && dialog.admission.is_some())
                    || (exact && dialog.admission.as_ref().is_none_or(|s| !s.current(&shell)))
                    || dialog.saved_draft_stale != !exact
                    || dialog.draft_visit.as_ref().is_none_or(|visit| {
                        visit.writable() != exact
                            || !visit.active()
                            || visit.source != stored.content.source
                            || visit.key != (cap.key(), fixture::FILE.into())
                    })
                    || value["note"] != note(input)
                    || value["readonly"] != !exact
                {
                    return Err("Restored editor/durable ownership mismatch".into());
                }
            }
            isolated(state, &cap)?;
            let event = json!({"ok":true,"identity":identity,"ordinal":ordinal,"stage":stage,"ui":value,"idle":idle,"draft":stored,"noSession":true,"noTransport":true,"capabilityCurrent":true});
            capture_stage(&cap, ordinal, &event).await?;
            isolated(state, &cap)?;
            observed.push(event);
            eval.send(json!({"ok":true}))
                .map_err(|_| "Stage ACK failed")?;
            continue;
        }
        if value["ok"] != true
            || observed.len() != stages.len()
            || value["forbiddenActions"] != 0
            || value["noEditorDuringLoading"] != true
            || value["actualClose"] != !reopened
            || value["actualRestore"] != reopened
        {
            return Err("Incomplete restart workflow".into());
        }
        let idle = settled_idle(state, &cap).await?;
        let final_draft = row(&cap).await?;
        isolated(state, &cap)?;
        if observed.last().and_then(|event| event.get("draft")) != Some(&json!(final_draft)) {
            return Err("Draft changed after final observed stage".into());
        }
        let _ = eval.send(json!({"ok":true}));
        return Ok(
            json!({"ok":true,"identity":identity,"initialIdle":initial,"stages":observed,"finalIdle":idle,"finalDraft":final_draft,"noSession":true,"noTransport":true,"noBegin":true,"noDispatch":true,"ui":value}),
        );
    }
}
