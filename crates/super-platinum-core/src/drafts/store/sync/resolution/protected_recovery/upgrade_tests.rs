//! Pure temporary-database M01-M08 regressions. Authored, not run by the author.
use super::*;
use crate::drafts::store::{sync::tests::local, worker::UpgradeTestWorker};
use crate::drafts::sync::{ConsumeKind, RemoteDraft, Snapshot};
use serde_json::json;
use snapshot::Cell;

fn scope(owner: &Owned, n: usize) -> Scope {
    Scope {
        account_id: owner.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: format!("C{n}"),
            thread: (n % 2 == 1).then(|| "100.000001".into()),
        },
    }
}
fn run(worker: &UpgradeTestWorker, owner: &Arc<Owned>, action: Action) -> Result<Reply, AppError> {
    let (command, rx) = Command::new(owner.clone(), action);
    worker.run(owner, command);
    rx.recv().unwrap()
}
fn inspect(worker: &UpgradeTestWorker, owner: &Arc<Owned>) -> Arc<Review> {
    let Reply::Inspected(v) = run(worker, owner, Action::Inspect).unwrap() else {
        panic!("inspection")
    };
    v
}
fn commit(worker: &UpgradeTestWorker, owner: &Arc<Owned>, review: Arc<Review>) -> Receipt {
    let Reply::Committed(v) = run(worker, owner, Action::Commit(review)).unwrap() else {
        panic!("commit")
    };
    v
}
fn image(owner: &Owned) -> Image {
    Image::read(
        &Connection::open_with_flags(owner.path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap(),
    )
    .unwrap()
}
fn seed(owner: &Owned, n: usize, variant: usize) -> crate::drafts::store::sync::Stored {
    let mut store = DraftStore::open(owner.path()).unwrap();
    let scope = scope(owner, n);
    let mut destination = json!({"channel_id":scope.key.channel,"future":{"keep":true}});
    if let Some(thread) = &scope.key.thread {
        destination["thread_ts"] = json!(thread);
    }
    let snapshot = Snapshot {
        blocks: vec![
            json!({"type":"future_rich","text":"日本語 🚀 é\nsecond line","opaque":{"unknown":true,"bytes":[0,255]}}),
        ],
        file_ids: vec!["F1".into()],
        destinations: vec![destination],
    };
    let base = RemoteDraft {
        scope: scope.clone(),
        id: format!("D{n}"),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot,
    };
    let mut state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    let mut canonical = local("日本語 🚀 é\nsecond line");
    canonical.attachments.push(serde_json::from_value(json!({"id":1,"path":owner.path().with_extension("not-read-attachment"),"name":"日本語.txt","bytes":17})).unwrap());
    canonical.composer.reply_broadcast = scope.key.thread.is_some();
    if variant == 3 {
        state
            .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
            .unwrap();
        canonical = LocalDraft::default();
    } else if variant == 5 {
        state.edit(&scope, None);
        canonical = LocalDraft::default();
    } else if variant != 0 {
        let mut changed = base.snapshot.clone();
        changed.blocks.push(json!({"opaque_leaf":"newer"}));
        state.edit(&scope, Some(changed));
    }
    store
        .save([(scope.key.clone(), canonical.clone())])
        .unwrap();
    let crate::drafts::store::sync::Commit::Applied(mut stored) = store
        .commit_synced(&scope, None, &canonical, &state, None)
        .unwrap()
    else {
        panic!("fixture enrollment must commit the exact canonical draft")
    };
    if variant != 0 {
        // Dispatch identities must originate in the real durable prepare API.
        // Never manufacture a new InFlight row with commit_synced or raw SQL.
        let crate::drafts::store::sync::Prepare::Prepared {
            stored: prepared,
            operation: op,
        } = store.prepare_synced(&scope, stored.revision).unwrap()
        else {
            panic!("fixture operation must be durably prepared")
        };
        stored = prepared;
        state = stored.state.clone();
        if matches!(variant, 2 | 6 | 7 | 8) {
            assert!(state.mark_uncertain(&scope, &op.id));
        }
        if variant == 3 {
            assert!(state.reject(&scope, &op.id));
        }
        if variant == 4 {
            let mut remote = base;
            remote.last_updated_ts = "20.000001".into();
            remote
                .snapshot
                .blocks
                .push(json!({"unsupported_remote":true}));
            assert!(state.observe_conflict(remote));
        }
        if matches!(variant, 6 | 7 | 8) {
            let ticket = state.reserve_check(&op, 1_000).unwrap();
            assert!(state.defer_check(&op, if variant == 6 { 90_000 } else { 180_000 }));
            if variant == 7 {
                assert!(state.finish_check(&ticket, 120_000));
            }
            if variant == 8 {
                let mut remote = op.base.clone().unwrap();
                remote.snapshot = op.snapshot.clone().unwrap();
                remote.last_updated_ts = "20.000001".into();
                assert!(state.confirm(&op.id, remote)); // retained ticket is now historical
            }
        }
    }
    if state != stored.state {
        let crate::drafts::store::sync::Commit::Applied(next) = store
            .commit_synced(&scope, Some(stored.revision), &canonical, &state, None)
            .unwrap()
        else {
            panic!("fixture outcome must commit against its prepared revision")
        };
        stored = next;
    }
    // Only add whitespace to the actual committed wire record. This does not
    // introduce an operation, change state, or bypass preparation admission.
    let committed_raw: String = store
        .0
        .query_row(
            "SELECT state FROM composer_draft_sync WHERE destination=?1",
            [serde_json::to_string(&scope.key).unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<DraftSync>(&committed_raw).unwrap(),
        stored.state
    );
    // Exact original whitespace survives in durable migration history.
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET state=?1 WHERE destination=?2",
            params![
                format!("  {committed_raw}\n"),
                serde_json::to_string(&scope.key).unwrap()
            ],
        )
        .unwrap();
    stored
}
#[test]
fn m01_missing_inspect_and_cancel_create_no_database_or_schema() {
    let owner = Owned::fresh().unwrap();
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    assert_eq!(review.class(), Class::Missing);
    assert!(!owner.path().exists());
    drop(review);
    assert!(!owner.path().exists());
    let review = inspect(&worker, &owner);
    let receipt = commit(&worker, &owner, review.clone());
    assert_eq!(receipt.imported, 0);
    let before = image(&owner);
    let again = commit(&worker, &owner, review);
    assert!(again.repeated);
    assert_eq!(again.id, receipt.id);
    assert_eq!(image(&owner), before);
}
#[test]
fn m01_existing_inspect_is_byte_readonly_and_does_not_install_uploads_or_owner() {
    let owner = Owned::fresh().unwrap();
    let store = DraftStore::open(owner.path()).unwrap();
    store.0.execute_batch("DROP TABLE upload_receipts; DROP TABLE upload_journal_identity; DELETE FROM sqlite_sequence;").unwrap();
    // sqlite_sequence itself is a known SQLite artifact only with upload schema;
    // construct the exact no-upload ordinary schema on a fresh shadow image.
    let mut raw = Image::read(&store.0).unwrap();
    raw.schema.retain(|o| o.name != "sqlite_sequence");
    raw.tables.remove("sqlite_sequence");
    drop(store);
    // File recreation here is fixture setup, never the migration opener.
    std::fs::remove_file(owner.path()).unwrap();
    let conn = Connection::open(owner.path()).unwrap();
    for o in &raw.schema {
        if let Some(sql) = &o.sql {
            conn.execute_batch(sql).unwrap();
        }
    }
    drop(conn);
    let before = std::fs::read(owner.path()).unwrap();
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    assert_eq!(review.class(), Class::OrdinaryLocal);
    assert_eq!(std::fs::read(owner.path()).unwrap(), before);
    assert!(
        image(&owner)
            .table("composer_draft_owner")
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(!image(&owner).tables.contains_key("upload_receipts"));
}
#[test]
fn m02_unsupported_mixed_unknown_and_missing_fields_refuse_without_mutation() {
    for case in 0..10 {
        let owner = Owned::fresh().unwrap();
        seed(&owner, 0, 2);
        seed(&owner, 1, 0);
        let conn = Connection::open(owner.path()).unwrap();
        match case {
            0..=5 => {
                conn.execute("UPDATE composer_draft_sync SET format_version=?1 WHERE destination=(SELECT MIN(destination) FROM composer_draft_sync)",[match case {0=>1,1=>2,2=>3,3=>4,4=>6,_=>99}]).unwrap();
            }
            6 => {
                conn.execute(
                    "UPDATE composer_draft_owner SET account=?1",
                    [uuid::Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            7 => {
                conn.execute(
                    "UPDATE composer_draft_sync SET state=json_set(state,'$.unknown_authority',1)",
                    [],
                )
                .unwrap();
            }
            8 => {
                conn.execute(
                    "UPDATE composer_draft_sync SET state=json_remove(state,'$.check_ticket')",
                    [],
                )
                .unwrap();
            }
            _ => {
                conn.execute_batch("CREATE TABLE composer_draft_continuity_format(singleton INTEGER PRIMARY KEY,version INTEGER);INSERT INTO composer_draft_continuity_format VALUES(1,7);").unwrap();
            }
        }
        drop(conn);
        let before = std::fs::read(owner.path()).unwrap();
        let worker = UpgradeTestWorker::new();
        assert!(
            run(&worker, &owner, Action::Inspect).is_err(),
            "case {case}"
        );
        assert_eq!(std::fs::read(owner.path()).unwrap(), before);
    }
}
#[test]
fn m02_duplicate_or_noncanonical_destinations_and_duplicate_json_keys_refuse() {
    for case in 0..3 {
        let owner = Owned::fresh().unwrap();
        seed(&owner, 0, 0);
        let conn = Connection::open(owner.path()).unwrap();
        if case == 0 {
            conn.execute(
                "UPDATE composer_draft_sync SET destination=' '||destination",
                [],
            )
            .unwrap();
        }
        if case == 1 {
            conn.execute(
                "INSERT INTO composer_drafts SELECT ' '||destination,content FROM composer_drafts",
                [],
            )
            .unwrap();
        }
        if case == 2 {
            let raw: String = conn
                .query_row("SELECT state FROM composer_draft_sync", [], |r| r.get(0))
                .unwrap();
            let raw = raw.trim();
            let changed = format!("{{\"check_ticket\":null,{}", &raw[1..]);
            conn.execute("UPDATE composer_draft_sync SET state=?1", [changed])
                .unwrap();
        }
        drop(conn);
        let before = image(&owner);
        assert!(run(&UpgradeTestWorker::new(), &owner, Action::Inspect).is_err());
        assert_eq!(image(&owner), before);
    }
}
#[test]
fn m02_exact_owner_arc_and_worker_exclusivity_are_not_caller_assertions() {
    let owner = Owned::fresh().unwrap();
    let first = UpgradeTestWorker::new();
    let review = inspect(&first, &owner);
    assert!(run(&UpgradeTestWorker::new(), &owner, Action::Inspect).is_err());
    let other = Owned::fresh().unwrap();
    assert!(run(&first, &other, Action::Commit(review)).is_err());
    assert!(!owner.path().exists());
    assert!(!other.path().exists());
}
#[test]
fn m03_m04_full_workflow_corpus_and_raw_auxiliary_rows_are_preserved() {
    let owner = Owned::fresh().unwrap();
    let expected = (0..6).map(|n| seed(&owner, n, n)).collect::<Vec<_>>();
    let mut store = DraftStore::open(owner.path()).unwrap();
    let key = serde_json::to_string(&scope(&owner, 0).key).unwrap();
    store
        .0
        .execute(
            "INSERT INTO composer_drafts VALUES(?1,?2)",
            params![
                serde_json::to_string(&scope(&owner, 3).key).unwrap(),
                serde_json::to_string(&LocalDraft::default()).unwrap()
            ],
        )
        .unwrap();
    let raw = format!(
        " {} ",
        serde_json::to_string(&local("retained 日本語 🚀")).unwrap()
    );
    let writer = uuid::Uuid::new_v4().to_string();
    store
        .0
        .execute(
            "INSERT INTO composer_draft_blocked VALUES(?1,?2,?3)",
            params![key, uuid::Uuid::new_v4().to_string(), raw],
        )
        .unwrap();
    store
        .0
        .execute(
            "INSERT INTO composer_draft_alternates VALUES(?1,?2,9,?3)",
            params![key, writer, raw],
        )
        .unwrap();
    store
        .0
        .execute(
            "INSERT INTO composer_draft_reviewed_writers VALUES(?1,?2,8)",
            params![key, writer],
        )
        .unwrap();
    let mut upload = crate::drafts::store::uploads::tests::draft();
    upload.account_id = owner.account().into();
    store.reserve_upload(&upload).unwrap();
    crate::slash_commands::journal::install(&mut store.0, owner.account()).unwrap();
    drop(store);
    let before = image(&owner);
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    assert_eq!(review.class(), Class::Legacy5);
    assert_eq!(commit(&worker, &owner, review.clone()).imported, 6);
    let after = image(&owner);
    before.preservation(&after).unwrap();
    schema::validate_import(
        &before,
        after.table("composer_draft_sync").unwrap(),
        owner.account(),
    )
    .unwrap();
    let conn = Connection::open(owner.path()).unwrap();
    for (n, original) in expected.iter().enumerate() {
        let row = storage::read_row(&conn, &scope(&owner, n)).unwrap();
        assert_eq!(&row.stored.state, &original.state);
        assert_eq!(
            row.stored.revision,
            original.revision.checked_add(1).unwrap()
        );
        assert_eq!(row.ownership.verified_epoch, 0);
    }
    let clocks = after.table("composer_draft_local_clock").unwrap();
    assert_eq!(
        clocks.rows.len(),
        before.table("composer_drafts").unwrap().rows.len()
    );
    assert!(
        clocks
            .rows
            .iter()
            .all(|r| r[1..] == [Cell::Integer(1), Cell::Null, Cell::Null, Cell::Integer(0)])
    );
    assert!(
        after
            .table("composer_draft_writer_lineage")
            .unwrap()
            .rows
            .is_empty()
    );
    let receipt = commit(&worker, &owner, review);
    assert!(receipt.repeated);
    assert_eq!(image(&owner), after);
}
#[test]
fn m05_pending_historical_and_resolved_deadlines_import_actual_attempts_and_maximum() {
    let owner = Owned::fresh().unwrap();
    seed(&owner, 0, 6);
    seed(&owner, 1, 7);
    seed(&owner, 2, 8);
    let before = image(&owner);
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    commit(&worker, &owner, review);
    let store = DraftStore::open(owner.path()).unwrap();
    let attempts = recovery::attempts(&store.0).unwrap();
    assert_eq!(attempts.len(), 2);
    assert!(
        attempts
            .iter()
            .any(|(_, a)| a.state == recovery::State::Pending)
    );
    assert!(
        attempts
            .iter()
            .any(|(_, a)| a.state == recovery::State::Historical)
    );
    assert_eq!(recovery::deadline(&store.0, "T1").unwrap(), 180_000);
    assert_eq!(adapter_schema::known(&store.0, "T1").unwrap(), 180_000);
    schema::validate_import(
        &before,
        Image::read(&store.0)
            .unwrap()
            .table("composer_draft_sync")
            .unwrap(),
        owner.account(),
    )
    .unwrap();
    assert!(
        Image::read(&store.0)
            .unwrap()
            .table("composer_draft_execution_result")
            .unwrap()
            .rows
            .is_empty()
    );
}
#[test]
fn m06_each_transaction_fault_and_real_commit_failure_roll_back_exact_raw_inventory() {
    for fault in [
        Step::Opened,
        Step::Ordinary,
        Step::Imported,
        Step::Adapter,
        Step::History,
        Step::BeforeCommit,
        Step::Commit,
    ] {
        let owner = Owned::fresh().unwrap();
        seed(&owner, 0, 6);
        let before = image(&owner);
        let worker = UpgradeTestWorker::new();
        let review = inspect(&worker, &owner);
        let (mut command, rx) = Command::new(owner.clone(), Action::Commit(review));
        command.fault = Some(fault);
        worker.run(&owner, command);
        assert!(rx.recv().unwrap().is_err(), "{fault:?}");
        assert_eq!(image(&owner), before, "{fault:?}");
        let conn = Connection::open(owner.path()).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
}
#[test]
fn m06_missing_failure_removes_only_exclusively_created_file() {
    for fault in [
        Step::Ordinary,
        Step::Imported,
        Step::Adapter,
        Step::History,
        Step::BeforeCommit,
        Step::Commit,
    ] {
        let owner = Owned::fresh().unwrap();
        let worker = UpgradeTestWorker::new();
        let review = inspect(&worker, &owner);
        let (mut command, rx) = Command::new(owner.clone(), Action::Commit(review));
        command.fault = Some(fault);
        worker.run(&owner, command);
        assert!(rx.recv().unwrap().is_err());
        assert!(!owner.path().exists());
        owner.check().unwrap();
    }
}
#[test]
fn m06_stale_content_schema_and_competing_create_refuse_without_repair() {
    for case in 0..3 {
        let owner = Owned::fresh().unwrap();
        if case != 2 {
            seed(&owner, 0, 0);
        }
        let worker = UpgradeTestWorker::new();
        let review = inspect(&worker, &owner);
        let conn = Connection::open(owner.path()).unwrap();
        match case {
            0 => {
                conn.execute(
                    "UPDATE composer_drafts SET content=?1",
                    [serde_json::to_string(&local("newer")).unwrap()],
                )
                .unwrap();
            }
            1 => {
                conn.execute_batch("CREATE TABLE unrelated(value BLOB)")
                    .unwrap();
            }
            _ => {
                conn.execute_batch("CREATE TABLE competitor(value BLOB)")
                    .unwrap();
            }
        }
        drop(conn);
        let before = image(&owner);
        assert!(run(&worker, &owner, Action::Commit(review)).is_err());
        assert_eq!(image(&owner), before);
    }
}
#[test]
fn m07_other_connection_write_lock_refuses_then_requires_new_review() {
    let owner = Owned::fresh().unwrap();
    seed(&owner, 0, 0);
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    let other = Connection::open(owner.path()).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let before = image(&owner);
    assert!(run(&worker, &owner, Action::Commit(review)).is_err());
    assert_eq!(image(&owner), before);
    other.execute_batch("ROLLBACK").unwrap();
    let review = inspect(&worker, &owner);
    assert_eq!(commit(&worker, &owner, review).imported, 1);
}
#[test]
fn m08_migration_current_is_exact_restart_is_separate_and_no_dispatch_receipt() {
    let owner = Owned::fresh().unwrap();
    let initial = seed(&owner, 0, 1);
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    commit(&worker, &owner, review);
    let store = DraftStore::open(owner.path()).unwrap();
    let scope = scope(&owner, 0);
    let before = Image::read(&store.0).unwrap();
    let loaded =
        loading::load(&store, owner.account(), LoadMode::Current, &BTreeMap::new()).unwrap();
    assert_eq!(loaded.managed[&scope.key].row().state(), &initial.state);
    let migrated_revision = initial.revision.checked_add(1).unwrap();
    assert_eq!(
        loaded.managed[&scope.key].row().revision(),
        migrated_revision
    );
    assert_eq!(Image::read(&store.0).unwrap(), before);
    let loaded = loading::load(
        &store,
        owner.account(),
        LoadMode::AfterRestart,
        &BTreeMap::new(),
    )
    .unwrap();
    let expected = initial.state.restore_after_restart();
    assert_eq!(loaded.managed[&scope.key].row().state(), &expected);
    assert_eq!(
        loaded.managed[&scope.key].row().revision(),
        migrated_revision.checked_add(1).unwrap()
    );
    let once = Image::read(&store.0).unwrap();
    let current =
        loading::load(&store, owner.account(), LoadMode::Current, &BTreeMap::new()).unwrap();
    assert_eq!(current.managed[&scope.key].row().state(), &expected);
    assert_eq!(Image::read(&store.0).unwrap(), once);
    for (name, table) in &before.tables {
        if !matches!(
            name.as_str(),
            "composer_draft_sync" | "composer_draft_continuity"
        ) {
            assert_eq!(once.tables.get(name), Some(table), "{name}");
        }
    }
    assert!(matches!(expected.workflow(), Workflow::Uncertain(_)));
    assert!(
        once.table("composer_draft_execution_result")
            .unwrap()
            .rows
            .is_empty()
    );
}

#[path = "upgrade_edge_tests.rs"]
mod edges;

#[test]
fn m04_fixture_channel_omits_thread_and_thread_keeps_exact_timestamp() {
    let owner = Owned::fresh().unwrap();
    for n in 0..2 {
        let stored = seed(&owner, n, 0);
        let destination = &stored.state.desired().unwrap().destinations[0];
        assert_eq!(destination["future"], json!({"keep":true}));
        if n == 0 {
            assert!(destination.get("thread_ts").is_none());
        } else {
            assert_eq!(destination["thread_ts"], json!("100.000001"));
        }
    }
}
