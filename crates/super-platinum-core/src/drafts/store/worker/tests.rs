use super::super::sync::tests::{Database, enroll, local, scope};
use super::*;

#[test]
fn queued_legacy_edits_cannot_overwrite_enrolled_pair_or_authorize_prepare() {
    let db = Database::new();
    let scope = scope();
    let row = enroll(&mut db.open(), &scope);
    let worker = Worker::default();
    worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("Legacy after enrollment"))].into(),
    );
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::Prepare(scope.clone(), row.revision, sender));
    assert!(!worker.cycle(|_| Ok(db.open())));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        Prepare::Blocked(_)
    ));
    let loaded = db
        .open()
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(loaded.local[0].1.composer.text, "Original 🚀");
    assert_eq!(
        loaded.blocked_local[&scope.key].draft.composer.text,
        "Legacy after enrollment"
    );
    assert!(worker.pending.lock().unwrap().updates.is_empty());
    assert!(
        worker
            .pending
            .lock()
            .unwrap()
            .blocked
            .contains(&scope.account_id)
    );
    // Explicit settlement is itself ordered through the same worker.
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::Commit(
        scope.clone(),
        Some(row.revision),
        local("Original 🚀"),
        row.state,
        Some(loaded.blocked_local[&scope.key].token.clone()),
        sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        Commit::Applied(_)
    ));
    assert!(
        !worker
            .pending
            .lock()
            .unwrap()
            .blocked
            .contains(&scope.account_id)
    );
}

#[test]
fn failed_legacy_retry_is_exposed_but_never_overlays_synced_local_or_grants_dispatch() {
    let db = Database::new();
    let scope = scope();
    let row = enroll(&mut db.open(), &scope);
    let worker = Worker::default();
    worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("Not yet durable"))].into(),
    );
    let (sender, local_reply) = mpsc::sync_channel(1);
    worker.submit(Request::Load(scope.account_id.clone(), sender));
    let (sender, sync_reply) = mpsc::sync_channel(1);
    worker.submit(Request::LoadSynced(
        scope.account_id.clone(),
        LoadMode::Current,
        sender,
    ));
    let (sender, prepare_reply) = mpsc::sync_channel(1);
    worker.submit(Request::Prepare(scope.clone(), row.revision, sender));
    let calls = std::cell::Cell::new(0);
    assert!(worker.cycle(|_| {
        let call = calls.get();
        calls.set(call + 1);
        if call == 0 {
            Err(sync::invalid("test disk full"))
        } else {
            Ok(db.open())
        }
    }));
    assert_eq!(
        local_reply.recv().unwrap().unwrap()[0].1.composer.text,
        "Original 🚀"
    );
    let loaded = sync_reply.recv().unwrap().unwrap();
    assert_eq!(
        loaded.uncommitted_local[&scope.key].composer.text,
        "Not yet durable"
    );
    assert!(prepare_reply.recv().unwrap().is_err());
    assert!(!worker.cycle(|_| Ok(db.open())));
    let loaded = db
        .open()
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(
        loaded.blocked_local[&scope.key].draft.composer.text,
        "Not yet durable"
    );
    assert_eq!(loaded.synced[&scope.key].state, row.state);
}

#[test]
fn lost_worker_reply_leaves_durable_pending_operation_and_never_replays_it() {
    let db = Database::new();
    let scope = scope();
    let row = enroll(&mut db.open(), &scope);
    let worker = Worker::default();
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    worker.submit(Request::Prepare(scope.clone(), row.revision, sender));
    worker.cycle(|_| Ok(db.open()));
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::Prepare(scope.clone(), row.revision, sender));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        Prepare::Stale(_)
    ));
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::LoadSynced(
        scope.account_id.clone(),
        LoadMode::AfterRestart,
        sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    let loaded = receiver.recv().unwrap().unwrap();
    assert!(matches!(
        loaded.synced[&scope.key].state.workflow(),
        crate::drafts::sync::Workflow::Uncertain(_)
    ));
}

#[test]
fn release_blocks_local_edit_queued_during_open_before_commit() {
    let db = Database::new();
    let scope = scope();
    let row = enroll(&mut db.open(), &scope);
    let worker = Worker::default();
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::Release(scope.clone(), row.revision, sender));
    worker.cycle(|_| {
        worker.enqueue(
            scope.account_id.clone(),
            [(scope.key.clone(), local("Newer queued input"))].into(),
        );
        Ok(db.open())
    });
    assert!(receiver.recv().unwrap().is_err());
    assert!(
        db.open()
            .load_synced(&scope.account_id, LoadMode::Current)
            .unwrap()
            .synced
            .contains_key(&scope.key)
    );
    assert!(
        worker
            .pending
            .lock()
            .unwrap()
            .updates
            .contains_key(&(scope.account_id.clone(), scope.key.clone()))
    );
}

#[test]
fn successful_release_serializes_before_following_legacy_edit() {
    let db = Database::new();
    let scope = scope();
    let row = enroll(&mut db.open(), &scope);
    let worker = Worker::default();
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::Release(scope.clone(), row.revision, sender));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        Release::Released(_)
    ));
    worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("Local after release"))].into(),
    );
    worker.cycle(|_| Ok(db.open()));
    let loaded = db
        .open()
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert!(loaded.synced.is_empty());
    assert!(loaded.blocked_local.is_empty());
    assert_eq!(loaded.local[0].1.composer.text, "Local after release");
}

#[test]
fn released_conditional_write_never_overwrites_newer_visit_legacy_input() {
    for during_open in [false, true] {
        let db = Database::new();
        let scope = scope();
        let row = enroll(&mut db.open(), &scope);
        db.open().release_synced(&scope, row.revision).unwrap();
        let worker = Worker::default();
        let (sender, receiver) = mpsc::sync_channel(1);
        worker.submit(Request::ConditionalLocal(
            scope.clone(),
            local("Original 🚀"),
            local("Old visit"),
            sender,
        ));
        if !during_open {
            worker.enqueue(
                scope.account_id.clone(),
                [(scope.key.clone(), local("New visit"))].into(),
            );
        }
        worker.cycle(|_| {
            if during_open {
                worker.enqueue(
                    scope.account_id.clone(),
                    [(scope.key.clone(), local("New visit"))].into(),
                );
            }
            Ok(db.open())
        });
        assert!(!receiver.recv().unwrap().unwrap());
        worker.cycle(|_| Ok(db.open()));
        assert_eq!(db.open().load().unwrap()[0].1.composer.text, "New visit");
    }
}

#[test]
fn release_and_conditional_disk_wait_never_hold_ui_enqueue_mutex() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    static BUSY: AtomicBool = AtomicBool::new(false);
    for releasing in [true, false] {
        let db = Database::new();
        let scope = scope();
        let row = enroll(&mut db.open(), &scope);
        if !releasing {
            db.open().release_synced(&scope, row.revision).unwrap();
        }
        let store = db.open();
        store
            .0
            .busy_handler(Some(|_| {
                BUSY.store(true, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(1));
                true
            }))
            .unwrap();
        let blocker = db.open();
        blocker.0.execute_batch("BEGIN IMMEDIATE").unwrap();
        BUSY.store(false, Ordering::SeqCst);
        let worker = Arc::new(Worker::default());
        let (release_sender, release_receiver) = mpsc::sync_channel(1);
        let (local_sender, local_receiver) = mpsc::sync_channel(1);
        if releasing {
            worker.submit(Request::Release(
                scope.clone(),
                row.revision,
                release_sender,
            ));
        } else {
            worker.submit(Request::ConditionalLocal(
                scope.clone(),
                local("Original 🚀"),
                local("Conditional input"),
                local_sender,
            ));
        }
        let running = worker.clone();
        let thread = std::thread::spawn(move || {
            let store = std::cell::RefCell::new(Some(store));
            running.cycle(|_| Ok(store.borrow_mut().take().unwrap()));
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !BUSY.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let busy = BUSY.load(Ordering::SeqCst);
        let unlocked = worker.pending.try_lock().is_ok();
        if unlocked {
            worker.enqueue(
                scope.account_id.clone(),
                [(scope.key.clone(), local("Later UI input"))].into(),
            );
        }
        blocker.0.execute_batch("COMMIT").unwrap();
        thread.join().unwrap();
        assert!(
            busy,
            "SQLite transaction must have started before mutex assertion"
        );
        assert!(unlocked, "disk wait must not block UI enqueue");
        if releasing {
            assert!(matches!(
                release_receiver.recv().unwrap().unwrap(),
                Release::Released(_)
            ));
        } else {
            assert!(local_receiver.recv().unwrap().unwrap());
        }
        worker.cycle(|_| Ok(db.open()));
        assert_eq!(
            db.open().load().unwrap()[0].1.composer.text,
            "Later UI input"
        );
    }
}

#[test]
fn local_settlement_rejects_queued_legacy_edit_without_erasing_it() {
    for enqueue_during_open in [false, true] {
        let db = Database::new();
        let scope = scope();
        let worker = Worker::default();
        enroll(&mut db.open(), &scope);
        let writer = super::super::LocalWriter::default();
        db.open()
            .save_owned_local_alternate(&scope, &local("reviewed"), &writer, 1)
            .unwrap();
        let review = db.open().read_local_review(&scope).unwrap();
        if !enqueue_during_open {
            worker.enqueue(
                scope.account_id.clone(),
                [(scope.key.clone(), local("unseen legacy"))].into(),
            );
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        worker.submit(Request::SettleLocalVersions(
            review,
            LocalChoice::Current,
            sender,
        ));
        worker.cycle(|_| {
            if enqueue_during_open {
                worker.enqueue(
                    scope.account_id.clone(),
                    [(scope.key.clone(), local("unseen legacy"))].into(),
                );
            }
            Ok(db.open())
        });
        assert!(matches!(
            receiver.recv().unwrap().unwrap(),
            LocalSettlement::Stale | LocalSettlement::Blocked
        ));
        worker.cycle(|_| Ok(db.open()));
        let current = db.open().read_local_review(&scope).unwrap();
        assert_eq!(
            current.blocked.unwrap().draft.composer.text,
            "unseen legacy"
        );
        assert_eq!(current.alternates.len(), 1);
        assert_eq!(current.local.composer.text, "Original 🚀");
    }
}

#[test]
fn local_settlement_orders_delayed_writer_requests_without_resurrection() {
    let db = Database::new();
    let mut scope = scope();
    scope.key.thread = None;
    let worker = Worker::default();
    let writer = super::super::LocalWriter::default();
    db.open()
        .save([(scope.key.clone(), local("chosen current"))])
        .unwrap();
    db.open()
        .save_owned_local_if_unchanged(
            &scope,
            &local("wrong base"),
            &local("reviewed old"),
            &writer,
            2,
        )
        .unwrap();
    let review = db.open().read_local_review(&scope).unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::SettleLocalVersions(
        review,
        LocalChoice::Current,
        sender,
    ));
    let (old_sender, old_receiver) = mpsc::sync_channel(1);
    worker.submit(Request::OwnedConditionalLocal(
        scope.clone(),
        local("chosen current"),
        local("stale old"),
        writer.clone(),
        2,
        old_sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        LocalSettlement::Applied { .. }
    ));
    assert!(!old_receiver.recv().unwrap().unwrap());
    let review = db.open().read_local_review(&scope).unwrap();
    assert_eq!(review.local.composer.text, "chosen current");
    assert!(review.blocked.is_none() && review.alternates.is_empty());
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::OwnedConditionalLocal(
        scope.clone(),
        local("chosen current"),
        local("new unseen typing"),
        writer,
        3,
        sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    assert!(!receiver.recv().unwrap().unwrap());
    let review = db.open().read_local_review(&scope).unwrap();
    assert_eq!(review.local.composer.text, "chosen current");
    assert_eq!(
        review.alternates[0].draft.composer.text,
        "new unseen typing"
    );
}
