//! Inspection and retained-path edge cases; isolated temporary roots only.
use super::*;

#[test]
fn m06_newly_created_path_replacement_cannot_report_success_or_delete_replacement() {
    let owner = Owned::fresh().unwrap();
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    let (mut command, rx) = Command::new(owner.clone(), Action::Commit(review));
    let path = owner.path().to_owned();
    let moved = path.with_extension("retained-original");
    command.before_seal = Some(Box::new(move || {
        #[cfg(unix)]
        {
            std::fs::rename(&path, &moved).unwrap();
            std::fs::write(&path, b"replacement evidence").unwrap();
        }
        #[cfg(windows)]
        {
            assert!(std::fs::rename(&path, &moved).is_err());
        }
    }));
    worker.run(&owner, command);
    #[cfg(unix)]
    {
        assert!(rx.recv().unwrap().is_err());
        assert_eq!(
            std::fs::read(owner.path()).unwrap(),
            b"replacement evidence"
        );
        assert!(owner.path().with_extension("retained-original").exists());
    }
    #[cfg(windows)]
    {
        assert!(rx.recv().unwrap().is_ok());
        assert!(!owner.path().with_extension("retained-original").exists());
    }
}
#[test]
fn m01_old_local_only_schema_is_inspected_without_ddl_and_migrates_without_enrollment() {
    let owner = Owned::fresh().unwrap();
    let conn = Connection::open(owner.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE composer_drafts(destination TEXT PRIMARY KEY,content TEXT NOT NULL)",
    )
    .unwrap();
    let key = serde_json::to_string(&scope(&owner, 0).key).unwrap();
    let content = serde_json::to_string(&local("old 日本語 🚀")).unwrap();
    conn.execute(
        "INSERT INTO composer_drafts VALUES(?1,?2)",
        params![key, content],
    )
    .unwrap();
    drop(conn);
    let original = image(&owner);
    let bytes = std::fs::read(owner.path()).unwrap();
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    assert_eq!(review.class(), Class::OrdinaryLocal);
    assert_eq!(std::fs::read(owner.path()).unwrap(), bytes);
    assert_eq!(commit(&worker, &owner, review).imported, 0);
    let final_image = image(&owner);
    original.preservation(&final_image).unwrap();
    assert!(
        final_image
            .table("composer_draft_sync")
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(!final_image.tables.contains_key("upload_receipts"));
}
#[test]
fn m02_wal_without_shm_and_hot_journal_refuse_before_sqlite_open() {
    for wal in [true, false] {
        let owner = Owned::fresh().unwrap();
        let store = DraftStore::open(owner.path()).unwrap();
        if wal {
            store.0.execute_batch("PRAGMA journal_mode=WAL; INSERT INTO composer_draft_owner VALUES(1,'fixture');").unwrap();
        }
        drop(store);
        let sidecar =
            |suffix: &str| std::path::PathBuf::from(format!("{}{suffix}", owner.path().display()));
        assert!(!sidecar("-shm").exists());
        if !wal {
            std::fs::write(
                sidecar("-journal"),
                b"retained crash journal: never replay or erase",
            )
            .unwrap();
        }
        let bytes = std::fs::read(owner.path()).unwrap();
        if wal {
            assert_eq!(bytes[18], 2);
        }
        let before = ["-wal", "-shm", "-journal"].map(|s| std::fs::read(sidecar(s)).ok());
        assert!(run(&UpgradeTestWorker::new(), &owner, Action::Inspect).is_err());
        assert_eq!(std::fs::read(owner.path()).unwrap(), bytes);
        assert_eq!(
            ["-wal", "-shm", "-journal"].map(|s| std::fs::read(sidecar(s)).ok()),
            before
        );
    }
}
#[test]
fn m02_live_wal_and_noncanonical_path_preflight_are_nonmutating() {
    let owner = Owned::fresh().unwrap();
    let store = DraftStore::open(owner.path()).unwrap();
    store.0.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE wal_probe(value TEXT); INSERT INTO wal_probe VALUES('uncheckpointed');").unwrap();
    let path = |s: &str| std::path::PathBuf::from(format!("{}{s}", owner.path().display()));
    let before = ["", "-wal", "-shm"].map(|s| std::fs::read(path(s)).unwrap());
    assert!(run(&UpgradeTestWorker::new(), &owner, Action::Inspect).is_err());
    assert_eq!(
        ["", "-wal", "-shm"].map(|s| std::fs::read(path(s)).unwrap()),
        before
    );
    assert!(owned::plain_chain(std::path::Path::new("relative.sqlite")).is_err());
    // PathBuf::join normalizes .. on Windows verbatim canonical paths. Build
    // the literal spelling so this actually tests the ParentDir refusal.
    let mut raw = owner.path().as_os_str().to_os_string();
    raw.push(std::path::MAIN_SEPARATOR_STR);
    raw.push("..");
    let parent_spelling = std::path::PathBuf::from(raw);
    assert!(
        parent_spelling
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    );
    assert!(owned::plain_chain(&parent_spelling).is_err());
}
#[test]
fn m02_malformed_persisted_history_rows_are_errors_not_indexing_panics() {
    for source in [false, true] {
        let owner = Owned::fresh().unwrap();
        seed(&owner, 0, 2);
        let worker = UpgradeTestWorker::new();
        let review = inspect(&worker, &owner);
        commit(&worker, &owner, review);
        let conn = Connection::open(owner.path()).unwrap();
        let column = if source { "source" } else { "result" };
        let raw: String = conn
            .query_row(
                &format!("SELECT {column} FROM composer_draft_migration_receipt"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        if source {
            value["tables"]["composer_draft_sync"]["rows"][0] = json!([]);
        } else {
            value["rows"][0] = json!([]);
        }
        conn.execute(
            &format!("UPDATE composer_draft_migration_receipt SET {column}=?1"),
            [value.to_string()],
        )
        .unwrap();
        drop(conn);
        let bytes = std::fs::read(owner.path()).unwrap();
        assert!(run(&worker, &owner, Action::Inspect).is_err());
        assert_eq!(std::fs::read(owner.path()).unwrap(), bytes);
    }
}

#[test]
fn m06_lost_success_reply_recovers_same_receipt_and_lineages_without_reimport() {
    let owner = Owned::fresh().unwrap();
    seed(&owner, 0, 2);
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    let (command, rx) = Command::new(owner.clone(), Action::Commit(review.clone()));
    drop(rx);
    worker.run(&owner, command);
    let after = image(&owner);
    let repeated = commit(&worker, &owner, review.clone());
    assert!(repeated.repeated);
    assert_eq!(repeated.id, review.id);
    assert_eq!(image(&owner), after);
}
#[test]
fn m02_retained_marker_and_database_file_identity_are_actual_handles() {
    let owner = Owned::fresh().unwrap();
    seed(&owner, 0, 0);
    let worker = UpgradeTestWorker::new();
    let review = inspect(&worker, &owner);
    let marker = owner.path().parent().unwrap().join("owner");
    if let Ok(second) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(marker)
    {
        assert!(second.try_lock().is_err());
    }
    let moved = owner.path().with_extension("retained-original");
    #[cfg(windows)]
    {
        assert!(std::fs::rename(owner.path(), &moved).is_err());
    }
    #[cfg(unix)]
    {
        std::fs::rename(owner.path(), &moved).unwrap();
        std::fs::copy(&moved, owner.path()).unwrap();
        let before = std::fs::read(owner.path()).unwrap();
        assert!(run(&worker, &owner, Action::Commit(review)).is_err());
        assert_eq!(std::fs::read(owner.path()).unwrap(), before);
        assert_eq!(std::fs::read(moved).unwrap(), before);
    }
    #[cfg(not(unix))]
    {
        let _ = review;
    }
}
