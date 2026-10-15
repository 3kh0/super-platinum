use super::super::sync::tests::{Database, local, scope};
use super::*;
use crate::drafts::{codec, sync::RemoteDraft};
use serde_json::json;

fn setup() -> (Database, Scope, LocalReview, Snapshot) {
    let db = Database::new();
    let mut scope = scope();
    scope.key.thread = None;
    let original = local("Original");
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "Dr1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "100.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &original.composer).unwrap(),
    };
    let state = DraftSync::adopt_remote(&scope, base).unwrap();
    db.open()
        .save([(scope.key.clone(), original.clone())])
        .unwrap();
    assert!(matches!(
        db.open()
            .commit_synced(&scope, None, &original, &state, None)
            .unwrap(),
        Commit::Applied(_)
    ));
    let expected = db.open().read_local_review(&scope).unwrap();
    let mut raw = state.desired().unwrap().clone();
    raw.blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_quote","elements":[{"type":"text","text":"Dialog"}]}]}),
    ];
    (db, scope, expected, raw)
}

#[test]
fn queued_or_during_open_legacy_input_blocks_reviewed_schedule() {
    for during_open in [false, true] {
        let (db, scope, expected, raw) = setup();
        let worker = Worker::default();
        if !during_open {
            worker.enqueue(
                scope.account_id.clone(),
                [(scope.key.clone(), local("New legacy"))].into(),
            );
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        worker.submit(Request::CommitReviewedSchedule(expected, raw, 500, sender));
        worker.cycle(|_| {
            if during_open {
                worker.enqueue(
                    scope.account_id.clone(),
                    [(scope.key.clone(), local("New legacy"))].into(),
                );
            }
            Ok(db.open())
        });
        assert!(matches!(
            receiver.recv().unwrap().unwrap(),
            ReviewedScheduleCommit::Blocked | ReviewedScheduleCommit::Stale
        ));
        worker.cycle(|_| Ok(db.open()));
        let current = db.open().read_local_review(&scope).unwrap();
        assert_eq!(current.local.composer.text, "Original");
        assert_eq!(current.blocked.unwrap().draft.composer.text, "New legacy");
        assert!(current.stored.unwrap().state.consumption().is_none());
    }
}

#[test]
fn failed_legacy_write_remains_pending_and_refuses_schedule() {
    let (db, scope, expected, raw) = setup();
    let worker = Worker::default();
    worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("Failed local"))].into(),
    );
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::CommitReviewedSchedule(expected, raw, 500, sender));
    let calls = std::cell::Cell::new(0);
    worker.cycle(|_| {
        let count = calls.get();
        calls.set(count + 1);
        if count == 0 {
            Err(sync::invalid("isolated disk failure"))
        } else {
            Ok(db.open())
        }
    });
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        ReviewedScheduleCommit::Blocked
    ));
    assert_eq!(
        worker.pending.lock().unwrap().updates[&(scope.account_id.clone(), scope.key.clone())]
            .composer
            .text,
        "Failed local"
    );
    assert!(
        db.open()
            .read_local_review(&scope)
            .unwrap()
            .stored
            .unwrap()
            .state
            .consumption()
            .is_none()
    );
}

#[test]
fn dropped_schedule_commit_reply_is_not_replayed_or_dispatched() {
    let (db, scope, expected, raw) = setup();
    let worker = Worker::default();
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    worker.submit(Request::CommitReviewedSchedule(
        expected.clone(),
        raw.clone(),
        500,
        sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    let committed = db.open().read_local_review(&scope).unwrap();
    assert!(committed.local.composer.text.is_empty());
    assert_eq!(
        committed
            .stored
            .as_ref()
            .unwrap()
            .state
            .consumption()
            .unwrap()
            .snapshot,
        raw
    );
    assert!(committed.stored.as_ref().unwrap().state.pending().is_none());
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::CommitReviewedSchedule(expected, raw, 500, sender));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        ReviewedScheduleCommit::Stale
    ));
    assert_eq!(
        db.open()
            .read_local_review(&scope)
            .unwrap()
            .stored
            .unwrap()
            .state,
        committed.stored.unwrap().state
    );
}
