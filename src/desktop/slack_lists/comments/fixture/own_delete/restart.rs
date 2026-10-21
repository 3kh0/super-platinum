//! Closed fresh-process evidence from the actual isolated deletion journal.
//! A saved record can seed a later server read, never another deletion or ACK.
use super::*;
use super_platinum_core::slack_lists::comments::journal::{
    Journal, Scope,
    own_delete::{DeleteInventory, DeletePhase},
};

fn case() -> String {
    std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default()
}
fn named(value: &str) -> bool {
    matches!(
        value,
        "own-delete-restart-pending-exercise"
            | "own-delete-restart-pending-restore"
            | "own-delete-restart-unknown-exercise"
            | "own-delete-restart-unknown-restore"
    )
}
fn restore_phase(value: &str) -> Option<DeletePhase> {
    match value {
        "own-delete-restart-pending-restore" => Some(DeletePhase::Pending),
        "own-delete-restart-unknown-restore" => Some(DeletePhase::Uncertain),
        _ => None,
    }
}

async fn inventory(state: Signal<ShellState>) -> Result<(Model, DeleteInventory), String> {
    let original_case = case();
    let (original, owner, target, path, account, scope) = {
        let shell = state.peek();
        if !named(&original_case) || !enabled(&shell) {
            return Err("Closed deletion restart fixture required".into());
        }
        let session = session(&shell).map_err(|_| "Fixture identity missing")?;
        let entry = shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .ok_or("Original comments missing")?;
        if !entry.owner.active(&shell) {
            return Err("Original comments retired".into());
        }
        (
            model(&shell).map_err(|_| "Fixture unavailable")?,
            entry.owner.clone(),
            entry.target.clone(),
            compose::path(&shell).ok_or("Isolated journal missing")?,
            shell
                .core
                .active_account
                .clone()
                .ok_or("Fixture account missing")?,
            Scope::capture(&session.team_id, &session.user_id, &entry.target)
                .map_err(|_| "Invalid original journal scope")?,
        )
    };
    let read_path = path.clone();
    let read_account = account.clone();
    let read_scope = scope.clone();
    let inventory = tokio::task::spawn_blocking(move || {
        Journal::open(read_path, &read_account)
            .map_err(|e| e.to_string())?
            .delete_inventory(&read_scope, RECEIVED_TS)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Delete inventory read failed")??;
    let shell = state.peek();
    let current = model(&shell).map_err(|_| "Fixture retired")?;
    let session = session(&shell).map_err(|_| "Fixture identity retired")?;
    if case() != original_case
        || !std::sync::Arc::ptr_eq(&original.0, &current.0)
        || !owner.active(&shell)
        || shell.core.active_account.as_ref() != Some(&account)
        || compose::path(&shell).as_ref() != Some(&path)
        || !shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .is_some_and(|e| e.target == target)
        || Scope::capture(&session.team_id, &session.user_id, &target)
            .ok()
            .as_ref()
            != Some(&scope)
        || inventory.scope() != &scope
        || inventory.ts() != RECEIVED_TS
    {
        return Err("Original deletion journal scope retired".into());
    }
    Ok((original, inventory))
}

/// Reads disk independently of the controller's optional retained recovery.
pub(crate) async fn snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    serde_json::to_value(inventory(state).await?.1).map_err(|e| e.to_string())
}

fn pristine(state: &State) -> bool {
    state.policy.is_empty()
        && state.prepared.is_empty()
        && state.deletes.is_empty()
        && state.records.is_empty()
        && state.responses.is_empty()
        && state.releases.is_empty()
        && state.held.is_none()
        && state.future_deleted.is_none()
        && state.published.is_none()
}

pub(crate) async fn publish_saved(state: Signal<ShellState>) -> Result<Value, String> {
    let expected = restore_phase(&case()).ok_or("Exact deletion restore case required")?;
    let (model, inventory) = inventory(state).await?;
    let attempt = inventory
        .attempt()
        .ok_or("Persisted original deletion missing")?;
    if attempt.phase() != expected || attempt.raw().is_some() || attempt.readback().is_some() {
        return Err("Original pending or unconfirmed deletion record required".into());
    }
    let record = attempt.record();
    record
        .validate()
        .map_err(|_| "Invalid original deletion record")?;
    let shell = state.peek();
    let session = session(&shell).map_err(|_| "Fixture identity retired")?;
    let mut ledger = model.0.lock().map_err(|_| "Fixture ledger poisoned")?;
    if record.workspace() != session.team_id
        || record.user() != session.user_id
        || record.ts() != RECEIVED_TS
        || record.target().list_id() != LIST
        || record.target().record_id() != "R_LIST_000"
        || record.target().channel_id() != "CCOMMENT01"
        || record.target().root_ts() != Some(ROOT)
        || record.target().document_raw() != &ledger.document
        || ledger.records.get("R_LIST_000") != Some(record.target().record_raw())
        || !shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .is_some_and(|e| &e.target == record.target())
    {
        return Err("Saved deletion source differs from the original fixture".into());
    }
    let ledger = ledger
        .own_delete
        .as_mut()
        .ok_or("Delete fixture not seeded")?;
    if !pristine(ledger) || record.original_raw() != &ledger.seed["received"] {
        return Err(
            "Restore must preserve its original source and issue no deletion or policy requests"
                .into(),
        );
    }
    ledger.future_deleted = Some(record.ts().into());
    ledger.published = Some(record.ts().into());
    Ok(
        json!({"attemptId":attempt.id(),"record":record,"originalReceipt":attempt.raw(),
        "deletes":0,"fromPersistedRecord":true,"deleted":record.ts()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_restore_cases_authorize_their_original_phase() {
        assert_eq!(
            restore_phase("own-delete-restart-pending-restore"),
            Some(DeletePhase::Pending)
        );
        assert_eq!(
            restore_phase("own-delete-restart-unknown-restore"),
            Some(DeletePhase::Uncertain)
        );
        for value in [
            "own-delete",
            "own-delete-restart-pending-exercise",
            "own-delete-restart-unknown-exercise",
            "own-delete-restart-unknown-restore-extra",
        ] {
            assert_eq!(restore_phase(value), None);
        }
        assert!(!named("own-delete"));
        assert!(named("own-delete-restart-pending-exercise"));
    }
    #[test]
    fn restoration_refuses_any_dispatch_policy_or_prior_publication() {
        assert!(pristine(&State::default()));
        for field in 0..9 {
            let mut value = State::default();
            match field {
                0 => value.policy.push(json!({})),
                1 => value.prepared.push(json!({})),
                2 => value.deletes.push(json!({})),
                3 => value.records.push(json!({})),
                4 => value.responses.push(json!({})),
                5 => value.releases.push(json!({})),
                6 => value.held = Some(oneshot::channel().0),
                7 => value.future_deleted = Some(RECEIVED_TS.into()),
                _ => value.published = Some(RECEIVED_TS.into()),
            }
            assert!(!pristine(&value), "guard {field}");
        }
    }
}
