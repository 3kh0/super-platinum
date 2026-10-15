//! Proposed child of the existing drafts/store/worker.rs, never a second worker.
use super::super::sync::tests::{Database, local, scope};
use super::*;
use crate::slash_commands::{Failure, controller::Record, journal};
use serde_json::{Value, json};
fn record() -> Record {
    let s = scope();
    let row = json!({"type":"app","name":"/example","app":"A1","teams":["T1"]});
    Record::decode(&serde_json::to_vec(&json!({"id":uuid::Uuid::new_v4().to_string(),"version":1,
        "frozen":{"owner":{"account":s.account_id,"team":"T1","user":"U1","channel":"C1","thread":null},"revision":1,
        "original":{"ops":[{"insert":"/example"}]},"display":"/example","argument_blocks":null,
        "command":{"name":"/example","canonical":"/example","app":"A1","teams":["T1"],"thread_allowed":false,"autocomplete":true,"raw":row}},
        "phase":"Prepared","retry_after_secs":null,"http_status":null,"retired":false})).unwrap()).unwrap()
}
fn next(old: &Record, version: u64, phase: Value) -> Record {
    let mut raw = serde_json::to_value(old).unwrap();
    raw["version"] = json!(version);
    raw["phase"] = phase;
    Record::decode(&serde_json::to_vec(&raw).unwrap()).unwrap()
}
fn setup() -> Database {
    let db = Database::new();
    journal::install(&mut db.open().0, &scope().account_id).unwrap();
    db
}
fn submit(
    worker: &Worker,
    old: Option<Record>,
    next: Record,
) -> tokio::sync::oneshot::Receiver<Result<(), Failure>> {
    let (reply, receive) = tokio::sync::oneshot::channel();
    worker.submit(Request::SlashJournal(Box::new(journal::Command::Compare {
        account: scope().account_id,
        old,
        next,
        reply,
    })));
    receive
}
#[test]
fn actual_worker_fifo_commits_prepare_dispatch_and_completion_in_order() {
    let db = setup();
    let worker = Worker::default();
    let p = record();
    let d = next(&p, 2, json!("DispatchPossible"));
    let a = next(
        &d,
        3,
        json!({"Completed":{"Acknowledged":{"keep_input":false,"feedback":null,"raw":{"ok":true}}}}),
    );
    let replies = [
        submit(&worker, None, p.clone()),
        submit(&worker, Some(p), d.clone()),
        submit(&worker, Some(d), a.clone()),
    ];
    assert!(!worker.cycle(|_| Ok(db.open())));
    for reply in replies {
        assert_eq!(reply.blocking_recv().unwrap(), Ok(()));
    }
    assert!(journal::load(&mut db.open().0, &scope().account_id).unwrap() == vec![a]);
}
#[test]
fn actual_prior_legacy_failure_blocks_reservation_and_preserves_pending_input() {
    let db = setup();
    let worker = Worker::default();
    let s = scope();
    worker.enqueue(
        s.account_id.clone(),
        [(s.key.clone(), local("pending input"))].into(),
    );
    let reply = submit(&worker, None, record());
    let calls = std::cell::Cell::new(0);
    assert!(worker.cycle(|_| {
        let call = calls.get();
        calls.set(call + 1);
        if call == 0 {
            Err(sync::invalid("synthetic disk failure"))
        } else {
            Ok(db.open())
        }
    }));
    assert_eq!(reply.blocking_recv().unwrap(), Err(Failure::Stale));
    assert!(
        worker
            .pending
            .lock()
            .unwrap()
            .updates
            .contains_key(&(s.account_id.clone(), s.key))
    );
    assert!(
        journal::load(&mut db.open().0, &s.account_id)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn input_queued_while_opening_database_is_rechecked_before_admission() {
    let db = setup();
    let worker = Worker::default();
    let s = scope();
    let reply = submit(&worker, None, record());
    assert!(!worker.cycle(|_| {
        worker.enqueue(
            s.account_id.clone(),
            [(s.key.clone(), local("newer during open"))].into(),
        );
        Ok(db.open())
    }));
    assert_eq!(reply.blocking_recv().unwrap(), Err(Failure::Stale));
    assert!(
        journal::load(&mut db.open().0, &s.account_id)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn actual_worker_lost_dispatch_reply_never_reissues_admission() {
    let db = setup();
    let worker = Worker::default();
    let p = record();
    let d = next(&p, 2, json!("DispatchPossible"));
    let prepared = submit(&worker, None, p.clone());
    worker.cycle(|_| Ok(db.open()));
    assert_eq!(prepared.blocking_recv().unwrap(), Ok(()));
    drop(submit(&worker, Some(p.clone()), d.clone()));
    worker.cycle(|_| Ok(db.open()));
    let retry = submit(&worker, Some(p), d.clone());
    worker.cycle(|_| Ok(db.open()));
    assert_eq!(retry.blocking_recv().unwrap(), Err(Failure::NoReplay));
    assert!(journal::load(&mut db.open().0, &scope().account_id).unwrap() == vec![d]);
}
