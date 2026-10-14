use super::super::sync::tests::{Database, local, scope};
use super::*;
use crate::drafts::{codec, sync::RemoteDraft};
use serde_json::{Value, json};
fn seed(store: &mut DraftStore, scope: &Scope) -> Value {
    let local = local("local");
    let base = RemoteDraft {
        scope: scope.clone(),
        id: format!("D-{}", scope.key.channel),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &local.composer).unwrap(),
    };
    store.save([(scope.key.clone(), local.clone())]).unwrap();
    let state = DraftSync::adopt_remote(scope, base.clone()).unwrap();
    assert!(matches!(
        store
            .commit_synced(scope, None, &local, &state, None)
            .unwrap(),
        Commit::Applied(_)
    ));
    let snapshot = codec::encode_composer(
        &scope.key,
        &super::super::sync::tests::local("remote").composer,
    )
    .unwrap();
    json!({"ok":true,"draft":{"id":base.id,"client_msg_id":base.client_msg_id,"last_updated_ts":"11.000001","is_sent":false,"is_deleted":false,"date_scheduled":0,"blocks":snapshot.blocks,"file_ids":snapshot.file_ids,"destinations":snapshot.destinations}})
}
fn setup() -> (Database, Scope, Value) {
    let db = Database::new();
    let mut scope = scope();
    scope.key.thread = None;
    let raw = seed(&mut db.open(), &scope);
    (db, scope, raw)
}
fn review(db: &Database, scope: &Scope, raw: &Value) -> RemoteReview {
    RemoteReview::prepare(
        db.open().read_local_review(scope).unwrap(),
        raw.clone(),
        "U1",
    )
    .unwrap()
}
#[test]
fn queued_legacy_input_before_or_during_open_blocks_choice_without_losing_versions() {
    for during_open in [false, true] {
        let (db, scope, raw) = setup();
        let worker = Worker::default();
        let writer = super::super::LocalWriter::default();
        db.open()
            .save_owned_local_alternate(&scope, &local("reviewed alternate"), &writer, 1)
            .unwrap();
        let expected = review(&db, &scope, &raw);
        if !during_open {
            worker.enqueue(
                scope.account_id.clone(),
                [(scope.key.clone(), local("unseen legacy"))].into(),
            );
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        worker.submit(Request::ReconcileRemoteReview(
            expected,
            RemoteChoice::Remote,
            sender,
        ));
        worker.cycle(|_| {
            if during_open {
                worker.enqueue(
                    scope.account_id.clone(),
                    [(scope.key.clone(), local("unseen legacy"))].into(),
                );
            }
            Ok(db.open())
        });
        assert!(matches!(
            receiver.recv().unwrap().unwrap(),
            RemoteSettlement::Blocked | RemoteSettlement::Stale
        ));
        worker.cycle(|_| Ok(db.open()));
        let current = db.open().read_local_review(&scope).unwrap();
        assert_eq!(current.local.composer.text, "local");
        assert_eq!(
            current.blocked.unwrap().draft.composer.text,
            "unseen legacy"
        );
        assert_eq!(
            current.alternates[0].draft.composer.text,
            "reviewed alternate"
        );
    }
}
#[test]
fn failed_legacy_save_remains_pending_and_refuses_remote_choice() {
    let (db, scope, raw) = setup();
    let worker = Worker::default();
    let expected = review(&db, &scope, &raw);
    worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("not durable"))].into(),
    );
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::ReconcileRemoteReview(
        expected,
        RemoteChoice::Remote,
        sender,
    ));
    let opens = std::cell::Cell::new(0);
    assert!(worker.cycle(|_| {
        let n = opens.get();
        opens.set(n + 1);
        if n == 0 {
            Err(sync::invalid("fixture disk failure"))
        } else {
            Ok(db.open())
        }
    }));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        RemoteSettlement::Blocked
    ));
    let pending = worker.pending.lock().unwrap();
    assert_eq!(
        pending.updates[&(scope.account_id.clone(), scope.key.clone())]
            .composer
            .text,
        "not durable"
    );
    assert!(pending.failed.contains(&scope.account_id));
    drop(pending);
    assert_eq!(
        db.open()
            .read_local_review(&scope)
            .unwrap()
            .local
            .composer
            .text,
        "local"
    );
    worker.cycle(|_| Ok(db.open()));
    assert_eq!(
        db.open()
            .read_local_review(&scope)
            .unwrap()
            .blocked
            .unwrap()
            .draft
            .composer
            .text,
        "not durable"
    );
}
#[test]
fn account_blocked_bookkeeping_includes_other_destination_alternates() {
    for other in [false, true] {
        let (db, scope, raw) = setup();
        let worker = Worker::default();
        let writer = super::super::LocalWriter::default();
        db.open()
            .save_owned_local_alternate(&scope, &local("reviewed"), &writer, 1)
            .unwrap();
        let mut second = scope.clone();
        second.key.channel = "C2".into();
        if other {
            seed(&mut db.open(), &second);
            db.open()
                .save_owned_local_alternate(
                    &second,
                    &local("unreviewed other destination"),
                    &super::super::LocalWriter::default(),
                    1,
                )
                .unwrap();
        }
        worker
            .pending
            .lock()
            .unwrap()
            .blocked
            .insert(scope.account_id.clone());
        let (sender, receiver) = mpsc::sync_channel(1);
        worker.submit(Request::ReconcileRemoteReview(
            review(&db, &scope, &raw),
            RemoteChoice::Remote,
            sender,
        ));
        worker.cycle(|_| Ok(db.open()));
        assert!(matches!(
            receiver.recv().unwrap().unwrap(),
            RemoteSettlement::Applied { .. }
        ));
        assert_eq!(
            worker
                .pending
                .lock()
                .unwrap()
                .blocked
                .contains(&scope.account_id),
            other
        );
        assert!(
            db.open()
                .read_local_review(&scope)
                .unwrap()
                .alternates
                .is_empty()
        );
        if other {
            assert_eq!(
                db.open().read_local_review(&second).unwrap().alternates[0]
                    .draft
                    .composer
                    .text,
                "unreviewed other destination"
            );
        }
    }
}
#[test]
fn lost_reply_keeps_committed_choice_and_never_replays_it() {
    let (db, scope, raw) = setup();
    let worker = Worker::default();
    let expected = review(&db, &scope, &raw);
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    worker.submit(Request::ReconcileRemoteReview(
        expected.clone(),
        RemoteChoice::Remote,
        sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    let committed = db.open().read_local_review(&scope).unwrap();
    assert_eq!(committed.local.composer.text, "remote");
    let revision = committed.stored.unwrap().revision;
    worker.cycle(|_| panic!("nothing should replay"));
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::ReconcileRemoteReview(
        expected,
        RemoteChoice::Remote,
        sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        RemoteSettlement::Stale
    ));
    assert_eq!(
        db.open()
            .read_local_review(&scope)
            .unwrap()
            .stored
            .unwrap()
            .revision,
        revision
    );
}
#[test]
fn reviewed_writer_is_retired_but_newer_queued_input_remains_an_alternate() {
    let (db, scope, raw) = setup();
    let worker = Worker::default();
    let writer = super::super::LocalWriter::default();
    db.open()
        .save_owned_local_alternate(&scope, &local("reviewed"), &writer, 7)
        .unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::ReconcileRemoteReview(
        review(&db, &scope, &raw),
        RemoteChoice::Remote,
        sender,
    ));
    let (old_sender, old_receiver) = mpsc::sync_channel(1);
    worker.submit(Request::OwnedAlternate(
        scope.clone(),
        local("late reviewed writer"),
        writer.clone(),
        7,
        old_sender,
    ));
    let (new_sender, new_receiver) = mpsc::sync_channel(1);
    worker.submit(Request::OwnedAlternate(
        scope.clone(),
        local("newer input"),
        writer,
        8,
        new_sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        RemoteSettlement::Applied { .. }
    ));
    old_receiver.recv().unwrap().unwrap();
    new_receiver.recv().unwrap().unwrap();
    let current = db.open().read_local_review(&scope).unwrap();
    assert_eq!(current.local.composer.text, "remote");
    assert_eq!(current.alternates.len(), 1);
    assert_eq!(current.alternates[0].draft.composer.text, "newer input");
    assert!(
        worker
            .pending
            .lock()
            .unwrap()
            .blocked
            .contains(&scope.account_id)
    );
}
