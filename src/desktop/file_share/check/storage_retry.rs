//! Supplemental actual UI storage settlement. Never edits the journal or ledger.
use super::*;
use super_platinum_core::file_sharing::{Receipt, Scope, State as ReceiptState, store::Journal};

pub(super) const STAGES: [&str; 3] = ["retry-ready", "retry-saved", "retry-recovery"];

#[derive(Default)]
pub(super) struct Oracle {
    before: Option<Receipt>,
    saved: Option<Receipt>,
    ledger: Option<Value>,
    owner: String,
    seen: usize,
}
fn immutable(a: &Receipt, b: &Receipt) -> bool {
    a.attempt_id == b.attempt_id
        && a.scope == b.scope
        && a.intent == b.intent
        && a.file_title == b.file_title
        && a.destination_name == b.destination_name
        && a.created_at == b.created_at
}
fn saved_agrees(before: &Receipt, saved: &Receipt) -> bool {
    immutable(before, saved)
        && before.state() == ReceiptState::Unconfirmed
        && before.raw_response().is_none()
        && saved.raw_response() == Some(&json!({"ok":true,"file_msg_ts":"1719800400.000001"}))
        && journal::label(&saved.state()) == "accepted"
        && !saved.unresolved()
}
async fn read(cap: &fixture::Capability) -> Result<Receipt, String> {
    let path = cap.storage_path().to_path_buf();
    tokio::task::spawn_blocking(move || {
        Journal::open(path, fixture::ACCOUNT)
            .map_err(|_| "Retry journal open failed".to_owned())?
            .get(&Scope {
                team: fixture::TEAM.into(),
                user: fixture::USER.into(),
                file: fixture::FILE.into(),
                destination: fixture::DESTINATION.into(),
            })
            .map_err(|_| "Retry journal read failed".to_owned())?
            .ok_or_else(|| "Retry admitted receipt missing".to_owned())
    })
    .await
    .map_err(|_| "Retry journal worker failed")?
}
impl Oracle {
    pub(super) async fn verify_final(&self, cap: &fixture::Capability) -> Result<(), String> {
        let row = read(cap).await?;
        if self.seen != STAGES.len()
            || self.saved.as_ref() != Some(&row)
            || self.ledger.as_ref() != Some(&evidence(cap)?)
        {
            return Err("Retry raw receipt/ledger changed after final capture".into());
        }
        Ok(())
    }
    pub(super) async fn observe(
        &mut self,
        stage: &str,
        state: Signal<ShellState>,
        cap: &fixture::Capability,
        ui: &Value,
        ledger: &Value,
        note: &Value,
    ) -> Result<Value, String> {
        if cap.case() != fixture::Case::StorageFailure
            || STAGES.get(self.seen).copied() != Some(stage)
        {
            return Err("Retry stage/case changed".into());
        }
        let row = read(cap).await?;
        isolated(state, cap)?;
        let shell = state.peek();
        let retained = retained_status(&shell, &row.attempt_id);
        let pending_count = shell
            .file_share
            .custody
            .lock()
            .map_err(|_| "Retry custody poisoned")?
            .pending
            .len();
        if ledger["dispatch_count"] != 1
            || ledger["reads"] != json!(["files.info", "conversations.info"])
            || ledger["attempt"] != row.attempt_id
            || ledger["begin_attempt"] != row.attempt_id
            || ledger["released"] != true
            || ledger["awaiting_release"] != false
            || !valid_form(&ledger["fields"], note)
            || json!(row.intent.note()) != *note
        {
            return Err("Retry changed original dispatch/note identity".into());
        }
        if let Some(original) = &self.ledger {
            if original != ledger {
                return Err("Retry changed immutable prepared dispatch ledger".into());
            }
        }
        let retry_ready = if let Some(dialog) = visible(&shell) {
            super::super::retry_status(&shell, &dialog.id) == Some(RetryState::Ready)
        } else {
            false
        };
        if stage == "retry-ready" {
            let dialog = visible(&shell).ok_or("Retry review absent")?;
            if dialog.phase != Phase::Outcome
                || dialog.closing
                || dialog.receipt.as_ref() != Some(&row)
                || json!(dialog.note) != *note
                || row.state() != ReceiptState::Unconfirmed
                || row.raw_response().is_some()
                || json!(row)["outcome"] != Value::Null
                || !row.unresolved()
                || !retry_ready
                || retained.as_ref().map(journal::label) != Some("accepted")
                || pending_count != 1
                || ui["owner"] != dialog.id
                || ui["note"] != *note
                || ui["retained"]["ok"] != true
                || ui["retained"]["selectionText"] != "Caf\u{e9}"
                || ui["retained"]["backward"] != true
                || ui["localSave"]["label"] != "Save result again"
                || ui["localSave"]["disabled"] != false
                || ui["localSave"]["geometry"]["ok"] != true
                || ui["unsaved"] != true
            {
                return Err("Retry did not start from actual retained unsaved receipt".into());
            }
            self.owner = dialog.id.clone();
            self.before = Some(row.clone());
            self.ledger = Some(ledger.clone());
        } else {
            if !self
                .before
                .as_ref()
                .is_some_and(|before| saved_agrees(before, &row))
                || pending_count != 0
                || retained.is_some()
                || retry_ready
            {
                return Err(
                    "Retry did not persist exact original result and release custody".into(),
                );
            }
            if stage == "retry-saved" {
                let dialog = visible(&shell).ok_or("Saved retry dialog absent")?;
                if dialog.id != self.owner
                    || dialog.phase != Phase::Outcome
                    || dialog.receipt.as_ref() != Some(&row)
                    || json!(dialog.note) != *note
                    || ui["owner"] != self.owner
                    || ui["note"] != *note
                    || ui["retained"]["ok"] != true
                    || ui["retained"]["sameRoot"] != true
                    || ui["retained"]["sameDialog"] != true
                    || ui["retained"]["selectionRetained"] != true
                    || ui["retained"]["selectionText"] != "Caf\u{e9}"
                    || ui["retained"]["backward"] != true
                    || ui["unsaved"] != false
                    || ui["retryButtonAbsent"] != true
                {
                    return Err("Saved retry lost exact readonly editor/receipt".into());
                }
                self.saved = Some(row.clone());
            } else {
                let recovery = recovery_visible(&shell).ok_or("Retry recovery absent")?;
                if self.saved.as_ref() != Some(&row)
                    || visible(&shell).is_some()
                    || recovery.loading
                    || recovery.error.is_some()
                    || !recovery.receipts.is_empty()
                    || !recovery.drafts.is_empty()
                    || !recovery.captured.is_empty()
                    || ui["count"] != 0
                {
                    return Err("Retry recovery did not reload exact resolved receipt".into());
                }
            }
        }
        self.seen += 1;
        Ok(
            json!({"receipt":row,"pendingCount":pending_count,"retainedState":retained.as_ref().map(journal::label),"retryReady":retry_ready,"ledgerUnchanged":true}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Receipt {
        serde_json::from_value(json!({
            "attempt_id":"fe871ad5-a80d-4abd-93cd-c31f76ce5d89",
            "scope":{"team":fixture::TEAM,"user":fixture::USER,"file":fixture::FILE,"destination":fixture::DESTINATION},
            "intent":{"file":fixture::FILE,"destination":fixture::DESTINATION,"note":expected_note(),"context_team":null},
            "file_title":"Fixture","destination_name":"share-fixture","created_at":10,"updated_at":10,"outcome":null
        })).unwrap()
    }
    #[test]
    fn exact_retry_receipt_oracle_refuses_note_attempt_and_raw_changes() {
        let original = row();
        let mut raw = json!(original);
        raw["outcome"] = json!({"Response":{"ok":true,"file_msg_ts":"1719800400.000001"}});
        raw["updated_at"] = json!(20);
        let accepted: Receipt = serde_json::from_value(raw.clone()).unwrap();
        assert!(saved_agrees(&original, &accepted));
        for field in ["attempt_id", "file_title", "destination_name", "created_at"] {
            let mut changed = raw.clone();
            changed[field] = if field == "created_at" {
                json!(11)
            } else {
                json!("different")
            };
            assert!(!saved_agrees(
                &original,
                &serde_json::from_value(changed).unwrap()
            ));
        }
        for (path, value) in [("note", json!([])), ("destination", json!("C00000002"))] {
            let mut changed = raw.clone();
            changed["intent"][path] = value;
            assert!(!saved_agrees(
                &original,
                &serde_json::from_value(changed).unwrap()
            ));
        }
        raw["outcome"]["Response"]["file_msg_ts"] = json!("changed");
        assert!(!saved_agrees(
            &original,
            &serde_json::from_value(raw).unwrap()
        ));
        assert!(!saved_agrees(&original, &original));
    }
}
