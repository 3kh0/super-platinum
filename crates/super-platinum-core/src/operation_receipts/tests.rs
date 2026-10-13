use super::*;

struct Store {
    dir: std::path::PathBuf,
    account: String,
}
impl Store {
    fn new() -> Self {
        Self {
            dir: std::env::temp_dir()
                .join(format!("operation-receipts-test-{}", uuid::Uuid::new_v4())),
            account: uuid::Uuid::new_v4().to_string(),
        }
    }
    fn path(&self) -> std::path::PathBuf {
        self.dir.join("receipts.sqlite")
    }
    fn open(&self) -> Journal {
        Journal::open(self.path(), &self.account).unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn scope() -> FileScope {
    FileScope {
        team_id: "T1".into(),
        user_id: "U1".into(),
        file_id: "F1".into(),
    }
}
fn rename() -> Operation {
    Operation::Rename {
        original_title: "Old".into(),
        requested_title: "New · 日本語".into(),
    }
}
fn started(journal: &mut Journal, key: FileScope, now: u64) -> Receipt {
    match journal.begin(key, rename(), now).unwrap() {
        Begin::Started(receipt) => receipt,
        Begin::Blocked(_) => panic!("expected a new receipt"),
    }
}

#[test]
fn crash_recovery_preserves_exact_intent_and_blocks_different_writes() {
    let store = Store::new();
    let first = started(&mut store.open(), scope(), 100);
    let mut reopened = store.open();
    assert_eq!(reopened.get(&scope()).unwrap(), Some(first.clone()));
    assert_eq!(
        reopened.unresolved("T1", "U1").unwrap(),
        vec![first.clone()]
    );
    assert_eq!(
        reopened
            .begin(
                scope(),
                Operation::Delete {
                    original_title: "New".into()
                },
                500
            )
            .unwrap(),
        Begin::Blocked(first)
    );
}

#[test]
fn account_workspace_user_and_file_scopes_are_isolated() {
    let store = Store::new();
    let mut journal = store.open();
    let first = started(&mut journal, scope(), 100);
    for key in [
        FileScope {
            team_id: "T2".into(),
            ..scope()
        },
        FileScope {
            user_id: "U2".into(),
            ..scope()
        },
        FileScope {
            file_id: "F2".into(),
            ..scope()
        },
    ] {
        assert!(journal.get(&key).unwrap().is_none());
        assert!(
            !journal
                .resolve(&key, &first.operation_id, Outcome::Confirmed, 101, 0)
                .unwrap()
        );
        started(&mut journal, key, 101);
    }
    assert_eq!(journal.unresolved("T2", "U1").unwrap().len(), 1);
    assert_eq!(journal.unresolved("T1", "U2").unwrap().len(), 1);
    assert!(Journal::open(store.path(), &uuid::Uuid::new_v4().to_string()).is_err());
    assert_eq!(journal.get(&scope()).unwrap(), Some(first));
    let other = Store::new();
    assert!(other.open().get(&scope()).unwrap().is_none());
    assert_ne!(
        Journal::default_path(&store.account).unwrap(),
        Journal::default_path(&other.account).unwrap()
    );
}

#[test]
fn explicit_retirement_does_not_claim_success_and_stale_callbacks_cannot_resolve_replacement() {
    let store = Store::new();
    let mut journal = store.open();
    let first = started(&mut journal, scope(), 100);
    assert!(journal.retire(&scope(), &first.operation_id, 101).unwrap());
    assert_eq!(
        store.open().get(&scope()).unwrap().unwrap().status,
        Status::Retired
    );
    assert!(journal.unresolved("T1", "U1").unwrap().is_empty());
    let second = started(&mut journal, scope(), 102);
    assert_ne!(first.operation_id, second.operation_id);
    assert!(
        !journal
            .resolve(&scope(), &first.operation_id, Outcome::Confirmed, 103, 0)
            .unwrap()
    );
    assert!(!journal.retire(&scope(), &first.operation_id, 103).unwrap());
    assert!(
        !journal
            .defer_check(&scope(), &first.operation_id, 103, 999)
            .unwrap()
    );
    assert_eq!(journal.get(&scope()).unwrap(), Some(second.clone()));
    assert!(
        journal
            .resolve(&scope(), &second.operation_id, Outcome::Confirmed, 104, 0)
            .unwrap()
    );
    assert!(
        !journal
            .resolve(&scope(), &second.operation_id, Outcome::Rejected, 105, 0)
            .unwrap()
    );
    assert_eq!(
        store.open().get(&scope()).unwrap().unwrap().status,
        Status::Confirmed
    );
}

#[test]
fn cooldown_survives_restart_rejection_retirement_and_stale_clock() {
    let store = Store::new();
    let mut journal = store.open();
    let first = started(&mut journal, scope(), 100);
    assert!(
        journal
            .defer_check(&scope(), &first.operation_id, 105, 200)
            .unwrap()
    );
    assert!(
        journal
            .defer_check(&scope(), &first.operation_id, 99, 120)
            .unwrap()
    );
    let receipt = store.open().get(&scope()).unwrap().unwrap();
    assert_eq!((receipt.updated_at, receipt.retry_at), (105, 200));
    assert!(journal.retire(&scope(), &first.operation_id, 106).unwrap());
    assert!(matches!(
        store.open().begin(scope(), rename(), 199).unwrap(),
        Begin::Blocked(_)
    ));
    let second = started(&mut journal, scope(), 200);
    assert!(
        journal
            .resolve(&scope(), &second.operation_id, Outcome::Rejected, 201, 250)
            .unwrap()
    );
    assert!(matches!(
        store.open().begin(scope(), rename(), 249).unwrap(),
        Begin::Blocked(_)
    ));
    started(&mut journal, scope(), 250);
}

#[test]
fn concurrent_duplicate_begin_authorizes_exactly_one_dispatch() {
    let store = Store::new();
    drop(store.open());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = store.path();
            let account = store.account.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut journal = Journal::open(path, &account).unwrap();
                barrier.wait();
                journal.begin(scope(), rename(), 100).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Begin::Started(_)))
            .count(),
        1
    );
    let receipts: Vec<_> = results
        .into_iter()
        .map(|r| match r {
            Begin::Started(r) | Begin::Blocked(r) => r,
        })
        .collect();
    assert_eq!(receipts[0], receipts[1]);
}

#[test]
fn retention_bounds_finished_receipts_without_losing_unresolved_writes_or_cooldowns() {
    let store = Store::new();
    let mut journal = store.open();
    let uncertain = started(&mut journal, scope(), 1);
    let cooldown_scope = FileScope {
        file_id: "cooldown".into(),
        ..scope()
    };
    let cooldown = started(&mut journal, cooldown_scope.clone(), 1);
    journal
        .resolve(
            &cooldown_scope,
            &cooldown.operation_id,
            Outcome::Rejected,
            2,
            10_000,
        )
        .unwrap();
    for i in 0..140 {
        let key = FileScope {
            file_id: format!("finished-{i}"),
            ..scope()
        };
        let receipt = started(&mut journal, key.clone(), 3 + i);
        journal
            .resolve(&key, &receipt.operation_id, Outcome::Confirmed, 3 + i, 0)
            .unwrap();
    }
    let count: i64 = journal
        .conn
        .query_row("SELECT count(*) FROM receipts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, MAX_TERMINAL_RECEIPTS + 2);
    assert_eq!(journal.get(&scope()).unwrap(), Some(uncertain));
    assert!(journal.get(&cooldown_scope).unwrap().is_some());
    assert!(
        journal
            .get(&FileScope {
                file_id: "finished-0".into(),
                ..scope()
            })
            .unwrap()
            .is_none()
    );
}

#[test]
fn invalid_identity_intent_or_storage_fails_before_authorizing_dispatch() {
    let store = Store::new();
    assert!(Journal::open(store.path(), "../not-an-account").is_err());
    assert!(!store.path().exists());
    let mut journal = store.open();
    assert!(
        journal
            .begin(
                FileScope {
                    user_id: String::new(),
                    ..scope()
                },
                rename(),
                0
            )
            .is_err()
    );
    assert!(
        journal
            .begin(
                scope(),
                Operation::Rename {
                    original_title: "Old".into(),
                    requested_title: " \n".into()
                },
                0
            )
            .is_err()
    );
    assert!(journal.begin(scope(), rename(), u64::MAX).is_err());
    assert!(journal.get(&scope()).unwrap().is_none());
    journal.conn.execute_batch("PRAGMA query_only=ON").unwrap();
    assert!(journal.begin(scope(), rename(), 0).is_err());
}

#[test]
fn unsupported_schema_and_corrupt_receipt_fail_closed() {
    let store = Store::new();
    let mut journal = store.open();
    started(&mut journal, scope(), 1);
    journal
        .conn
        .execute("UPDATE receipts SET body='invalid json'", [])
        .unwrap();
    assert!(journal.begin(scope(), rename(), 2).is_err());
    journal
        .conn
        .execute_batch("PRAGMA user_version=99")
        .unwrap();
    assert!(Journal::open(store.path(), &store.account).is_err());
}

#[test]
fn malformed_stored_scope_cannot_escape_workspace_isolation() {
    let store = Store::new();
    let mut journal = store.open();
    let mut receipt = started(&mut journal, scope(), 1);
    receipt.scope.user_id = "another-user".into();
    journal
        .conn
        .execute(
            "UPDATE receipts SET body=?1",
            [serde_json::to_string(&receipt).unwrap()],
        )
        .unwrap();
    assert!(journal.get(&scope()).is_err());
    assert!(journal.unresolved("T1", "U1").is_err());
    assert!(journal.begin(scope(), rename(), 2).is_err());
}

#[cfg(unix)]
#[test]
fn receipt_database_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let store = Store::new();
    started(&mut store.open(), scope(), 1);
    assert_eq!(
        std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
