use super::*;
use crate::drafts::store::{sync::tests::local, worker::UpgradeTestWorker};

struct Fixture {
    owner: Arc<Owned>,
    worker: UpgradeTestWorker,
}
impl Fixture {
    fn fresh() -> Self {
        Self {
            owner: Owned::fresh().unwrap(),
            worker: UpgradeTestWorker::new(),
        }
    }
    fn run(&self, action: Action) -> Result<Reply, AppError> {
        let (command, rx) = Command::new(self.owner.clone(), action);
        self.worker.run(&self.owner, command);
        rx.recv().unwrap()
    }
    fn inspect(&self) -> Arc<Review> {
        let Reply::Inspected(r) = self.run(Action::Inspect).unwrap() else {
            panic!("inspect")
        };
        r
    }
    fn image(&self) -> Image {
        Image::read(
            &Connection::open_with_flags(self.owner.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap(),
        )
        .unwrap()
    }
    fn protected() -> Self {
        let f = Self::fresh();
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("100.000001".into()),
        };
        let mut store = DraftStore::open(f.owner.path()).unwrap();
        let value = local("retained \u{65e5}\u{672c} \u{1f680}");
        store.save([(key.clone(), value.clone())]).unwrap();
        store
            .0
            .execute(
                "UPDATE composer_drafts SET content=?2 WHERE destination=?1",
                params![
                    serde_json::to_string(&key).unwrap(),
                    serde_json::to_string_pretty(&value).unwrap()
                ],
            )
            .unwrap();
        drop(store);
        let before = f.image();
        assert!(matches!(
            f.run(Action::Commit(f.inspect())).unwrap(),
            Reply::Committed(_)
        ));
        assert_eq!(
            f.image().tables["composer_drafts"],
            before.tables["composer_drafts"]
        );
        f
    }
    fn initialize(&self) -> bool {
        let Reply::SlashInitialized { changed, .. } =
            self.run(Action::InitializeSlash(self.inspect())).unwrap()
        else {
            panic!("initialize")
        };
        changed
    }
}

#[test]
fn missing_requires_existing_commit_then_fresh_review_and_preserves_all_tables() {
    let f = Fixture::fresh();
    let r = f.inspect();
    assert!(f.run(Action::InitializeSlash(r)).is_err());
    assert!(!f.owner.path().exists());
    let r = f.inspect();
    assert!(matches!(
        f.run(Action::Commit(r)).unwrap(),
        Reply::Committed(_)
    ));
    let before = f.image();
    assert!(f.initialize());
    let after = f.image();
    for (name, table) in before.tables {
        assert_eq!(after.tables.get(&name), Some(&table));
    }
    assert_eq!(
        after.tables["composer_draft_slash_format"].rows[0][1],
        snapshot::Cell::Integer(2)
    );
    assert!(!f.initialize());
    assert_eq!(f.image(), after);
}

#[test]
fn existing_local_raw_rows_survive_and_old_review_or_intervening_work_refuses() {
    let f = Fixture::protected();
    let before = f.image();
    let r = f.inspect();
    let _intervening = f.inspect();
    assert!(f.run(Action::InitializeSlash(r)).is_err());
    assert_eq!(f.image(), before);
    assert!(f.initialize());
    let after = f.image();
    for (name, table) in before.tables {
        assert_eq!(after.tables.get(&name), Some(&table));
    }
    let r = f.inspect();
    let conn = Connection::open(f.owner.path()).unwrap();
    conn.execute("UPDATE composer_drafts SET content=content||' '", [])
        .unwrap();
    drop(conn);
    let changed = f.image();
    assert!(f.run(Action::InitializeSlash(r)).is_err());
    assert_eq!(f.image(), changed);
}

#[test]
fn existing_journal_and_versions_preserve_raw_history_and_only_upgrade_v1_marker() {
    for version in 0..=3 {
        let f = Fixture::protected();
        let mut store = DraftStore::open(f.owner.path()).unwrap();
        slash_intent::upgrade_reference(&mut store, f.owner.account(), version, version > 0)
            .unwrap();
        let scope = Scope {
            account_id: f.owner.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C2".into(),
                thread: None,
            },
        };
        store
            .save([(scope.key.clone(), local("/example argument"))])
            .unwrap();
        let view = views::local(&store.0, &scope).unwrap();
        let review = operations::enroll(&store, &scope, &view).unwrap();
        let writer = writers::register(&store, &review).unwrap();
        let WriteResult::Applied(review) =
            writers::checkpoint(&store, &writer, 1, Some(&review), review.local(), None).unwrap()
        else {
            panic!("checkpoint")
        };
        let plan = crate::slash_commands::controller::fixture_plan(
            &review,
            Arc::new(std::sync::atomic::AtomicBool::new(true)),
        );
        crate::slash_commands::journal::compare(
            &mut store.0,
            f.owner.account(),
            None,
            plan.prior(),
        )
        .unwrap();
        drop(store);
        let before = f.image();
        assert_eq!(f.initialize(), version < 2);
        let after = f.image();
        preserve(&before, &after).unwrap();
        assert_eq!(
            before.tables["sp_slash_intents"],
            after.tables["sp_slash_intents"]
        );
        assert!(!before.tables["sp_slash_intents"].rows.is_empty());
        assert!(!f.initialize());
        assert_eq!(f.image(), after);
    }
}

#[test]
fn partial_unknown_and_wrong_owner_schemas_refuse_without_repair() {
    for sql in [
        "CREATE TABLE sp_slash_meta(singleton INTEGER PRIMARY KEY,version INTEGER,account TEXT)",
        "CREATE TABLE unexpected(payload BLOB)",
        "CREATE TRIGGER unexpected_trigger AFTER INSERT ON composer_drafts BEGIN SELECT 1; END;",
        "UPDATE composer_draft_owner SET account='wrong-account'",
    ] {
        let f = Fixture::protected();
        let r = f.inspect();
        Connection::open(f.owner.path())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
        let before = f.image();
        assert!(f.run(Action::InitializeSlash(r)).is_err());
        assert!(f.run(Action::Inspect).is_err());
        assert_eq!(f.image(), before);
    }
}

#[test]
fn faults_and_actual_deferred_commit_failure_roll_back_every_schema_step() {
    for fault in [
        Step::Opened,
        Step::Adapter,
        Step::BeforeCommit,
        Step::Commit,
    ] {
        let f = Fixture::protected();
        let before = f.image();
        let r = f.inspect();
        let (mut command, rx) = Command::new(f.owner.clone(), Action::InitializeSlash(r));
        command.fault = Some(fault);
        f.worker.run(&f.owner, command);
        assert!(rx.recv().unwrap().is_err());
        assert_eq!(f.image(), before);
        assert!(f.initialize());
    }
}

#[test]
fn connection_lock_and_owner_mismatch_do_not_grant_initialization() {
    let f = Fixture::protected();
    let other = Fixture::protected();
    let r = f.inspect();
    let before = other.image();
    assert!(other.run(Action::InitializeSlash(r)).is_err());
    assert_eq!(other.image(), before);
    let r = f.inspect();
    let before = f.image();
    let lock = Connection::open(f.owner.path()).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE;").unwrap();
    assert!(f.run(Action::InitializeSlash(r)).is_err());
    lock.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(f.image(), before);
    assert!(f.initialize());
}
