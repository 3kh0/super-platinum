//! Real isolated Worker/SQLite regressions. No relaxed clean acknowledgment.
use super::*;
use crate::drafts::store::terminal::{self, Execution, Report};

fn checkpoint_fixture() -> (Operations, Scope, p::Stream, p::Writer, p::Review) {
    let (ops, scope) = fixture();
    let local = acknowledged(ops.local_inventory(scope.clone()).unwrap());
    let review = done(ops.enroll(local).unwrap());
    let state = done(
        ops.stream_state(TeamInput {
            scope: scope.clone(),
        })
        .unwrap(),
    );
    let stream = done(
        ops.open_stream(OpenInput {
            scope: scope.clone(),
            expected: state,
        })
        .unwrap(),
    );
    let review = done(
        ops.verify_local(MutationInput {
            stream: stream.clone(),
            expected: review,
        })
        .unwrap(),
    );
    let writer = done(ops.register(review.clone()).unwrap());
    (ops, scope, stream, writer, review)
}
fn checkpoint(
    ops: &Operations,
    writer: &p::Writer,
    expected: p::Review,
    serial: u64,
    text: &str,
) -> StorageJob<CheckpointInput, p::WriteResult> {
    ops.checkpoint(CheckpointInput {
        writer: writer.clone(),
        serial,
        expected,
        local: sync::tests::local(text),
        direct: None,
    })
    .unwrap()
}
#[test]
fn repaired_checkpoint_observes_exact_history_and_refuses_forgery_fence_and_replay() {
    let (ops, scope, _, writer, review) = checkpoint_fixture();
    done(
        ops.fixture_fault(scope.clone(), p::test_support::FaultPoint::WriterCommit)
            .unwrap(),
    );
    let job = checkpoint(&ops, &writer, review, 1, "immutable repaired text");
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    assert!(job.observe_checkpoint_settlement().is_err());
    done(
        ops.fixture_fault(scope.clone(), p::test_support::FaultPoint::WriterCommit)
            .unwrap(),
    );
    job.retry_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    job.retry_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    assert!(
        job.observe_checkpoint().is_err(),
        "repair is not clean success"
    );
    let (actual, history, dropped) = job.observe_checkpoint_settlement().unwrap();
    assert_eq!(actual.local().composer.text, "immutable repaired text");
    assert_eq!(history.failed_submissions().len(), 2);
    assert!(!history.operation_pending());
    assert_eq!(
        job.failed_attempts(),
        history
            .failed_submissions()
            .into_iter()
            .map(|id| (id, p::Error::Storage))
            .collect::<Vec<_>>()
    );
    let count = ops.retained().len();
    let records = ops.queue.diagnostics().unwrap().records.len();
    drop(dropped);
    assert_eq!(ops.retained().len(), count);
    let mut wrong: Vec<_> = history
        .failed_submissions()
        .into_iter()
        .map(|id| (id, Report::new(Execution::StorageUnresolved)))
        .collect();
    wrong.push((history.successful_submission(), terminal::review(&actual)));
    wrong[0].1 = Report::new(Execution::StorageAccepted);
    assert!(
        ops.queue
            .worker
            .observe_writer_settlement(
                &job.step,
                "protected-checkpoint",
                terminal::review(&actual),
                Some(&wrong),
                true
            )
            .is_err()
    );
    assert!(
        ops.queue
            .worker
            .observe_writer_settlement(
                &job.step,
                "protected-write-local",
                terminal::review(&actual),
                None,
                false
            )
            .is_err()
    );
    assert_eq!(ops.retained().len(), count);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
    let (_, _, receipt) = job.observe_checkpoint_settlement().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    let receipt = match receipt.acknowledge() {
        Err((AdmissionError::Fencing, receipt)) => receipt,
        _ => panic!("fence must retain the exact receipt"),
    };
    assert_eq!(ops.retained().len(), count);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(receipt.acknowledge().is_ok());
    assert_eq!(ops.retained().len(), count - 1);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records - 3);
    assert_eq!(job.submit(), Err(AdmissionError::Stale));
    assert!(job.observe_checkpoint_settlement().is_err());
    assert!(done(ops.review(scope).unwrap()).same(&actual));
}
#[test]
fn local_write_repair_retires_only_exact_failed_chain_after_observation() {
    let (ops, scope) = fixture();
    let before = acknowledged(ops.local_inventory(scope.clone()).unwrap());
    let lease = acknowledged(ops.renew_local(before.clone()).unwrap());
    done(
        ops.fixture_fault(scope.clone(), p::test_support::FaultPoint::LocalCommit)
            .unwrap(),
    );
    let job = ops
        .write_local(LocalWriteInput {
            lease,
            predecessor: 0,
            serial: 1,
            expected: before.local().clone(),
            local: sync::tests::local("repaired local"),
        })
        .unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    assert!(job.observe_local_write_settlement().is_err());
    job.retry_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    assert!(job.observe_local_write().is_err());
    let (actual, history, receipt) = job.observe_local_write_settlement().unwrap();
    assert_eq!(actual.local().composer.text, "repaired local");
    assert_eq!(history.failed_submissions().len(), 1);
    assert!(!history.operation_pending());
    let (_, _, duplicate) = job.observe_local_write_settlement().unwrap();
    assert!(receipt.acknowledge().is_ok());
    assert!(matches!(
        duplicate.acknowledge(),
        Err((AdmissionError::Stale, _))
    ));
    assert!(done(ops.local_inventory(scope).unwrap()).same(&actual));
    assert_eq!(job.submit(), Err(AdmissionError::Stale));
}
#[test]
fn pending_checkpoint_settlement_preserves_the_unrelated_send_family() {
    let (ops, scope, stream, writer, review) = checkpoint_fixture();
    let first = checkpoint(&ops, &writer, review, 1, "message to send");
    first.submit().unwrap();
    first.wait().unwrap();
    assert!(
        first.observe_checkpoint_settlement().is_err(),
        "clean remains clean"
    );
    let (review, ack) = first.observe_checkpoint().unwrap();
    assert!(ack.acknowledge().is_ok());
    let send = ops
        .consume_send(SendInput {
            stream,
            expected: review,
        })
        .unwrap();
    let consumed = done(send.clone());
    // Consume freezes the intent; the actual Prepare step creates its operation.
    assert!(consumed.row().state().pending().is_none());
    let prepared = send.prepare().unwrap();
    prepared.submit().unwrap();
    assert_eq!(prepared.wait().unwrap(), JobState::Ready);
    let pending = done(ops.review(scope.clone()).unwrap());
    assert!(pending.row().state().pending().is_some());
    assert_eq!(
        pending.row().state().consumption(),
        consumed.row().state().consumption()
    );
    let retained_send: Vec<_> = ops
        .retained()
        .into_iter()
        .map(|v| (v.family, v.generation))
        .collect();
    let job = checkpoint(
        &ops,
        &writer,
        pending.clone(),
        2,
        "newer input while sending",
    );
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    assert!(job.observe_checkpoint().is_err());
    let (actual, history, ack) = job.observe_checkpoint_settlement().unwrap();
    assert!(history.operation_pending());
    assert!(history.failed_submissions().is_empty());
    assert_eq!(actual.local().composer.text, "newer input while sending");
    assert_eq!(
        actual.row().state().pending(),
        pending.row().state().pending()
    );
    assert!(ack.acknowledge().is_ok());
    let retained_after: Vec<_> = ops
        .retained()
        .into_iter()
        .map(|v| (v.family, v.generation))
        .collect();
    assert_eq!(retained_after, retained_send);
    assert!(ops.resume_consume_send(&send.key()).is_ok());
    // Prepare owns the moved linear result; both retained family handles survive.
    assert_eq!(send.state(), JobState::Transferred);
    let resumed = ops.resume_send_prepared(&prepared.key()).unwrap();
    assert_eq!(resumed.state(), JobState::Ready);
    assert!(Arc::ptr_eq(&resumed.input(), &prepared.input()));
    assert!(
        send.prepare().is_ok(),
        "checkpoint retirement cannot retire Send"
    );
    assert!(done(ops.review(scope).unwrap()).same(&actual));
}
