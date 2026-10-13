use super::tests::{Database, enroll, local, scope, state};
use super::*;
use crate::drafts::sync::{ConsumeKind, OperationKind};

fn detach(store: &mut DraftStore, scope: &Scope, mut row: Stored) -> Stored {
    row.state
        .request_consume(scope, row.state.local_revision(), ConsumeKind::Send)
        .unwrap();
    match store
        .commit_synced(
            scope,
            Some(row.revision),
            &LocalDraft::default(),
            &row.state,
            None,
        )
        .unwrap()
    {
        Commit::Applied(row) => row,
        _ => panic!("detach CAS"),
    }
}

#[test]
fn durable_detach_is_atomic_and_legacy_edits_still_block_consumption_dispatch() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let mut detached = row.state.clone();
    let intent = detached
        .request_consume(&scope, detached.local_revision(), ConsumeKind::Send)
        .unwrap();
    assert!(matches!(
        store
            .commit_synced(
                &scope,
                Some(row.revision + 1),
                &LocalDraft::default(),
                &detached,
                None
            )
            .unwrap(),
        Commit::Stale(_)
    ));
    assert_eq!(store.load().unwrap()[0].1.composer.text, "Original 🚀");
    let row = detach(&mut store, &scope, row);
    assert!(store.load().unwrap().is_empty());
    assert_eq!(row.state.consumption().unwrap().snapshot, intent.snapshot);
    store
        .save_legacy(&vec![(scope.key.clone(), local("racing legacy edit"))])
        .unwrap();
    assert!(matches!(
        store.prepare_synced(&scope, row.revision).unwrap(),
        Prepare::Blocked(_)
    ));
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    let blocked = &loaded.blocked_local[&scope.key];
    assert_eq!(blocked.draft.composer.text, "racing legacy edit");
    let Commit::Applied(settled) = store
        .commit_synced(
            &scope,
            Some(row.revision),
            &LocalDraft::default(),
            &row.state,
            Some(&blocked.token),
        )
        .unwrap()
    else {
        panic!()
    };
    let Prepare::Prepared { stored, operation } =
        store.prepare_synced(&scope, settled.revision).unwrap()
    else {
        panic!()
    };
    assert_eq!(operation.kind, OperationKind::Send);
    assert!(operation.base.is_none());
    assert!(matches!(
        store.prepare_synced(&scope, stored.revision).unwrap(),
        Prepare::Blocked(_)
    ));
    drop(store);
    let mut store = db.open();
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert!(loaded.local.is_empty());
    let restored = &loaded.synced[&scope.key];
    assert_eq!(restored.state.pending(), Some(&operation));
    assert!(restored.state.consumption().is_some());
    assert!(matches!(restored.state.workflow(), Workflow::Uncertain(_)));
    assert!(matches!(
        store.prepare_synced(&scope, restored.revision).unwrap(),
        Prepare::Blocked(_)
    ));
}

#[test]
fn delayed_consume_success_cas_cannot_erase_newer_durable_typing() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let enrolled = enroll(&mut store, &scope);
    let row = detach(&mut store, &scope, enrolled);
    let Prepare::Prepared { stored, operation } =
        store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!()
    };
    let mut next = stored.state.clone();
    next.edit(&scope, state(&scope, "next text").desired().cloned());
    let Commit::Applied(newer) = store
        .commit_synced(
            &scope,
            Some(stored.revision),
            &local("next text"),
            &next,
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    let mut stale = stored.state;
    assert!(stale.confirm_sent(
        &scope,
        &operation.id,
        &operation.client_msg_id,
        None,
        "11.000001"
    ));
    assert!(matches!(
        store
            .commit_synced(
                &scope,
                Some(stored.revision),
                &LocalDraft::default(),
                &stale,
                None
            )
            .unwrap(),
        Commit::Stale(_)
    ));
    let mut current = newer.state;
    assert!(current.confirm_sent(
        &scope,
        &operation.id,
        &operation.client_msg_id,
        None,
        "11.000001"
    ));
    let Commit::Applied(final_row) = store
        .commit_synced(
            &scope,
            Some(newer.revision),
            &local("next text"),
            &current,
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    drop(store);
    let mut store = db.open();
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert_eq!(loaded.local[0].1.composer.text, "next text");
    let restored = &loaded.synced[&scope.key];
    assert!(restored.state.consumption().is_none());
    assert_eq!(restored.revision, final_row.revision);
    assert_ne!(restored.state.client_msg_id(), operation.client_msg_id);
    let Prepare::Prepared {
        operation: next, ..
    } = store.prepare_synced(&scope, restored.revision).unwrap()
    else {
        panic!()
    };
    assert_eq!(next.kind, OperationKind::Create);
    assert_eq!(next.snapshot, state(&scope, "next text").desired().cloned());
}

#[test]
fn legacy_rows_read_without_consumption_and_upgrade_before_detaching() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let mut body = serde_json::to_value(&row.state).unwrap();
    body.as_object_mut().unwrap().remove("consumption");
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET format_version=1,state=?1",
            [body.to_string()],
        )
        .unwrap();
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert!(loaded.synced[&scope.key].state.consumption().is_none());
    detach(&mut store, &scope, loaded.synced[&scope.key].clone());
    let version: i64 = store
        .0
        .query_row(
            "SELECT format_version FROM composer_draft_sync",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, FORMAT);
}

#[test]
fn invalid_consumption_scope_is_rejected_before_clearing_local_content() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let mut detached = row.state.clone();
    detached
        .request_consume(&scope, detached.local_revision(), ConsumeKind::Send)
        .unwrap();
    let mut body = serde_json::to_value(detached).unwrap();
    body["consumption"]["scope"]["account_id"] = "00000000-0000-4000-8000-000000000099".into();
    let invalid: DraftSync = serde_json::from_value(body).unwrap();
    assert!(
        store
            .commit_synced(
                &scope,
                Some(row.revision),
                &LocalDraft::default(),
                &invalid,
                None
            )
            .is_err()
    );
    assert_eq!(store.load().unwrap()[0].1.composer.text, "Original 🚀");
}

#[test]
fn check_deadline_is_durable_monotonic_and_bound_to_exact_uncertain_send() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let row = detach(&mut store, &scope, row);
    let Prepare::Prepared {
        mut stored,
        operation,
    } = store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!()
    };
    assert!(!stored.state.defer_check(&operation, 5000));
    stored.state.mark_uncertain(&scope, &operation.id);
    assert!(stored.state.defer_check(&operation, 5000));
    assert!(stored.state.defer_check(&operation, 1000));
    let mut wrong = operation.clone();
    wrong.scope.key.channel = "other".into();
    assert!(!stored.state.defer_check(&wrong, 9999));
    wrong = operation.clone();
    wrong.id = uuid::Uuid::new_v4().to_string();
    assert!(!stored.state.defer_check(&wrong, 9999));
    assert!(matches!(
        store
            .commit_synced(
                &scope,
                Some(stored.revision),
                &LocalDraft::default(),
                &stored.state,
                None
            )
            .unwrap(),
        Commit::Applied(_)
    ));
    drop(store);
    let mut store = db.open();
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    let mut state = loaded.synced[&scope.key].state.clone();
    assert_eq!(state.check_retry_at_ms(&operation), Some(5000));
    assert!(state.confirm_sent(
        &scope,
        &operation.id,
        &operation.client_msg_id,
        None,
        "1700000000.000001"
    ));
    assert_eq!(state.check_retry_at_ms(&operation), None);
    assert!(!state.defer_check(&operation, 9999));
}

#[test]
fn legacy_v2_without_check_metadata_loads_without_inventing_a_deadline() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let mut value = serde_json::to_value(&row.state).unwrap();
    value.as_object_mut().unwrap().remove("check_deadline");
    let old: DraftSync = serde_json::from_value(value).unwrap();
    assert_eq!(old, row.state);
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET format_version=2,state=?1",
            [serde_json::to_string(&old).unwrap()],
        )
        .unwrap();
    assert_eq!(
        store
            .load_synced(&scope.account_id, LoadMode::Current)
            .unwrap()
            .synced[&scope.key]
            .state,
        old
    );
}
