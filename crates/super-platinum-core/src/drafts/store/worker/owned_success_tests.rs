//! Actual isolated Worker/SQLite; no network, configured path or renderer.
use super::*;
use p::owned_store::{FactoryIdentity, OwnedRoute};
#[path = "owned_writer_settlement_tests.rs"]
mod writer_settlement;
fn fixture() -> (Operations, Scope) {
    let route = OwnedRoute::fresh_fixture().unwrap();
    let before = route.inspect().unwrap().wait().unwrap();
    route.commit(&before).unwrap().wait().unwrap();
    let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
    let ready = opened
        .initialize_cold(&FactoryIdentity::default())
        .unwrap()
        .wait()
        .unwrap();
    let ops = ready.operations();
    let scope = Scope {
        account_id: ops.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    (ops, scope)
}
fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}
fn acknowledged<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (value, ack) = job.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    value
}
#[test]
fn more_than_64_actual_local_writes_release_both_bounded_books() {
    let (ops, scope) = fixture();
    let local = acknowledged(ops.local_inventory(scope.clone()).unwrap());
    let lease = acknowledged(ops.renew_local(local.clone()).unwrap());
    let mut prior = local.local().clone();
    let base = ops.retained().len();
    let records = ops.queue.diagnostics().unwrap().records.len();
    for serial in 1..=128 {
        let next = sync::tests::local(&format!("typing {serial} \u{65e5}\u{1f680}"));
        let job = ops
            .write_local(LocalWriteInput {
                lease: lease.clone(),
                predecessor: serial - 1,
                serial,
                expected: prior,
                local: next.clone(),
            })
            .unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Ready);
        let key = job.key();
        let old = job.clone();
        let (observed, ack) = job.observe_local_write().unwrap();
        assert_eq!(
            serde_json::to_value(observed.local()).unwrap(),
            serde_json::to_value(&next).unwrap()
        );
        assert!(ack.acknowledge().is_ok());
        assert_eq!(old.submit(), Err(AdmissionError::Stale));
        assert!(matches!(
            ops.resume_write_local(&key),
            Err(AdmissionError::Stale)
        ));
        assert!(matches!(
            old.local_result(),
            Some(Ok(LocalOutcome::Applied(_)))
        ));
        assert_eq!(ops.retained().len(), base);
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
        prior = next;
    }
    let actual = acknowledged(ops.local_inventory(scope).unwrap());
    assert_eq!(
        serde_json::to_value(actual.local()).unwrap(),
        serde_json::to_value(prior).unwrap()
    );
}
#[test]
fn more_than_64_actual_checkpoints_preserve_writer_and_monotonic_storage() {
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
    let mut review = done(
        ops.verify_local(MutationInput {
            stream,
            expected: review,
        })
        .unwrap(),
    );
    let writer = done(ops.register(review.clone()).unwrap());
    let base = ops.retained().len();
    let records = ops.queue.diagnostics().unwrap().records.len();
    for serial in 1..=128 {
        let value = sync::tests::local(&format!("checkpoint {serial}"));
        let old_revision = review.row().revision();
        let job = ops
            .checkpoint(CheckpointInput {
                writer: writer.clone(),
                serial,
                expected: review,
                local: value.clone(),
                direct: None,
            })
            .unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Ready);
        let (next, ack) = job.observe_checkpoint().unwrap();
        assert!(next.row().revision() > old_revision);
        assert_eq!(
            serde_json::to_value(next.local()).unwrap(),
            serde_json::to_value(value).unwrap()
        );
        assert!(ack.acknowledge().is_ok());
        review = next;
        assert_eq!(ops.retained().len(), base);
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
    }
    assert!(done(ops.review(scope).unwrap()).same(&review));
}
#[test]
fn held_and_unacknowledged_results_are_not_evicted_when_full() {
    let (ops, scope) = fixture();
    let observed = ops.local_inventory(scope.clone()).unwrap();
    observed.submit().unwrap();
    observed.wait().unwrap();
    let (_, ack) = observed.observe_success().unwrap();
    let held: Vec<_> = (0..63)
        .map(|_| ops.local_inventory(scope.clone()).unwrap())
        .collect();
    assert_eq!(ops.retained().len(), 64);
    assert!(held.iter().all(|j| j.observe_success().is_err()));
    let refused = match ops.local_inventory(scope.clone()) {
        Err(e) => e,
        Ok(_) => panic!("capacity"),
    };
    assert_eq!(refused.reason, AdmissionError::Capacity);
    assert_eq!(refused.input, scope);
    assert_eq!(ops.retained().len(), 64);
    assert!(ops.resume_local_inventory(&observed.key()).is_ok());
    assert!(ack.acknowledge().is_ok());
    assert!(ops.local_inventory(scope).is_ok());
    assert_eq!(ops.retained().len(), 64);
    assert!(
        held.iter()
            .all(|j| ops.resume_local_inventory(&j.key()).is_ok())
    );
}
#[test]
fn dropped_receipt_retains_success_and_second_receipt_cannot_replay() {
    let (ops, scope) = fixture();
    let job = ops.local_inventory(scope).unwrap();
    job.submit().unwrap();
    job.wait().unwrap();
    let (_, first) = job.observe_success().unwrap();
    drop(first);
    assert!(ops.resume_local_inventory(&job.key()).is_ok());
    let (_, a) = job.observe_success().unwrap();
    let (_, b) = job.observe_success().unwrap();
    assert!(a.acknowledge().is_ok());
    assert!(matches!(b.acknowledge(), Err((AdmissionError::Stale, _))));
    assert!(job.result().unwrap().is_ok());
}
#[test]
fn failed_review_stays_held_while_observed_enrollment_can_retire() {
    let (ops, scope) = fixture();
    let missing = ops.review(scope.clone()).unwrap();
    missing.submit().unwrap();
    assert!(matches!(missing.wait().unwrap(), JobState::Failed(_)));
    assert!(missing.observe_success().is_err());
    assert!(ops.resume_review(&missing.key()).is_ok());
    let local = acknowledged(ops.local_inventory(scope).unwrap());
    let enrolled = ops.enroll(local).unwrap();
    enrolled.submit().unwrap();
    enrolled.wait().unwrap();
    let (review, ack) = enrolled.observe_success().unwrap();
    assert_eq!(review.scope().account_id, ops.account());
    assert!(ops.resume_enroll(&enrolled.key()).is_ok());
    assert!(ack.acknowledge().is_ok());
    assert!(matches!(
        ops.resume_enroll(&enrolled.key()),
        Err(AdmissionError::Stale)
    ));
    assert!(ops.resume_review(&missing.key()).is_ok());
}
#[test]
fn observed_receipt_refuses_fence_and_retries_without_losing_custody() {
    let (ops, scope) = fixture();
    let job = ops.local_inventory(scope).unwrap();
    job.submit().unwrap();
    job.wait().unwrap();
    let (_, ack) = job.observe_success().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    let ack = match ack.acknowledge() {
        Err((AdmissionError::Fencing, a)) => a,
        _ => panic!("fence"),
    };
    assert!(ops.resume_local_inventory(&job.key()).is_ok());
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(ack.acknowledge().is_ok());
}
#[test]
fn accounting_capacity_refusal_retains_request_until_observed_success_is_acknowledged() {
    let (ops, scope) = fixture();
    let first = ops.local_inventory(scope.clone()).unwrap();
    first.submit().unwrap();
    first.wait().unwrap();
    let (_, ack) = first.observe_success().unwrap();
    let mut retained = Vec::new();
    let refused = loop {
        assert!(retained.len() < 64);
        let job = ops.local_inventory(scope.clone()).unwrap();
        match job.submit() {
            Ok(()) => {
                assert_eq!(job.wait().unwrap(), JobState::Ready);
                retained.push(job);
            }
            Err(AdmissionError::Capacity) => break job,
            Err(e) => panic!("unexpected refusal {e:?}"),
        }
    };
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), 64);
    assert!(refused.observe_success().is_err());
    assert!(ack.acknowledge().is_ok());
    refused.submit().unwrap();
    assert_eq!(refused.wait().unwrap(), JobState::Ready);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), 64);
    assert!(
        retained
            .iter()
            .all(|j| ops.resume_local_inventory(&j.key()).is_ok())
    );
}
