//! Real isolated Owned worker/SQLite. Bounded waits below synchronize tests only.
use super::*;
use crate::drafts::store::worker::{Request, accounting};
use std::sync::mpsc;

fn queued_report(ops: &Operations, completion: &SlashCompletion) {
    let permit = ops.queue.worker.arm_slash(&completion.held).unwrap();
    ops.queue
        .worker
        .bind_slash_report(
            &completion.held,
            permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None),
        )
        .unwrap();
}

fn waiter(completion: SlashCompletion) -> mpsc::Receiver<Result<JobState, AdmissionError>> {
    let (send, result) = mpsc::channel();
    // Dropping the join handle models a detached UI waiter; the account and
    // retained completion still own all actual input/result evidence.
    std::thread::spawn(move || {
        let _ = send.send(completion.wait());
    });
    result
}

fn received(result: mpsc::Receiver<Result<JobState, AdmissionError>>) -> JobState {
    result
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap()
}

fn completion_records(ops: &Operations) -> Vec<accounting::Record> {
    ops.queue
        .diagnostics()
        .unwrap()
        .records
        .into_iter()
        .filter(|record| record.family == "protected-slash")
        // setup/completion executed exactly one real reserve and claim.
        .skip(2)
        .collect()
}

#[test]
fn unsubmitted_wait_never_sends_or_manufactures_settlement() {
    let (ops, input, _) = fixture::setup();
    let completion = completion(&ops, input.clone());
    assert_eq!(completion.wait().unwrap(), JobState::Waiting);
    queued_report(&ops, &completion);
    let report = completion.report().unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Waiting);
    assert!(completion_records(&ops).is_empty());
    assert!(completion.terminal().is_none());
    assert!(completion.take_fresh_clear().is_none());
    assert!(Arc::ptr_eq(&report, &completion.report().unwrap()));
    assert_eq!(records(&ops), vec![input.inner.plan.next().clone()]);
}

#[test]
fn resumed_concurrent_waiters_share_one_actual_commit_and_once_clear() {
    let (ops, input, _) = fixture::setup();
    let completion = completion(&ops, input.clone());
    queued_report(&ops, &completion);
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    db.0.execute_batch("BEGIN EXCLUSIVE").unwrap();
    completion.submit_storage().unwrap();
    let key = completion.key();
    let entered = completion.held.test_wait_events();
    let first = waiter(ops.resume_slash_completion(&key).unwrap());
    assert_eq!(
        entered.recv_timeout(Duration::from_secs(5)).unwrap(),
        "receiver-owned"
    );
    let second = waiter(ops.resume_slash_completion(&key).unwrap());
    assert_eq!(
        entered.recv_timeout(Duration::from_secs(5)).unwrap(),
        "condvar-wait"
    );
    // The hook sends while holding the actual status mutex. Acquiring it in
    // state() below synchronizes after Condvar::wait atomically releases it;
    // both branches are entered before COMMIT, not merely threads scheduled.
    assert!(matches!(first.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(matches!(second.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(completion.state(), JobState::Waiting);
    let retained = completion.report().unwrap();
    drop(completion);
    db.0.execute_batch("COMMIT").unwrap();
    assert_eq!(received(first), JobState::Ready);
    assert_eq!(received(second), JobState::Ready);
    let completion = ops.resume_slash_completion(&key).unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    let attempts = completion_records(&ops);
    assert_eq!(attempts.len(), 1);
    assert!(attempts[0].terminal.is_some());
    assert!(Arc::ptr_eq(&retained, &completion.report().unwrap()));
    let terminal = completion.terminal().unwrap();
    assert!(
        completion
            .take_fresh_clear()
            .unwrap()
            .matches(input.prior(), &terminal)
    );
    assert!(completion.take_fresh_clear().is_none());
    assert_eq!(records(&ops), vec![terminal]);
    assert!(current(&ops, &input).local().empty());
}

#[tokio::test]
async fn waited_failure_retries_storage_only_and_retains_exact_report() {
    let (ops, input, _) = fixture::setup();
    let completion = completion(&ops, input.clone());
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    db.0.execute_batch("CREATE TRIGGER slash_wait_fail BEFORE INSERT ON composer_draft_slash_completions BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    let calls = AtomicUsize::new(0);
    completion
        .execute_fixture(|permit| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move { permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None) }
        })
        .await
        .unwrap();
    assert_eq!(
        received(waiter(completion.clone())),
        JobState::Failed(p::Error::Storage)
    );
    let failed = completion_records(&ops);
    assert_eq!(failed.len(), 1);
    assert!(failed[0].terminal.is_some());
    let report = completion.report().unwrap();
    let key = completion.key();
    drop(completion);
    let completion = ops.resume_slash_completion(&key).unwrap();
    assert_eq!(
        completion.wait().unwrap(),
        JobState::Failed(p::Error::Storage)
    );
    assert!(completion.take_fresh_clear().is_none());
    db.0.execute_batch("DROP TRIGGER slash_wait_fail").unwrap();
    completion.retry_storage().unwrap();
    assert_eq!(received(waiter(completion.clone())), JobState::Ready);
    let attempts = completion_records(&ops);
    assert_eq!(attempts.len(), 2);
    assert_ne!(attempts[0].id, attempts[1].id);
    assert!(attempts.iter().all(|attempt| attempt.terminal.is_some()));
    assert!(Arc::ptr_eq(&report, &completion.report().unwrap()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        completion
            .execute_fixture(|_| async { panic!("no HTTP retry") })
            .await
            .is_err()
    );
    assert!(completion.take_fresh_clear().is_some());
    assert!(completion.take_fresh_clear().is_none());
}

#[test]
fn stopped_actual_worker_refuses_submit_and_wait_keeps_unsubmitted_report() {
    let (ops, input, _) = fixture::setup();
    let completion = completion(&ops, input.clone());
    queued_report(&ops, &completion);
    let report = completion.report().unwrap();
    {
        let _pending = ops.queue.worker.pending.lock().unwrap();
        ops.queue.stopping.store(true, Ordering::Release);
    }
    ops.queue.worker.ready.notify_all();
    ops.queue
        .thread
        .lock()
        .unwrap()
        .take()
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(completion.submit_storage(), Err(AdmissionError::Dead));
    assert_eq!(completion.wait().unwrap(), JobState::Waiting);
    assert!(Arc::ptr_eq(&report, &completion.report().unwrap()));
    assert!(completion_records(&ops).is_empty());
    assert!(completion.terminal().is_none());
    assert!(completion.take_fresh_clear().is_none());
    assert_eq!(records(&ops), vec![input.inner.plan.next().clone()]);
}

#[test]
fn lost_actual_queued_reply_wakes_waiters_without_a_commit_certificate() {
    let (ops, input, _) = fixture::setup();
    let completion = completion(&ops, input.clone());
    queued_report(&ops, &completion);
    let report = completion.report().unwrap();
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    db.0.execute_batch("BEGIN EXCLUSIVE").unwrap();
    // Block this real executor on a preceding actual read. The completion stays
    // queued, so test-only loss of its real sender is deterministic.
    let read = ops.review(input.review().scope().clone()).unwrap();
    read.submit().unwrap();
    until(|| {
        if ops
            .queue
            .diagnostics()
            .unwrap()
            .records
            .iter()
            .any(|r| r.stage == accounting::Stage::Detached)
        {
            Ok(())
        } else {
            Err(AdmissionError::Incomplete)
        }
    });
    completion.submit_storage().unwrap();
    let first = waiter(completion.clone());
    let second = waiter(ops.resume_slash_completion(&completion.key()).unwrap());
    {
        let mut pending = ops.queue.worker.pending.lock().unwrap();
        assert_eq!(pending.requests.len(), 1);
        assert!(
            matches!(&pending.requests[0], Request::SlashProtected(command) if matches!(command.as_ref(), p::slash_runtime::Command::Complete { .. }))
        );
        // Fault injection only: drop the actual queued command/reply sender and
        // mark its accounting abandoned; do not fabricate a received result.
        pending.requests.clear();
        pending.accounting.as_mut().unwrap().abandon();
    }
    ops.queue.worker.accounted.notify_all();
    assert_eq!(received(first), JobState::Disconnected);
    assert_eq!(received(second), JobState::Disconnected);
    assert!(
        completion_records(&ops)
            .iter()
            .all(|attempt| attempt.terminal.is_none())
    );
    assert!(Arc::ptr_eq(&report, &completion.report().unwrap()));
    assert!(completion.terminal().is_none());
    assert!(completion.take_fresh_clear().is_none());
    db.0.execute_batch("COMMIT").unwrap();
    assert_eq!(records(&ops), vec![input.inner.plan.next().clone()]);
}
