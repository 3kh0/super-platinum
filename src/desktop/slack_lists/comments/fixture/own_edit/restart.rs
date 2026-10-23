//! Closed fresh-process recovery evidence. Never reconstructs a write intent.
use super::*;
use super_platinum_core::slack_lists::comments::journal::{
    Journal, Scope,
    own_edit::{EditInventory, EditPhase},
};

fn named() -> bool {
    if super::super::plain::named()
        || super::super::ordered::named()
        || super::title_users::named()
        || super::noop_named()
        || super::runs_named()
    {
        return true;
    }
    matches!(
        std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
        Ok("own-edit-storage"
            | "own-edit-restart-unsent-exercise"
            | "own-edit-restart-unsent-restore"
            | "own-edit-restart-unknown-exercise"
            | "own-edit-restart-unknown-restore")
    )
}

async fn inventory(state: Signal<ShellState>) -> Result<EditInventory, String> {
    let (path, account, scope) = {
        let shell = state.peek();
        if !named() || !enabled(&shell) {
            return Err("Closed restart fixture required".into());
        }
        let session = session(&shell).map_err(|_| "Fixture identity missing")?;
        let target = &shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .ok_or("Original comments missing")?
            .target;
        (
            compose::path(&shell).ok_or("Isolated journal missing")?,
            shell
                .core
                .active_account
                .clone()
                .ok_or("Fixture account missing")?,
            Scope::capture(&session.team_id, &session.user_id, target)
                .map_err(|_| "Invalid original journal scope")?,
        )
    };
    tokio::task::spawn_blocking(move || {
        Journal::open(path, &account)
            .map_err(|e| e.to_string())?
            .edit_inventory(&scope, RECEIVED_TS)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Edit inventory read failed")?
}

pub(crate) async fn snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    serde_json::to_value(inventory(state).await?).map_err(|e| e.to_string())
}

/// Independently publish a synthetic canonical server readback from the inert
/// persisted record. This does not accept an ACK, create an Intent or send HTTP.
pub(crate) async fn publish_saved(state: Signal<ShellState>) -> Result<Value, String> {
    if std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
        != Ok("own-edit-restart-unknown-restore")
    {
        return Err("Positive recovery readback requires the exact restore case".into());
    }
    let original = model(&state.peek()).map_err(|_| "Fixture unavailable")?;
    let inventory = inventory(state).await?;
    let attempt = inventory
        .attempt()
        .ok_or("Persisted original attempt missing")?;
    if attempt.phase() != EditPhase::Uncertain || attempt.raw().is_some() {
        return Err("Original unconfirmed record required".into());
    }
    let record = attempt.record();
    let shell = state.peek();
    let current = model(&shell).map_err(|_| "Fixture retired")?;
    let session = session(&shell).map_err(|_| "Fixture identity retired")?;
    if !std::sync::Arc::ptr_eq(&original.0, &current.0)
        || record.workspace() != session.team_id
        || record.user() != session.user_id
        || !shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .is_some_and(|e| &e.target == record.target())
    {
        return Err("Original recovery scope retired".into());
    }
    let mut ledger = current.0.lock().map_err(|_| "Fixture ledger poisoned")?;
    let ledger = ledger.own_edit.as_mut().ok_or("Fixture not seeded")?;
    if !ledger.updates.is_empty()
        || ledger.published.is_some()
        || record.seed().original_raw() != &ledger.received
    {
        return Err("Restore must not issue an update or replace its original source".into());
    }
    let mut row = record.seed().original_raw().clone();
    row["blocks"] = json!(record.blocks());
    row["text"] = json!(record.text());
    row["edited"] = json!({"user":record.user(),"ts":"1781525800.000001"});
    ledger.published = Some(row.clone());
    Ok(json!({"attemptId":attempt.id(),"record":record,"row":row,
        "originalReceipt":attempt.raw(),"updates":0,"fromPersistedRecord":true}))
}
