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
    let local = if let OperationKind::Schedule { post_at } = kind {
        state
            .request_consume(
                &scope,
                state.local_revision(),
                ConsumeKind::Schedule { post_at },
            )
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
fn schedule_failed_completion_keeps_pending_ticket_and_retry_after_across_restart() {
    let db = Database::new();
    let mut store = db.open();
    let (row, op, local) = uncertain(&mut store, OperationKind::Schedule { post_at: 500 });
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
fn schedule_pending_ticket_and_positive_completion_round_trip_with_newer_local() {
    let db = Database::new();
    let mut store = db.open();
    let (row, op, local) = uncertain(&mut store, OperationKind::Schedule { post_at: 500 });
    let mut state = row.state.clone();
    let ticket = state.reserve_check(&op, 900_000).unwrap();
    let pending = saved(&mut store, &row, &state, &local);
    drop(store);
    let mut store = db.open();
    let loaded = store
        .load_synced(&scope().account_id, LoadMode::AfterRestart)
        .unwrap();
    let retained = &loaded.synced[&scope().key];
    assert_eq!(retained.state, pending.state);
    assert_eq!(retained.state.pending_check(&op), Some(&ticket));
    let mut complete = retained.state.clone();
    let newer = super::tests::local("Typing after Schedule");
    complete.edit(
        &scope(),
        Some(codec::encode(&scope().key, &newer.composer.text).unwrap()),
    );
    assert!(complete.finish_check(&ticket, 0));
    let mut remote = op.base.clone().unwrap();
    remote.snapshot = op.snapshot.clone().unwrap();
    remote.last_updated_ts = "101.000001".into();
    assert!(complete.confirm_scheduled(&op.id, remote, 500));
    let done = saved(&mut store, retained, &complete, &newer);
    drop(store);
    let restored = db
        .open()
        .load_synced(&scope().account_id, LoadMode::AfterRestart)
        .unwrap();
    assert_eq!(restored.synced[&scope().key].state, done.state);
    assert_eq!(restored.synced[&scope().key].revision, done.revision);
    let (_, restored_local) = restored
        .local
        .iter()
        .find(|(key, _)| key == &scope().key)
        .expect("newer local draft retained for the exact scope");
    assert_eq!(
        serde_json::to_value(restored_local).unwrap(),
        serde_json::to_value(&newer).unwrap()
    );
    assert!(done.state.consumption().is_none());
    assert_eq!(done.state.check_retry_at_ms(&op), None);
}
