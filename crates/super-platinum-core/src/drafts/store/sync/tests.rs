use super::*;

pub(in crate::drafts::store) struct Database {
    pub path: std::path::PathBuf,
}
impl Database {
    pub fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "platinum-draft-sync-{}.sqlite",
                uuid::Uuid::new_v4()
            )),
        }
    }
    pub fn open(&self) -> DraftStore {
        DraftStore::open(&self.path).unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
pub(in crate::drafts::store) fn scope() -> Scope {
    Scope {
        account_id: "00000000-0000-4000-8000-000000000001".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("1.2".into()),
        },
    }
}
pub(in crate::drafts::store) fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: crate::ComposerState::with_text(text),
        ..Default::default()
    }
}
pub(in crate::drafts::store) fn state(scope: &Scope, text: &str) -> DraftSync {
    let mut state = DraftSync::new(scope.clone());
    state.edit(scope,Some(crate::drafts::sync::Snapshot {
        blocks:vec![serde_json::json!({"type":"unknown_future_block","text":text,"opaque":{"preserve":true}})],
        file_ids:vec!["F1".into()],
        destinations:vec![serde_json::json!({"channel_id":scope.key.channel,"thread_ts":scope.key.thread})],
    }));
    state
}
pub(in crate::drafts::store) fn enroll(store: &mut DraftStore, scope: &Scope) -> Stored {
    store
        .save([(scope.key.clone(), local("Original 🚀"))])
        .unwrap();
    let Commit::Applied(stored) = store
        .commit_synced(
            scope,
            None,
            &local("Original 🚀"),
            &state(scope, "Original 🚀"),
            None,
        )
        .unwrap()
    else {
        panic!("enroll")
    };
    stored
}

#[test]
fn legacy_migration_preserves_content_and_enrollment_rejects_stale_local_snapshot() {
    let db = Database::new();
    // Create precisely the old schema; opening performs additive migration.
    let conn = Connection::open(&db.path).unwrap();
    conn.execute_batch(
        "CREATE TABLE composer_drafts(destination TEXT PRIMARY KEY,content TEXT NOT NULL)",
    )
    .unwrap();
    let scope = scope();
    let mut old = local("Legacy 日本語 🚀");
    old.composer.set_selection(1, 4);
    conn.execute(
        "INSERT INTO composer_drafts VALUES(?1,?2)",
        params![
            serde_json::to_string(&scope.key).unwrap(),
            serde_json::to_string(&old).unwrap()
        ],
    )
    .unwrap();
    drop(conn);
    let mut store = db.open();
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert_eq!(loaded.local[0].1.composer, old.composer);
    assert!(loaded.synced.is_empty());
    assert!(matches!(
        store
            .commit_synced(&scope, None, &local("stale"), &state(&scope, "stale"), None)
            .unwrap(),
        Commit::Stale(None)
    ));
    assert_eq!(store.load().unwrap()[0].1.composer, old.composer);
    assert!(matches!(
        store
            .commit_synced(&scope, None, &old, &state(&scope, "Legacy 日本語 🚀"), None)
            .unwrap(),
        Commit::Applied(_)
    ));
}

#[test]
fn failed_sync_write_rolls_back_visible_draft_and_revision() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    store.0.execute_batch("CREATE TRIGGER reject_sync BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'test disk failure'); END;").unwrap();
    let mut updated = row.state.clone();
    updated.edit(&scope, state(&scope, "New").desired().cloned());
    assert!(
        store
            .commit_synced(&scope, Some(row.revision), &local("New"), &updated, None)
            .is_err()
    );
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(loaded.local[0].1.composer.text, "Original 🚀");
    assert_eq!(loaded.synced[&scope.key].revision, row.revision);
    assert_eq!(loaded.synced[&scope.key].state, row.state);
}

#[test]
fn lost_prepare_reply_recovers_exact_identity_without_a_second_create() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let Prepare::Prepared { stored, operation } =
        store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!("prepare")
    };
    assert!(matches!(
        store.prepare_synced(&scope, row.revision).unwrap(),
        Prepare::Stale(_)
    ));
    assert!(matches!(
        store.prepare_synced(&scope, stored.revision).unwrap(),
        Prepare::Blocked(_)
    ));
    drop(store);
    let mut store = db.open();
    let restored = store
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    let recovered = &restored.synced[&scope.key];
    assert_eq!(recovered.state.client_msg_id(), row.state.client_msg_id());
    assert_eq!(recovered.state.pending(), Some(&operation));
    assert!(matches!(recovered.state.workflow(), Workflow::Uncertain(_)));
    assert!(recovered.revision > stored.revision);
    assert!(matches!(
        store.prepare_synced(&scope, recovered.revision).unwrap(),
        Prepare::Blocked(_)
    ));
    drop(store);
    assert_eq!(
        db.open()
            .load_synced(&scope.account_id, LoadMode::Current)
            .unwrap()
            .synced[&scope.key]
            .state,
        recovered.state
    );
}

#[test]
fn empty_composer_keeps_pending_delete_and_raw_snapshot_across_reopen() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let Prepare::Prepared { stored, operation } =
        store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!()
    };
    let mut acknowledged = stored.state.clone();
    assert!(acknowledged.confirm(
        &operation.id,
        crate::drafts::sync::RemoteDraft {
            scope: scope.clone(),
            id: "Dr1".into(),
            client_msg_id: operation.client_msg_id.clone(),
            last_updated_ts: "10.0".into(),
            snapshot: operation.snapshot.unwrap()
        }
    ));
    acknowledged.edit(&scope, None);
    let Commit::Applied(cleared) = store
        .commit_synced(
            &scope,
            Some(stored.revision),
            &LocalDraft::default(),
            &acknowledged,
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(store.load().unwrap().is_empty());
    let Prepare::Prepared { operation, .. } =
        store.prepare_synced(&scope, cleared.revision).unwrap()
    else {
        panic!()
    };
    assert_eq!(operation.kind, crate::drafts::sync::OperationKind::Delete);
    drop(store);
    let loaded = db
        .open()
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert!(loaded.local.is_empty());
    let synced = &loaded.synced[&scope.key].state;
    assert!(synced.tombstone());
    assert_eq!(synced.pending(), Some(&operation));
    assert_eq!(synced.acknowledged().unwrap().snapshot.file_ids, ["F1"]);
    assert!(
        synced.acknowledged().unwrap().snapshot.blocks[0]["opaque"]["preserve"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn old_completion_cannot_overwrite_a_newer_durable_edit() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let Prepare::Prepared { stored, operation } =
        store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!()
    };
    let mut edited = stored.state.clone();
    edited.edit(&scope, state(&scope, "Newer").desired().cloned());
    let Commit::Applied(latest) = store
        .commit_synced(
            &scope,
            Some(stored.revision),
            &local("Newer"),
            &edited,
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    let mut old = stored.state;
    old.mark_uncertain(&scope, &operation.id);
    assert!(matches!(
        store
            .commit_synced(
                &scope,
                Some(stored.revision),
                &local("Original 🚀"),
                &old,
                None
            )
            .unwrap(),
        Commit::Stale(Some(_))
    ));
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(loaded.local[0].1.composer.text, "Newer");
    assert_eq!(loaded.synced[&scope.key].state, latest.state);
}

#[test]
fn two_connections_claim_one_operation_and_account_scope_cannot_cross() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = db.path.clone();
            let scope = scope.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut store = DraftStore::open(path).unwrap();
                barrier.wait();
                store.prepare_synced(&scope, row.revision).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Prepare::Prepared { .. }))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Prepare::Stale(_)))
            .count(),
        1
    );
    assert!(
        store
            .load_synced("00000000-0000-4000-8000-000000000002", LoadMode::Current)
            .is_err()
    );
    let mut other = scope.clone();
    other.key.team = "T2".into();
    assert!(
        store
            .commit_synced(&other, None, &LocalDraft::default(), &row.state, None)
            .is_err()
    );
    assert!(matches!(
        store.prepare_synced(&other, 1).unwrap(),
        Prepare::Stale(None)
    ));
}

#[test]
fn legacy_writes_are_preserved_separately_and_settlement_cannot_remove_newer_edit() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    assert!(
        store
            .save([(scope.key.clone(), local("forbidden"))])
            .is_err()
    );
    store
        .save_legacy(&vec![(scope.key.clone(), local("Blocked one"))])
        .unwrap();
    let one = store
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap()
        .blocked_local
        .remove(&scope.key)
        .unwrap();
    store
        .save_legacy(&vec![(scope.key.clone(), local("Blocked two"))])
        .unwrap();
    let Commit::Applied(next) = store
        .commit_synced(
            &scope,
            Some(row.revision),
            &local("Original 🚀"),
            &row.state,
            Some(&one.token),
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
    assert_eq!(loaded.local[0].1.composer.text, "Original 🚀");
    let two = &loaded.blocked_local[&scope.key];
    assert_eq!(two.draft.composer.text, "Blocked two");
    assert!(matches!(
        store.prepare_synced(&scope, next.revision).unwrap(),
        Prepare::Blocked(_)
    ));
    let Commit::Applied(settled) = store
        .commit_synced(
            &scope,
            Some(next.revision),
            &local("Original 🚀"),
            &next.state,
            Some(&two.token),
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(
        store
            .load_synced(&scope.account_id, LoadMode::Current)
            .unwrap()
            .blocked_local
            .is_empty()
    );
    assert!(matches!(
        store.prepare_synced(&scope, settled.revision).unwrap(),
        Prepare::Prepared { .. }
    ));
}

#[test]
fn unsupported_records_fail_closed_and_cannot_be_overwritten() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    store
        .0
        .execute("UPDATE composer_draft_sync SET format_version=999", [])
        .unwrap();
    assert!(
        store
            .load_synced(&scope.account_id, LoadMode::AfterRestart)
            .is_err()
    );
    assert!(
        store
            .commit_synced(
                &scope,
                Some(row.revision),
                &LocalDraft::default(),
                &row.state,
                None
            )
            .is_err()
    );
    assert!(store.prepare_synced(&scope, row.revision).is_err());
    assert_eq!(store.load().unwrap()[0].1.composer.text, "Original 🚀");
}
