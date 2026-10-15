use super::super::sync::tests::{Database, local, scope};
use super::*;
use crate::drafts::{
    codec,
    sync::{ConsumeKind, RejectedScheduleChoice, RemoteDraft},
};

fn setup() -> (Database, Scope, LocalReview) {
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
    let mut state = DraftSync::adopt_remote(&scope, base).unwrap();
    db.open()
        .save([(scope.key.clone(), original.clone())])
        .unwrap();
    let Commit::Applied(row) = db
        .open()
        .commit_synced(&scope, None, &original, &state, None)
        .unwrap()
    else {
        panic!()
    };
    state
        .request_consume(&scope, 0, ConsumeKind::Schedule { post_at: 500 })
        .unwrap();
    let Commit::Applied(row) = db
        .open()
        .commit_synced(
            &scope,
            Some(row.revision),
            &LocalDraft::default(),
            &state,
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    let Prepare::Prepared {
        operation,
        mut stored,
    } = db.open().prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!()
    };
    assert!(stored.state.reject(&scope, &operation.id));
    assert!(matches!(
        db.open()
            .commit_synced(
                &scope,
                Some(stored.revision),
                &LocalDraft::default(),
                &stored.state,
                None
            )
            .unwrap(),
        Commit::Applied(_)
    ));
    let review = db.open().read_local_review(&scope).unwrap();
    (db, scope, review)
}
fn change(review: &LocalReview) -> RejectedScheduleChoice {
    RejectedScheduleChoice::Replace {
        snapshot: review
            .stored
            .as_ref()
            .unwrap()
            .state
            .consumption()
            .unwrap()
            .snapshot
            .clone(),
        post_at: 900,
    }
}

#[test]
fn pending_input_before_or_during_open_prevents_retry_and_keeps_rejection() {
    for during_open in [false, true] {
        let (db, scope, expected) = setup();
        let worker = Worker::default();
        if !during_open {
            worker.enqueue(
                scope.account_id.clone(),
                [(scope.key.clone(), local("New local"))].into(),
            );
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        worker.submit(Request::ReviewRejectedSchedule(
            expected.clone(),
            RejectedScheduleChoice::Retry,
            200,
            sender,
        ));
        worker.cycle(|_| {
            if during_open {
                worker.enqueue(
                    scope.account_id.clone(),
                    [(scope.key.clone(), local("New local"))].into(),
                );
            }
            Ok(db.open())
        });
        assert!(matches!(
            receiver.recv().unwrap().unwrap(),
            RejectedScheduleCommit::Stale | RejectedScheduleCommit::Blocked
        ));
        worker.cycle(|_| Ok(db.open()));
        let current = db.open().read_local_review(&scope).unwrap();
        assert_eq!(
            current.stored.unwrap().state,
            expected.stored.unwrap().state
        );
        assert_eq!(current.blocked.unwrap().draft.composer.text, "New local");
    }
}

#[test]
fn failed_legacy_input_blocks_change_and_stays_pending() {
    let (db, scope, expected) = setup();
    let worker = Worker::default();
    let choice = change(&expected);
    worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("Uncommitted"))].into(),
    );
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::ReviewRejectedSchedule(
        expected.clone(),
        choice,
        600,
        sender,
    ));
    let calls = std::cell::Cell::new(0);
    worker.cycle(|_| {
        let call = calls.get();
        calls.set(call + 1);
        if call == 0 {
            Err(sync::invalid("isolated disk failure"))
        } else {
            Ok(db.open())
        }
    });
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        RejectedScheduleCommit::Blocked
    ));
    assert_eq!(
        worker.pending.lock().unwrap().updates[&(scope.account_id.clone(), scope.key.clone())]
            .composer
            .text,
        "Uncommitted"
    );
    assert_eq!(
        db.open()
            .read_local_review(&scope)
            .unwrap()
            .stored
            .unwrap()
            .state,
        expected.stored.unwrap().state
    );
}

#[test]
fn lost_replacement_reply_commits_once_without_preparing_a_network_attempt() {
    let (db, scope, expected) = setup();
    let worker = Worker::default();
    let choice = change(&expected);
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    worker.submit(Request::ReviewRejectedSchedule(
        expected.clone(),
        choice.clone(),
        600,
        sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    let current = db.open().read_local_review(&scope).unwrap();
    let stored = current.stored.unwrap();
    assert_ne!(
        stored.state.consumption().unwrap().id,
        expected
            .stored
            .as_ref()
            .unwrap()
            .state
            .consumption()
            .unwrap()
            .id
    );
    assert!(stored.state.pending().is_none());
    assert!(current.local.composer.text.is_empty());
    let (sender, receiver) = mpsc::sync_channel(1);
    worker.submit(Request::ReviewRejectedSchedule(
        expected, choice, 600, sender,
    ));
    worker.cycle(|_| Ok(db.open()));
    assert!(matches!(
        receiver.recv().unwrap().unwrap(),
        RejectedScheduleCommit::Stale
    ));
    assert_eq!(
        db.open()
            .read_local_review(&scope)
            .unwrap()
            .stored
            .unwrap()
            .state,
        stored.state
    );
}
