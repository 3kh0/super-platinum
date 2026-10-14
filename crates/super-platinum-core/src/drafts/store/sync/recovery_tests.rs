use super::tests::{Database, local, scope};
use super::*;
use crate::drafts::{
    codec,
    sync::{CheckBlock, ConsumeKind, OperationKind, RemoteDraft},
};

fn uncertain(store: &mut DraftStore, kind: OperationKind) -> (Stored, Operation, LocalDraft) {
    let scope = scope();
    let mut state = DraftSync::adopt_remote(
        &scope,
        RemoteDraft {
            scope: scope.clone(),
            id: "DrCheck".into(),
            client_msg_id: uuid::Uuid::new_v4().to_string(),
            last_updated_ts: "100.000001".into(),
            snapshot: codec::encode(&scope.key, "Original").unwrap(),
        },
    )
    .unwrap();
    let local = if kind == OperationKind::Send {
        state
            .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
            .unwrap();
        LocalDraft::default()
    } else {
        state.edit(&scope, Some(codec::encode(&scope.key, "Edited").unwrap()));
        local("Edited")
    };
    let operation = state.begin_operation(&scope).unwrap();
    assert_eq!(operation.kind, kind);
    state.mark_uncertain(&scope, &operation.id);
    store.save([(scope.key.clone(), local.clone())]).unwrap();
    let Commit::Applied(stored) = store
        .commit_synced(&scope, None, &local, &state, None)
        .unwrap()
    else {
        panic!()
    };
    (stored, operation, local)
}

fn saved(
    store: &mut DraftStore,
    previous: &Stored,
    state: &DraftSync,
    local: &LocalDraft,
) -> Stored {
    let Commit::Applied(stored) = store
        .commit_synced(state.scope(), Some(previous.revision), local, state, None)
        .unwrap()
    else {
        panic!()
    };
    stored
}

#[test]
fn pending_send_and_update_tickets_round_trip_and_old_formats_cannot_read_new_guard() {
    for kind in [OperationKind::Send, OperationKind::Update] {
        let db = Database::new();
        let mut store = db.open();
        let (row, op, local) = uncertain(&mut store, kind);
        let mut pending = row.state.clone();
        let ticket = pending.reserve_check(&op, 1_000).unwrap();
        let committed = saved(&mut store, &row, &pending, &local);
        let format: i64 = store
            .0
            .query_row("SELECT format_version FROM composer_draft_sync", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(format, 5);
        assert!(!matches!(format, 1 | 2 | 3 | 4));
        drop(store);
        let loaded = db
            .open()
            .load_synced(&scope().account_id, LoadMode::AfterRestart)
            .unwrap();
        let restored = &loaded.synced[&scope().key];
        assert_eq!(restored.state, committed.state);
        assert_eq!(restored.revision, committed.revision);
        assert_eq!(restored.state.pending_check(&op), Some(&ticket));
        assert_eq!(restored.state.check_retry_at_ms(&op), None);
        assert_eq!(
            restored.state.clone().reserve_check(&op, u64::MAX),
            Err(CheckBlock::Pending)
        );
    }
}

#[test]
fn failed_completion_write_keeps_pending_guard_after_restart_instead_of_losing_long_deadline() {
    let db = Database::new();
    let mut store = db.open();
    let (row, op, local) = uncertain(&mut store, OperationKind::Update);
    let mut pending = row.state.clone();
    let ticket = pending.reserve_check(&op, 1_000).unwrap();
    let committed = saved(&mut store, &row, &pending, &local);
    let mut complete = committed.state.clone();
    assert!(complete.finish_check(&ticket, 86_400_000));
    store.0.execute_batch("CREATE TRIGGER fail_completion BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(FAIL,'fixture write failure'); END;").unwrap();
    assert!(
        store
            .commit_synced(&scope(), Some(committed.revision), &local, &complete, None)
            .is_err()
    );
    drop(store);
    let mut store = db.open();
    let loaded = store
        .load_synced(&scope().account_id, LoadMode::AfterRestart)
        .unwrap();
    let retained = &loaded.synced[&scope().key];
    assert_eq!(retained.state.pending_check(&op), Some(&ticket));
    assert_eq!(retained.state.check_retry_at_ms(&op), None);
    assert_eq!(
        retained.state.clone().reserve_check(&op, u64::MAX),
        Err(CheckBlock::Pending)
    );
    // A process retaining the exact read result may retry only its disk commit.
    store
        .0
        .execute_batch("DROP TRIGGER fail_completion;")
        .unwrap();
    let completed = saved(&mut store, retained, &complete, &local);
    drop(store);
    let restored = db
        .open()
        .load_synced(&scope().account_id, LoadMode::AfterRestart)
        .unwrap();
    assert_eq!(restored.synced[&scope().key].state, completed.state);
    assert_eq!(completed.state.check_retry_at_ms(&op), Some(86_400_000));
}

#[test]
fn failed_reservation_and_stale_cas_never_install_a_dispatch_receipt() {
    let db = Database::new();
    let mut store = db.open();
    let (row, op, local) = uncertain(&mut store, OperationKind::Update);
    let mut pending = row.state.clone();
    pending.reserve_check(&op, 0).unwrap();
    assert!(matches!(
        store
            .commit_synced(&scope(), Some(row.revision + 1), &local, &pending, None)
            .unwrap(),
        Commit::Stale(_)
    ));
    store.0.execute_batch("CREATE TRIGGER fail_reservation BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(FAIL,'fixture write failure'); END;").unwrap();
    assert!(
        store
            .commit_synced(&scope(), Some(row.revision), &local, &pending, None)
            .is_err()
    );
    drop(store);
    let loaded = db
        .open()
        .load_synced(&scope().account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(loaded.synced[&scope().key].state, row.state);
    assert!(
        loaded.synced[&scope().key]
            .state
            .pending_check(&op)
            .is_none()
    );
}

#[test]
fn legacy_formats_preserve_existing_deadline_without_inventing_a_pending_check() {
    for format in [1, 2, 3, 4] {
        let db = Database::new();
        let mut store = db.open();
        let (row, op, local) = uncertain(&mut store, OperationKind::Send);
        let mut state = row.state.clone();
        assert!(state.defer_check(&op, 120_000));
        let mut raw = serde_json::to_value(&state).unwrap();
        raw.as_object_mut().unwrap().remove("check_ticket");
        raw["check_deadline"]
            .as_object_mut()
            .unwrap()
            .remove("operation");
        store
            .0
            .execute(
                "UPDATE composer_draft_sync SET format_version=?1,state=?2",
                params![format, serde_json::to_string(&raw).unwrap()],
            )
            .unwrap();
        let loaded = store
            .load_synced(&scope().account_id, LoadMode::AfterRestart)
            .unwrap();
        let restored = &loaded.synced[&scope().key];
        assert_eq!(restored.state.check_retry_at_ms(&op), Some(120_000));
        assert!(restored.state.pending_check(&op).is_none());
        let mut admitted = restored.state.clone();
        assert_eq!(
            admitted.reserve_check(&op, 119_999),
            Err(CheckBlock::Cooldown(120_000))
        );
        let ticket = admitted.reserve_check(&op, 120_000).unwrap();
        let committed = saved(&mut store, restored, &admitted, &local);
        drop(store);
        let loaded = db
            .open()
            .load_synced(&scope().account_id, LoadMode::AfterRestart)
            .unwrap();
        assert_eq!(loaded.synced[&scope().key].state, committed.state);
        assert_eq!(
            loaded.synced[&scope().key].state.pending_check(&op),
            Some(&ticket)
        );
    }
}

#[test]
fn legacy_inflight_restores_uncertain_without_fabricating_a_read_deadline() {
    let db = Database::new();
    let mut store = db.open();
    let (row, op, _) = uncertain(&mut store, OperationKind::Update);
    let mut raw = serde_json::to_value(&row.state).unwrap();
    raw["workflow"] = serde_json::json!({"InFlight":op});
    raw.as_object_mut().unwrap().remove("check_ticket");
    raw.as_object_mut().unwrap().remove("check_deadline");
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET format_version=4,state=?1",
            [serde_json::to_string(&raw).unwrap()],
        )
        .unwrap();
    let loaded = store
        .load_synced(&scope().account_id, LoadMode::AfterRestart)
        .unwrap();
    let restored = &loaded.synced[&scope().key];
    assert_eq!(restored.state.workflow(), &Workflow::Uncertain(op.clone()));
    assert_eq!(restored.state.check_retry_at_ms(&op), Some(0));
    assert!(restored.state.pending_check(&op).is_none());
    assert_eq!(restored.revision, row.revision + 1);
}

#[test]
fn corrupted_operation_bound_guard_is_rejected_without_rewriting_durable_content() {
    let db = Database::new();
    let mut store = db.open();
    let (row, op, local) = uncertain(&mut store, OperationKind::Update);
    let mut pending = row.state.clone();
    pending.reserve_check(&op, 0).unwrap();
    saved(&mut store, &row, &pending, &local);
    let mut raw = serde_json::to_value(&pending).unwrap();
    raw["check_ticket"]["operation"]["snapshot"]["file_ids"] = serde_json::json!(["Fwrong"]);
    let raw = serde_json::to_string(&raw).unwrap();
    store
        .0
        .execute("UPDATE composer_draft_sync SET state=?1", [&raw])
        .unwrap();
    assert!(
        store
            .load_synced(&scope().account_id, LoadMode::AfterRestart)
            .is_err()
    );
    let retained: String = store
        .0
        .query_row("SELECT state FROM composer_draft_sync", [], |r| r.get(0))
        .unwrap();
    assert_eq!(retained, raw);
}

#[test]
fn format_five_missing_recovery_fields_is_corruption_not_an_expired_admission() {
    for missing in [
        vec!["check_ticket"],
        vec!["check_deadline"],
        vec!["check_ticket", "check_deadline"],
    ] {
        let db = Database::new();
        let mut store = db.open();
        let (row, op, local) = uncertain(&mut store, OperationKind::Update);
        // Explicit nulls are valid in v5 before a check is admitted.
        let initial = store
            .load_synced(&scope().account_id, LoadMode::Current)
            .unwrap();
        assert_eq!(initial.synced[&scope().key].state, row.state);
        let mut pending = row.state.clone();
        pending.reserve_check(&op, 0).unwrap();
        let committed = saved(&mut store, &row, &pending, &local);
        let mut raw = serde_json::to_value(&pending).unwrap();
        for field in missing {
            raw.as_object_mut().unwrap().remove(field);
        }
        let raw = serde_json::to_string(&raw).unwrap();
        store
            .0
            .execute("UPDATE composer_draft_sync SET state=?1", [&raw])
            .unwrap();
        assert!(
            store
                .load_synced(&scope().account_id, LoadMode::AfterRestart)
                .is_err()
        );
        let (revision, retained): (i64, String) = store
            .0
            .query_row(
                "SELECT storage_revision,state FROM composer_draft_sync",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(retained, raw);
        assert_eq!(revision as u64, committed.revision);
    }
}

#[test]
fn duplicate_recovery_fields_cannot_erase_a_persisted_ticket() {
    for field in ["check_ticket", "check_deadline"] {
        let db = Database::new();
        let mut store = db.open();
        let (row, op, local) = uncertain(&mut store, OperationKind::Update);
        let mut pending = row.state.clone();
        pending.reserve_check(&op, 0).unwrap();
        saved(&mut store, &row, &pending, &local);
        let mut raw = serde_json::to_string(&pending).unwrap();
        assert_eq!(raw.pop(), Some('}'));
        raw.push_str(&format!(",\"{field}\":null}}"));
        store
            .0
            .execute("UPDATE composer_draft_sync SET state=?1", [&raw])
            .unwrap();
        assert!(
            store
                .load_synced(&scope().account_id, LoadMode::AfterRestart)
                .is_err()
        );
        let retained: String = store
            .0
            .query_row("SELECT state FROM composer_draft_sync", [], |r| r.get(0))
            .unwrap();
        assert_eq!(retained, raw);
    }
}
