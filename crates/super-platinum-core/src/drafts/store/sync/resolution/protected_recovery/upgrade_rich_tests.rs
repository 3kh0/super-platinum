//! Bundled SQLite regressions, authored but not run by author. Synthetic Owned only.
use super::*;
use crate::drafts::store::{sync::tests::local, worker::UpgradeTestWorker};
use snapshot::Cell;

struct Fixture {
    owner: Arc<Owned>,
    worker: UpgradeTestWorker,
}
impl Fixture {
    fn new() -> Self {
        let owner = Owned::fresh().unwrap();
        let mut store = DraftStore::open(owner.path()).unwrap();
        let scope = Scope {
            account_id: owner.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        };
        let mut thread = scope.key.clone();
        thread.thread = Some("100.000001".into());
        let mut thread_local = local("retained \u{65e5}\u{672c} \u{1f680}");
        thread_local.attachments.push(
            serde_json::from_value(serde_json::json!({
                "id":1,"path":owner.path().with_extension("not-read-attachment"),
                "name":"retained-file.txt","bytes":17
            }))
            .unwrap(),
        );
        store
            .save([
                (scope.key.clone(), local("/example argument")),
                (thread.clone(), thread_local.clone()),
            ])
            .unwrap();
        // Preserve noncanonical whitespace bytes too; no attachment file is read.
        store
            .0
            .execute(
                "UPDATE composer_drafts SET content=?2 WHERE destination=?1",
                rusqlite::params![
                    serde_json::to_string(&thread).unwrap(),
                    serde_json::to_string_pretty(&thread_local).unwrap()
                ],
            )
            .unwrap();
        storage::activate(&store, owner.account()).unwrap();
        adapter_schema::install(&store, owner.account()).unwrap();
        let view = views::local(&store.0, &scope).unwrap();
        let review = operations::enroll(&store, &scope, &view).unwrap();
        let writer = writers::register(&store, &review).unwrap();
        let WriteResult::Applied(review) =
            writers::checkpoint(&store, &writer, 1, Some(&review), review.local(), None).unwrap()
        else {
            panic!("actual accepted writer")
        };
        slash_intent::runtime::fixture_install(&mut store, owner.account());
        // Actual journal reservation and retirement; no command dispatch/Claim.
        let plan = crate::slash_commands::controller::fixture_plan(
            &review,
            Arc::new(std::sync::atomic::AtomicBool::new(true)),
        );
        crate::slash_commands::journal::compare(&mut store.0, owner.account(), None, plan.prior())
            .unwrap();
        let mut raw = serde_json::to_value(plan.prior()).unwrap();
        raw["version"] = serde_json::json!(2);
        raw["retired"] = serde_json::json!(true);
        let retired =
            crate::slash_commands::controller::Record::decode(&serde_json::to_vec(&raw).unwrap())
                .unwrap();
        crate::slash_commands::journal::compare(
            &mut store.0,
            owner.account(),
            Some(plan.prior()),
            &retired,
        )
        .unwrap();
        drop(store);
        Self {
            owner,
            worker: UpgradeTestWorker::new(),
        }
    }
    fn review(&self) -> Arc<Review> {
        let (command, reply) = Command::new(self.owner.clone(), Action::Inspect);
        self.worker.run(&self.owner, command);
        let Reply::Inspected(review) = reply.recv().unwrap().unwrap() else {
            panic!("actual inspection")
        };
        review
    }
    fn authority(&self) -> Authority {
        authority::Authority::fixture(self.owner.clone(), self.review()).unwrap()
    }
    fn image(&self) -> Image {
        Image::read(
            &Connection::open_with_flags(self.owner.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap(),
        )
        .unwrap()
    }
    fn sql(&self, sql: &str) {
        Connection::open(self.owner.path())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
}

#[test]
fn migration_preserves_raw_anchors_writer_receipts_journal_and_clock() {
    let f = Fixture::new();
    let prior = f.image();
    let inspected = migrate(f.authority()).unwrap();
    assert_eq!(
        inspected.anchors().unwrap(),
        prior.table("composer_drafts").unwrap()
    );
    for (name, table) in &prior.tables {
        if name == "composer_drafts" || name == "composer_draft_adapter_format" {
            continue;
        }
        assert_eq!(
            inspected.image.table(name).unwrap(),
            table,
            "retained table {name}"
        );
    }
    assert!(
        !prior
            .table("composer_draft_writer_lineage")
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(!prior.table("sp_slash_intents").unwrap().rows.is_empty());
    assert_eq!(inspected.receipt.prior, prior);
}
#[test]
fn unchanged_actual_legacy_open_fails_and_cannot_republish_anchor() {
    let f = Fixture::new();
    migrate(f.authority()).unwrap();
    let before = f.image();
    let error = match DraftStore::open(f.owner.path()) {
        Ok(_) => panic!("old opener must fail"),
        Err(e) => e,
    };
    assert!(error.to_string().contains("index named composer_drafts"));
    let old = Connection::open(f.owner.path()).unwrap();
    assert!(
        old.prepare("SELECT destination,content FROM composer_drafts")
            .is_err()
    );
    assert!(old.execute("DELETE FROM composer_drafts", []).is_err());
    assert_eq!(f.image(), before);
}
#[test]
fn compatible_read_only_reopen_preserves_identity_and_returns_no_permission() {
    let f = Fixture::new();
    let first = migrate(f.authority()).unwrap();
    let bytes = first.image.clone();
    let id = first.identity.clone();
    drop(first);
    let reopened = inspect(f.owner.clone()).unwrap();
    assert_eq!(reopened.identity, id);
    assert_eq!(reopened.image, bytes);
    assert!(Arc::ptr_eq(&reopened.owner, &f.owner));
}
#[test]
fn every_schema_failure_rolls_back_all_raw_bytes_and_schema() {
    for at in [
        layout::Stage::Renamed,
        layout::Stage::Marker,
        layout::Stage::Adapter,
        layout::Stage::Content,
        layout::Stage::BeforeCommit,
    ] {
        let f = Fixture::new();
        let before = f.image();
        assert!(
            run(
                f.authority(),
                layout::Fault {
                    at: Some(at),
                    deferred_commit: false
                }
            )
            .is_err()
        );
        assert_eq!(f.image(), before, "rollback {at:?}");
        assert!(DraftStore::open(f.owner.path()).unwrap().load().is_ok());
    }
}
#[test]
fn deferred_commit_failure_rolls_back_completed_schema_transform() {
    let f = Fixture::new();
    let before = f.image();
    assert!(
        run(
            f.authority(),
            layout::Fault {
                at: None,
                deferred_commit: true
            }
        )
        .is_err()
    );
    assert_eq!(f.image(), before);
}
#[test]
fn busy_old_reader_blocks_migration_without_partial_schema() {
    let f = Fixture::new();
    let authority = f.authority();
    let before = f.image();
    let old = Connection::open(f.owner.path()).unwrap();
    old.execute_batch("BEGIN;").unwrap();
    let _: String = old
        .query_row("SELECT content FROM composer_drafts LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(migrate(authority).is_err());
    old.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(f.image(), before);
}
#[test]
fn changed_input_after_inspection_refuses_migration_without_rewriting() {
    let f = Fixture::new();
    let authority = f.authority();
    f.sql("UPDATE composer_drafts SET content=content||' ';");
    let before = f.image();
    assert!(migrate(authority).is_err());
    assert_eq!(f.image(), before);
}
#[test]
fn foreign_owned_review_cannot_construct_even_test_authority() {
    let one = Fixture::new();
    let two = Fixture::new();
    assert!(authority::Authority::fixture(one.owner.clone(), two.review()).is_err());
}
#[test]
fn unknown_trigger_is_not_executed_or_imported_into_shadow() {
    let f = Fixture::new();
    let authority = f.authority();
    f.sql("CREATE TRIGGER unknown_rich AFTER UPDATE ON composer_drafts BEGIN SELECT RAISE(ABORT,'unexpected'); END;");
    let before = f.image();
    assert!(migrate(authority).is_err());
    assert_eq!(f.image(), before);
}
#[test]
fn malformed_future_partial_barriers_are_read_only_refusals() {
    for sql in [
        "DROP INDEX composer_drafts;",
        "DROP TABLE composer_draft_rich_local;",
        "ALTER TABLE composer_drafts_rich_anchor_v1 RENAME TO missing_anchor;",
        "UPDATE composer_draft_rich_format SET migration_id='not-a-uuid';",
        "UPDATE composer_draft_rich_format SET account='other';",
        "PRAGMA ignore_check_constraints=ON;UPDATE composer_draft_rich_format SET version=2;",
        "INSERT INTO composer_draft_rich_local VALUES('opaque','{}');",
        "UPDATE composer_draft_rich_format SET prior='{}';",
    ] {
        let f = Fixture::new();
        migrate(f.authority()).unwrap();
        f.sql(sql);
        let before = f.image();
        assert!(inspect(f.owner.clone()).is_err());
        assert_eq!(f.image(), before);
    }
}
#[test]
fn old_protected_capability_refuses_and_no_legacy_fallback_is_created() {
    let f = Fixture::new();
    migrate(f.authority()).unwrap();
    let before = f.image();
    let store = DraftStore(Connection::open(f.owner.path()).unwrap());
    assert!(adapter_schema::require(&store.0).is_err());
    assert!(
        loading::load(
            &store,
            f.owner.account(),
            LoadMode::Current,
            &BTreeMap::new()
        )
        .is_err()
    );
    assert_eq!(f.image(), before);
}
#[test]
fn renamed_actual_anchor_trigger_still_advances_existing_clock() {
    let f = Fixture::new();
    migrate(f.authority()).unwrap();
    let conn = Connection::open(f.owner.path()).unwrap();
    let key: String = conn
        .query_row(
            "SELECT destination FROM composer_drafts_rich_anchor_v1 ORDER BY destination LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let before: i64 = conn
        .query_row(
            "SELECT revision FROM composer_draft_local_clock WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE composer_drafts_rich_anchor_v1 SET content=content WHERE destination=?1",
        [&key],
    )
    .unwrap();
    let after: i64 = conn
        .query_row(
            "SELECT revision FROM composer_draft_local_clock WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .unwrap();
    assert!(after > before);
    // This unit has no sanctioned content writer yet; the immutable first-stage
    // audit detects the external write even if user content bytes were unchanged.
    assert!(inspect(f.owner.clone()).is_err());
}
// A cached schema can let prepare succeed; query_row steps/reprepares before
// returning any row. Require the actual missing-table error, not empty results.
fn assert_legacy_read_refused(old: &Connection, sql: &str) {
    let error = old
        .query_row(sql, [], |row| row.get::<_, String>(0))
        .unwrap_err();
    assert!(
        matches!(error, rusqlite::Error::SqliteFailure(_, Some(ref message))
        if message == "no such table: composer_drafts")
    );
}
#[test]
fn existing_connection_reprepares_into_barrier_after_commit() {
    let f = Fixture::new();
    let old = Connection::open(f.owner.path()).unwrap();
    let _: i64 = old
        .query_row("SELECT COUNT(*) FROM composer_drafts", [], |r| r.get(0))
        .unwrap();
    migrate(f.authority()).unwrap();
    assert_legacy_read_refused(&old, "SELECT content FROM composer_drafts");
}
#[test]
fn primitive_wal_snapshot_is_not_revocable_and_is_not_migration_authority() {
    let f = Fixture::new();
    let conn = Connection::open(f.owner.path()).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    let prior = Image::read(&conn).unwrap();
    let receipt = layout::receipt(prior, f.owner.account()).unwrap();
    let old = Connection::open(f.owner.path()).unwrap();
    old.execute_batch("BEGIN;").unwrap();
    let count: i64 = old
        .query_row("SELECT COUNT(*) FROM composer_drafts", [], |r| r.get(0))
        .unwrap();
    // Test the primitive WITHOUT Authority or cold admission. This is a negative
    // proof of why a DB receipt/lock cannot authorize a live-process migration.
    conn.execute_batch("BEGIN IMMEDIATE;").unwrap();
    layout::install(&conn, &receipt, &layout::Fault::default()).unwrap();
    conn.execute_batch("COMMIT;").unwrap();
    assert_eq!(
        old.query_row("SELECT COUNT(*) FROM composer_drafts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        count
    );
    old.execute_batch("ROLLBACK;").unwrap();
    assert_legacy_read_refused(&old, "SELECT * FROM composer_drafts");
}
#[test]
fn marker_evidence_bound_fails_before_mutating_authorized_source() {
    let f = Fixture::new();
    let mut before = f.image();
    before.tables.get_mut("composer_drafts").unwrap().rows[0][1] =
        Cell::Text("x".repeat(1024 * 1024));
    assert!(layout::receipt(before, f.owner.account()).is_err());
}
#[test]
fn actual_unretired_prepared_command_blocks_migration() {
    let f = Fixture::new();
    let mut store = DraftStore::open(f.owner.path()).unwrap();
    let scope = Scope {
        account_id: f.owner.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let review = views::review(&store.0, &scope).unwrap();
    let plan = crate::slash_commands::controller::fixture_plan(
        &review,
        Arc::new(std::sync::atomic::AtomicBool::new(true)),
    );
    crate::slash_commands::journal::compare(&mut store.0, f.owner.account(), None, plan.prior())
        .unwrap();
    drop(store);
    let before = f.image();
    assert!(migrate(f.authority()).is_err());
    assert_eq!(f.image(), before);
}
#[test]
fn actual_pending_managed_operation_blocks_migration() {
    let f = Fixture::new();
    let store = DraftStore::open(f.owner.path()).unwrap();
    let scope = Scope {
        account_id: f.owner.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let review = views::review(&store.0, &scope).unwrap();
    let state = operations::stream_state(&store, f.owner.account(), "T1").unwrap();
    let stream = operations::open_stream(&store, f.owner.account(), "T1", &state).unwrap();
    let review = operations::verify(&store, &stream, &review).unwrap();
    let _prepared = operations::prepare(&store, &stream, &review).unwrap();
    drop(store);
    let before = f.image();
    assert!(migrate(f.authority()).is_err());
    assert_eq!(f.image(), before);
}
#[test]
fn own_rollback_journal_allows_identity_guard_but_not_closed_preflight() {
    let f = Fixture::new();
    let authority = f.authority();
    authority.preflight().unwrap();
    let conn = Connection::open(f.owner.path()).unwrap();
    conn.execute_batch("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE; ALTER TABLE composer_drafts RENAME TO temporary_anchor;").unwrap();
    let journal = std::path::PathBuf::from(format!("{}-journal", f.owner.path().display()));
    assert!(journal.is_file());
    authority.identity_current().unwrap();
    assert!(authority.preflight().is_err());
    conn.execute_batch("ROLLBACK;").unwrap();
    drop(conn);
    authority.preflight().unwrap();
}
#[test]
fn foreign_recovery_sidecars_refuse_before_sqlite_open() {
    for suffix in ["-journal", "-wal", "-shm"] {
        let f = Fixture::new();
        let authority = f.authority();
        let before = std::fs::read(f.owner.path()).unwrap();
        let sidecar = std::path::PathBuf::from(format!("{}{suffix}", f.owner.path().display()));
        std::fs::write(&sidecar, b"synthetic foreign sidecar").unwrap();
        assert!(migrate(authority).is_err());
        assert_eq!(std::fs::read(f.owner.path()).unwrap(), before);
        assert_eq!(
            std::fs::read(&sidecar).unwrap(),
            b"synthetic foreign sidecar"
        );
        std::fs::remove_file(sidecar).unwrap();
    }
}
