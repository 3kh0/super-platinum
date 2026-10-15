//! Real FIFO + actual Slice2a implementation on freshly owned temporary files.
use super::*;
use crate::drafts::store::protected::{LoadMode, owned_store::OwnedRoute};
use crate::drafts::store::sync::tests::local;
use rusqlite::{Connection, OpenFlags};

fn inspect(queue: &Queue) -> Arc<upgrade::Review> {
    let reply = queue
        .submit_upgrade(upgrade::Action::Inspect)
        .unwrap()
        .wait()
        .unwrap();
    let upgrade::Reply::Inspected(review) = &*reply else {
        panic!("inspection")
    };
    review.clone()
}
fn migrate(queue: &Queue) -> String {
    let before = inspect(queue);
    let reply = queue
        .submit_upgrade(upgrade::Action::Commit(before))
        .unwrap()
        .wait()
        .unwrap();
    let upgrade::Reply::Committed(receipt) = &*reply else {
        panic!("commit")
    };
    assert!(!receipt.repeated);
    receipt.id.clone()
}
fn logical(path: &std::path::Path) -> Vec<(String, String)> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut q = connection
        .prepare("SELECT destination,content FROM composer_drafts ORDER BY destination")
        .unwrap();
    q.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn fresh_inspection_and_cancellation_do_not_create_a_database() {
    let queue = Queue::fresh().unwrap();
    let path = queue.owner.path().to_owned();
    let review = inspect(&queue);
    assert_eq!(review.class(), upgrade::Class::Missing);
    assert!(!path.exists());
    assert!(queue.open(review).is_err());
    drop(queue);
    assert!(!path.exists());
}

#[test]
fn same_worker_handoff_retains_actual_owner_and_reattaches_without_another_command() {
    let queue = Queue::fresh().unwrap();
    let worker = queue.worker.clone();
    let owner = queue.owner.clone();
    let id = queue.worker_id().to_owned();
    let receipt = migrate(&queue);
    let review = inspect(&queue);
    assert_eq!(review.migration_id(), Some(receipt.as_str()));
    queue.open(review.clone()).unwrap();
    assert!(Arc::ptr_eq(&worker, &queue.worker));
    assert!(Arc::ptr_eq(&owner, &queue.owner));
    assert_eq!(id, queue.worker_id());
    let generation = queue.worker.pending.lock().unwrap().upgrade_generation;
    let jobs = queue.control.lock().unwrap().jobs.len();
    queue.open(review).unwrap();
    assert_eq!(
        queue.worker.pending.lock().unwrap().upgrade_generation,
        generation
    );
    assert_eq!(queue.control.lock().unwrap().jobs.len(), jobs);
    assert!(queue.submit_upgrade(upgrade::Action::Inspect).is_err());
}

#[test]
fn foreign_owner_worker_and_stale_generation_cannot_handoff() {
    let first = Queue::fresh().unwrap();
    migrate(&first);
    let old = inspect(&first);
    let second = Queue::fresh().unwrap();
    assert!(second.open(old.clone()).is_err());
    assert!(Queue::start(first.owner.clone()).is_err());
    inspect(&first);
    assert!(first.open(old).is_err());
    let current = inspect(&first);
    first.open(current).unwrap();
}

#[test]
fn handoff_rechecks_full_inventory_without_repairing_a_changed_file() {
    let queue = Queue::fresh().unwrap();
    migrate(&queue);
    let review = inspect(&queue);
    let conn = Connection::open(queue.owner.path()).unwrap();
    conn.execute_batch("PRAGMA user_version=991;").unwrap();
    drop(conn);
    let before = std::fs::read(queue.owner.path()).unwrap();
    assert!(queue.open(review).is_err());
    assert_eq!(std::fs::read(queue.owner.path()).unwrap(), before);
}

#[test]
fn public_lost_waiter_reattaches_same_commit_and_same_client_worker() {
    let route = OwnedRoute::fresh_fixture().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    let job = route.commit(&review).unwrap();
    let id = job.id();
    drop(job); // the route still owns the receiver, including a buffered reply
    let receipt = route.resume_commit(id.clone()).unwrap().wait().unwrap();
    assert!(!receipt.repeated);
    assert_eq!(route.resume_commit(id).unwrap().wait().unwrap(), receipt);
    let inspected = route.inspect().unwrap().wait().unwrap();
    let opened = inspected.open().unwrap();
    assert_eq!(opened.worker_id(), route.worker_id());
    assert_eq!(opened.migration_id(), Some(receipt.id.as_str()));
    let client = opened.client();
    // Existing protected API, not a factory initialization permit. Merely read
    // the inventory through the same Worker; A never chooses startup semantics.
    let loaded = client.load(LoadMode::Current).wait().unwrap();
    assert!(loaded.managed().is_empty() && loaded.locals().is_empty());
}

#[test]
fn public_foreign_review_and_job_identity_refuse() {
    let first = OwnedRoute::fresh_fixture().unwrap();
    let review = first.inspect().unwrap().wait().unwrap();
    let second = OwnedRoute::fresh_fixture().unwrap();
    assert!(matches!(second.commit(&review), Err(Error::Stale)));
    let job = first.commit(&review).unwrap();
    assert!(matches!(second.resume_commit(job.id()), Err(Error::Stale)));
    job.wait().unwrap();
}

#[test]
fn reopen_requires_all_actual_owners_to_release_then_inspects_same_receipt() {
    let route = OwnedRoute::fresh_fixture().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    let job = route.commit(&review).unwrap();
    let receipt = job.wait().unwrap();
    let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
    let ticket = opened.fixture_ticket().unwrap();
    let old_worker = route.worker_id().to_owned();
    let client = opened.client();
    assert!(OwnedRoute::reopen_fixture(&ticket).is_err());
    drop((review, job, route, opened));
    assert!(OwnedRoute::reopen_fixture(&ticket).is_err()); // retained Client owns lock
    drop(client);
    let reopened = OwnedRoute::reopen_fixture(&ticket).unwrap();
    assert_ne!(old_worker, reopened.worker_id());
    let actual = reopened.inspect().unwrap().wait().unwrap();
    assert_eq!(actual.metadata().migration_id, Some(receipt.id.clone()));
    let opened = actual.open().unwrap();
    assert_eq!(opened.migration_id(), Some(receipt.id.as_str()));
    assert!(opened.client().load(LoadMode::Current).wait().is_ok());
}

#[test]
fn local_raw_bytes_and_history_survive_real_migration_handoff_and_reopen() {
    let queue = Queue::fresh().unwrap();
    let key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    };
    let canonical = local("日本語 🚀 é\nretained raw");
    let mut store = DraftStore::open(queue.owner.path()).unwrap();
    store.save([(key.clone(), canonical)]).unwrap();
    let raw = format!(
        "  {}\n",
        store
            .0
            .query_row("SELECT content FROM composer_drafts", [], |row| row
                .get::<_, String>(0))
            .unwrap()
    );
    store
        .0
        .execute("UPDATE composer_drafts SET content=?1", [&raw])
        .unwrap();
    drop(store);
    let bytes = std::fs::read(queue.owner.path()).unwrap();
    let review = inspect(&queue);
    assert_eq!(std::fs::read(queue.owner.path()).unwrap(), bytes);
    let before = logical(queue.owner.path());
    let result = queue
        .submit_upgrade(upgrade::Action::Commit(review))
        .unwrap()
        .wait()
        .unwrap();
    assert!(matches!(&*result, upgrade::Reply::Committed(_)));
    assert_eq!(logical(queue.owner.path()), before);
    let path = queue.owner.path().to_owned();
    let history = |path: &std::path::Path| -> (String, String, String) {
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .query_row(
                "SELECT id,source,result FROM composer_draft_migration_receipt",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    };
    let expected_history = history(&path);
    queue.open(inspect(&queue)).unwrap();
    let ticket = queue.ticket().unwrap();
    drop(result);
    drop(queue);
    let reopened = Queue::reopen(&ticket).unwrap();
    reopened.open(inspect(&reopened)).unwrap();
    assert_eq!(logical(&path), before);
    assert_eq!(history(&path), expected_history);
}

#[test]
fn unsupported_partial_and_future_files_refuse_without_creating_authority() {
    for sql in [
        "CREATE TABLE surprise(x)",
        "CREATE TABLE composer_draft_continuity_format(x)",
        "PRAGMA user_version=9",
    ] {
        let queue = Queue::fresh().unwrap();
        let conn = Connection::open(queue.owner.path()).unwrap();
        conn.execute_batch(sql).unwrap();
        drop(conn);
        let before = std::fs::read(queue.owner.path()).unwrap();
        assert!(
            queue
                .submit_upgrade(upgrade::Action::Inspect)
                .unwrap()
                .wait()
                .is_err()
        );
        assert!(queue.proof.get().is_none());
        assert_eq!(std::fs::read(queue.owner.path()).unwrap(), before);
    }
}

#[test]
fn retained_jobs_have_a_hard_bound_and_no_silent_eviction() {
    let queue = Queue::fresh().unwrap();
    let first = queue.submit_upgrade(upgrade::Action::Inspect).unwrap();
    first.wait().unwrap();
    for _ in 1..MAX_JOBS {
        inspect(&queue);
    }
    assert!(matches!(
        queue.submit_upgrade(upgrade::Action::Inspect),
        Err(Error::Pending)
    ));
    assert!(Arc::ptr_eq(&first, &queue.retained_job(0, false).unwrap()));
    assert!(queue.retained_job(0, true).is_err());
}

#[test]
fn cancelled_commit_waiter_keeps_the_real_running_job_and_single_receipt() {
    let queue = Queue::fresh().unwrap();
    let review = inspect(&queue);
    let (arrived_tx, arrived) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let job = queue
        .submit_configured(upgrade::Action::Commit(review), |command| {
            command.after_seal = Some(Box::new(move || {
                arrived_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            }));
        })
        .unwrap();
    let index = queue.job_id(&job);
    arrived
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    drop(job);
    assert!(matches!(
        queue.submit_upgrade(upgrade::Action::Inspect),
        Err(Error::Pending)
    ));
    release.send(()).unwrap();
    let retained = queue.retained_job(index, true).unwrap();
    let first = retained.wait().unwrap();
    let second = retained.wait().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    let upgrade::Reply::Committed(receipt) = &*first else {
        panic!("commit")
    };
    assert!(!receipt.repeated);
    let conn =
        Connection::open_with_flags(queue.owner.path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM composer_draft_migration_receipt",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(queue.control.lock().unwrap().jobs.len(), 2);
}

#[test]
fn failed_commit_retains_rollback_failure_without_automatic_retry() {
    let queue = Queue::fresh().unwrap();
    let review = inspect(&queue);
    let job = queue
        .submit_configured(upgrade::Action::Commit(review), |command| {
            command.fault = Some(upgrade::Step::History);
        })
        .unwrap();
    assert!(matches!(job.wait(), Err(Error::Storage)));
    assert!(matches!(job.wait(), Err(Error::Storage)));
    assert!(!queue.owner.path().exists());
    assert!(queue.proof.get().is_none());
    assert_eq!(queue.control.lock().unwrap().jobs.len(), 2);
}

#[test]
fn fixture_locator_rejects_account_marker_database_and_ancestry_tampering() {
    let route = OwnedRoute::fresh_fixture().unwrap();
    let initial = route.inspect().unwrap().wait().unwrap();
    route.commit(&initial).unwrap().wait().unwrap();
    let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
    let encoded = opened.fixture_ticket().unwrap();
    drop((initial, opened, route));
    let original: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    for field in ["account", "marker", "database", "nonce", "unknown"] {
        let mut changed = original.clone();
        changed[field] = match field {
            "account" => serde_json::json!(uuid::Uuid::new_v4().to_string()),
            "marker" | "database" => serde_json::json!([0, 0]),
            _ => serde_json::json!("../outside"),
        };
        assert!(
            OwnedRoute::reopen_fixture(&changed.to_string()).is_err(),
            "{field}"
        );
    }
    assert!(OwnedRoute::reopen_fixture(&" ".repeat(1025)).is_err());
    let valid = OwnedRoute::reopen_fixture(&encoded).unwrap();
    assert!(valid.inspect().unwrap().wait().unwrap().open().is_ok());
}

#[test]
fn queued_protected_reply_finishes_before_final_owner_release() {
    let route = OwnedRoute::fresh_fixture().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    let job = route.commit(&review).unwrap();
    job.wait().unwrap();
    let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
    let ticket = opened.fixture_ticket().unwrap();
    let client = opened.client();
    let reply = client.load(LoadMode::Current);
    drop((client, opened, job, review, route));
    // Drop joins the actual worker before the marker goes away. This is a queue
    // lifetime test, not a graceful-close/drain certificate for retained Clients.
    assert!(reply.wait().is_ok());
    assert!(OwnedRoute::reopen_fixture(&ticket).is_ok());
}

#[test]
fn v5_uncertain_operation_and_unknown_snapshot_are_preserved_through_handoff() {
    use crate::drafts::store::sync::{Commit, Prepare};
    use crate::drafts::sync::{DraftSync, RemoteDraft, Snapshot};
    let queue = Queue::fresh().unwrap();
    let scope = Scope {
        account_id: queue.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let snapshot = Snapshot {
        blocks: vec![
            serde_json::json!({"type":"future_rich", "opaque":{"bytes":[0,255],"text":"日本語 🚀 é"}}),
        ],
        file_ids: vec!["F1".into()],
        destinations: vec![serde_json::json!({"channel_id":"C1", "future":{"retained":true}})],
    };
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: snapshot.clone(),
    };
    let mut state = DraftSync::adopt_remote(&scope, remote).unwrap();
    let mut changed = snapshot;
    changed
        .blocks
        .push(serde_json::json!({"unknown_leaf":"newer"}));
    state.edit(&scope, Some(changed));
    let canonical = local("newer 日本語 🚀 é");
    let mut store = DraftStore::open(queue.owner.path()).unwrap();
    store
        .save([(scope.key.clone(), canonical.clone())])
        .unwrap();
    let Commit::Applied(stored) = store
        .commit_synced(&scope, None, &canonical, &state, None)
        .unwrap()
    else {
        panic!("seed commit")
    };
    let Prepare::Prepared { stored, operation } =
        store.prepare_synced(&scope, stored.revision).unwrap()
    else {
        panic!("durable prepare")
    };
    state = stored.state;
    assert!(state.mark_uncertain(&scope, &operation.id));
    let Commit::Applied(stored) = store
        .commit_synced(&scope, Some(stored.revision), &canonical, &state, None)
        .unwrap()
    else {
        panic!("uncertain outcome")
    };
    let raw: String = store
        .0
        .query_row("SELECT state FROM composer_draft_sync", [], |row| {
            row.get(0)
        })
        .unwrap();
    let raw = format!("  {raw}\n");
    store
        .0
        .execute("UPDATE composer_draft_sync SET state=?1", [&raw])
        .unwrap();
    drop(store);
    migrate(&queue);
    let conn =
        Connection::open_with_flags(queue.owner.path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let history: String = conn
        .query_row(
            "SELECT source FROM composer_draft_migration_receipt",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(history.contains(&serde_json::to_string(&raw).unwrap()));
    drop(conn);
    queue.open(inspect(&queue)).unwrap();
    let (reply, receiver) = mpsc::channel();
    queue.submit_protected(ProtectedCommand {
        account: queue.account().into(),
        action: super::super::super::protected::commands::Action::Review { scope, reply },
    });
    let actual = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(actual.row().state(), &stored.state);
    assert_eq!(actual.row().revision(), stored.revision + 1);
    assert_eq!(
        serde_json::to_value(actual.local()).unwrap(),
        serde_json::to_value(&canonical).unwrap()
    );
    assert_eq!(
        actual.row().lineage().basis(),
        crate::drafts::store::protected::Basis::Unverified
    );
}

#[test]
fn protected_command_checks_actual_account_and_retained_file() {
    let queue = Queue::fresh().unwrap();
    migrate(&queue);
    queue.open(inspect(&queue)).unwrap();
    let original = std::fs::read(queue.owner.path()).unwrap();
    let (reply, receiver) = mpsc::channel();
    queue.submit_protected(ProtectedCommand {
        account: uuid::Uuid::new_v4().to_string(),
        action: super::super::super::protected::commands::Action::Load {
            mode: LoadMode::Current,
            reply,
        },
    });
    assert!(matches!(
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
        Err(Error::Storage)
    ));
    assert_eq!(std::fs::read(queue.owner.path()).unwrap(), original);
    let path = queue.owner.path().to_owned();
    let renamed = path.with_extension("retained-before-replacement");
    #[cfg(windows)]
    assert!(std::fs::rename(&path, &renamed).is_err());
    #[cfg(unix)]
    {
        std::fs::rename(&path, &renamed).unwrap();
        std::fs::write(&path, b"unrelated replacement").unwrap();
        let (reply, receiver) = mpsc::channel();
        queue.submit_protected(ProtectedCommand {
            account: queue.account().into(),
            action: super::super::super::protected::commands::Action::Load {
                mode: LoadMode::Current,
                reply,
            },
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"unrelated replacement");
    }
}

#[test]
fn reopened_fixture_rejects_wal_and_recovery_sidecars_before_sqlite_open() {
    for suffix in ["-wal", "-shm", "-journal"] {
        let queue = Queue::fresh().unwrap();
        migrate(&queue);
        queue.open(inspect(&queue)).unwrap();
        let ticket = queue.ticket().unwrap();
        let path = queue.owner.path().to_owned();
        drop(queue);
        let sidecar = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        std::fs::write(&sidecar, b"retained recovery evidence").unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(Queue::reopen(&ticket).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            std::fs::read(&sidecar).unwrap(),
            b"retained recovery evidence"
        );
    }
}

#[test]
fn handoff_refuses_a_recovery_sidecar_added_after_inspection_without_recovery() {
    let queue = Queue::fresh().unwrap();
    migrate(&queue);
    let review = inspect(&queue);
    let path = queue.owner.path().to_owned();
    let sidecar = std::path::PathBuf::from(format!("{}-journal", path.display()));
    std::fs::write(&sidecar, b"unaccepted recovery evidence").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(queue.open(review).is_err());
    assert!(queue.proof.get().is_none());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        std::fs::read(&sidecar).unwrap(),
        b"unaccepted recovery evidence"
    );
}

#[test]
fn terminated_worker_retains_failure_and_refuses_new_work() {
    let queue = Queue::fresh().unwrap();
    let job = queue
        .submit_configured(upgrade::Action::Inspect, |command| {
            command.after_open = Some(Box::new(|| panic!("fixture worker termination")));
        })
        .unwrap();
    assert!(matches!(job.wait(), Err(Error::Storage)));
    assert!(matches!(job.wait(), Err(Error::Storage)));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while *queue.alive.lock().unwrap() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(!*queue.alive.lock().unwrap());
    assert!(matches!(
        queue.submit_upgrade(upgrade::Action::Inspect),
        Err(Error::Storage)
    ));
    assert!(!queue.owner.path().exists());
}

#[test]
fn public_lost_handoff_result_reattaches_private_success_without_reexecution() {
    let route = OwnedRoute::fresh_fixture().unwrap();
    assert!(route.opened().is_none());
    let initial = route.inspect().unwrap().wait().unwrap();
    route.commit(&initial).unwrap().wait().unwrap();
    assert!(route.opened().is_none());
    let stale = route.inspect().unwrap().wait().unwrap();
    let current = route.inspect().unwrap().wait().unwrap();
    let opened = current.clone().open().unwrap();
    let worker = opened.worker_id().to_owned();
    let migration = opened.migration_id().unwrap().to_owned();
    let ticket = opened.fixture_ticket().unwrap();
    drop(opened); // the UI's completion/result handle was not retained
    assert!(stale.open().is_err());
    let retry = current.open().unwrap();
    assert_eq!(retry.worker_id(), worker);
    assert_eq!(retry.migration_id(), Some(migration.as_str()));
    drop((retry, initial)); // no public review or OpenedStore handles remain
    let retained = route.opened().unwrap();
    assert_eq!(retained.worker_id(), worker);
    assert_eq!(retained.migration_id(), Some(migration.as_str()));
    assert_eq!(retained.fixture_ticket().unwrap(), ticket);
    assert!(retained.client().load(LoadMode::Current).wait().is_ok());
}

#[test]
fn completed_handoff_lookup_has_no_queue_disk_or_new_inventory_effect() {
    let queue = Queue::fresh().unwrap();
    migrate(&queue);
    let stale = inspect(&queue);
    let actual = inspect(&queue);
    queue.open(actual.clone()).unwrap();
    let generation = queue.worker.pending.lock().unwrap().upgrade_generation;
    let jobs = queue.control.lock().unwrap().jobs.len();
    let bytes = std::fs::read(queue.owner.path()).unwrap();
    for _ in 0..3 {
        assert!(Arc::ptr_eq(&actual, &queue.opened().unwrap()));
        queue.open(actual.clone()).unwrap();
        assert!(queue.open(stale.clone()).is_err());
    }
    assert_eq!(
        queue.worker.pending.lock().unwrap().upgrade_generation,
        generation
    );
    assert_eq!(queue.control.lock().unwrap().jobs.len(), jobs);
    assert_eq!(std::fs::read(queue.owner.path()).unwrap(), bytes);
}
