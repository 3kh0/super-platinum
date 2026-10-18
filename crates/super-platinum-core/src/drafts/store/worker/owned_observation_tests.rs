//! Actual retained worker/SQLite observations; no socket or caller-made ticket.
use super::*;
use crate::drafts::sync::{DraftObservation, ObservationKind, RemoteDraft};

mod fixture {
    use super::super::*;
    use crate::drafts::store::worker::admission::ObservationInput;
    use crate::drafts::store::worker::admission::*;
    mod support {
        include!("completion_test_support.rs");
    }
    pub(super) fn setup() -> (
        Operations,
        p::Stream,
        p::Review,
        crate::drafts::sync::RemoteDraft,
    ) {
        let f = support::Fixture::new();
        let (review, remote) = f.remote();
        (
            Operations::from_initialized(f.queue.clone()),
            f.stream,
            review,
            remote,
        )
    }
}
fn event(remote: &RemoteDraft, number: u64) -> DraftObservation {
    let mut remote = remote.clone();
    remote.last_updated_ts = format!("{}.000001", 20 + number);
    DraftObservation {
        scope: remote.scope.clone(),
        draft_id: remote.id.clone(),
        client_msg_id: remote.client_msg_id.clone(),
        event_ts: format!("{}.000002", 20 + number),
        kind: ObservationKind::Active(remote),
    }
}
fn completion(
    ops: &Operations,
    input: ObservationInput,
) -> (ObservationReserved, ObservationCompletion) {
    let reserved = ops.observe(input).unwrap();
    reserved.submit().unwrap();
    assert_eq!(reserved.wait().unwrap(), JobState::Ready);
    let completion = reserved.finish().unwrap();
    (reserved, completion)
}
fn sql(ops: &Operations, text: &str) {
    rusqlite::Connection::open(ops.queue.owner.path())
        .unwrap()
        .execute_batch(text)
        .unwrap();
}
#[test]
fn ninety_six_observation_families_release_both_books_only_after_projection_ack() {
    let (ops, stream, mut review, remote) = fixture::setup();
    let slots = ops.retained().len();
    let records = ops.queue.diagnostics().unwrap().records.len();
    let mut previous: Option<ObservationOutcome> = None;
    for n in 0..96 {
        let observed_event = event(&remote, n);
        let reserved = ops
            .observe(ObservationInput::new(
                stream.clone(),
                review.clone(),
                observed_event.clone(),
            ))
            .unwrap();
        let root_key = reserved.key();
        assert!(reserved.finish().is_err());
        assert_eq!(reserved.wait().unwrap(), JobState::Waiting);
        drop(reserved);
        let reserved = ops.resume_observation_reserved(&root_key).unwrap();
        assert_eq!(reserved.input().event(), &observed_event);
        reserved.submit().unwrap();
        assert_eq!(reserved.wait().unwrap(), JobState::Ready);
        let c = reserved.finish().unwrap();
        let key = c.key();
        drop(c);
        let c = ops.resume_observation_completion(&key).unwrap();
        assert!(c.observe_success().is_err());
        c.submit().unwrap();
        let result = c.wait().unwrap().unwrap();
        assert!(result.before().same(&review));
        assert_eq!(
            result.after().row().state().observed_event_ts(&remote.id),
            Some(observed_event.event_ts.as_str())
        );
        assert_eq!(ops.retained().len(), slots + 2);
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records + 2);
        // A host taking/waiting a result has not yet handled its projection.
        assert!(
            c.result()
                .unwrap()
                .unwrap()
                .unwrap()
                .after()
                .same(result.after())
        );
        let (applied, ack) = c.observe_success().unwrap();
        assert!(ack.matches(&result));
        assert!(ack.matches(&applied.clone()));
        if let Some(old) = &previous {
            assert!(!ack.matches(old));
            assert!(
                ops.queue
                    .worker
                    .observe_observation_family(&c.held, Some(&old.value), true)
                    .is_err()
            );
            assert_eq!(ops.retained().len(), slots + 2);
        }
        previous = Some(result);
        review = applied.after().clone();
        assert!(ack.acknowledge().is_ok());
        assert!(ops.resume_observation_reserved(&root_key).is_err());
        assert!(ops.resume_observation_completion(&key).is_err());
        assert!(reserved.submit().is_err());
        assert!(reserved.finish().is_err());
        assert!(c.submit().is_err());
        assert!(c.retry_storage().is_err());
        assert!(c.observe_success().is_err());
        assert!(c.result().unwrap().unwrap().unwrap().after().same(&review));
        assert_eq!(ops.retained().len(), slots);
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
    }
}
#[test]
fn dropped_duplicate_and_fenced_receipts_preserve_exact_completed_family() {
    let (ops, stream, review, remote) = fixture::setup();
    let input = ObservationInput::new(stream, review, event(&remote, 0));
    let (_, c) = completion(&ops, input.clone());
    // This unrelated held family must survive the completed family's retirement.
    let held = ops.observe(input).unwrap();
    let held_key = held.key();
    c.submit().unwrap();
    c.wait().unwrap().unwrap();
    let count = ops.retained().len();
    let (_, dropped) = c.observe_success().unwrap();
    drop(dropped);
    assert_eq!(ops.retained().len(), count);
    let (_, a) = c.observe_success().unwrap();
    let (_, b) = c.observe_success().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    let (error, a) = a.acknowledge().err().unwrap();
    assert_eq!(error, AdmissionError::Fencing);
    assert_eq!(ops.retained().len(), count);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(a.acknowledge().is_ok());
    assert!(b.acknowledge().is_err());
    assert_eq!(ops.retained().len(), count - 2);
    assert_eq!(
        ops.resume_observation_reserved(&held_key).unwrap().state(),
        JobState::Waiting
    );
}
#[test]
fn held_capacity_and_foreign_or_wrong_family_keys_never_evict_or_admit() {
    let (ops, stream, review, remote) = fixture::setup();
    let (foreign, _, _, _) = fixture::setup();
    let input = ObservationInput::new(stream, review.clone(), event(&remote, 0));
    assert!(matches!(
        foreign.observe(input.clone()),
        Err(AdmissionRefused {
            reason: AdmissionError::Stale,
            ..
        })
    ));
    let first = ops.observe(input.clone()).unwrap();
    assert!(foreign.resume_observation_reserved(&first.key()).is_err());
    assert!(foreign.resume_observation_completion(&first.key()).is_err());
    assert!(ops.resume_observation_completion(&first.key()).is_err());
    let review_job = ops.review(review.scope().clone()).unwrap();
    assert!(ops.resume_observation_reserved(&review_job.key()).is_err());
    let records = ops.queue.diagnostics().unwrap().records.len();
    let mut held = vec![first];
    loop {
        match ops.observe(input.clone()) {
            Ok(job) => held.push(job),
            Err(refused) => {
                assert_eq!(refused.reason, AdmissionError::Capacity);
                assert_eq!(refused.input.event(), input.event());
                assert!(refused.input.expected().same(input.expected()));
                break;
            }
        }
    }
    assert!(ops.retained().len() <= 64);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
    for job in held {
        assert_eq!(
            ops.resume_observation_reserved(&job.key()).unwrap().state(),
            JobState::Waiting
        );
    }
}
#[test]
fn actual_storage_failure_and_retry_keep_all_original_evidence() {
    let (ops, stream, review, remote) = fixture::setup();
    let (_, c) = completion(
        &ops,
        ObservationInput::new(stream, review, event(&remote, 0)),
    );
    sql(
        &ops,
        "CREATE TRIGGER observation_fail BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END;",
    );
    c.submit().unwrap();
    assert!(matches!(c.wait().unwrap(), Err(p::Error::Storage)));
    let count = ops.retained().len();
    assert!(c.observe_success().is_err());
    sql(&ops, "DROP TRIGGER observation_fail;");
    c.retry_storage().unwrap();
    let result = c.wait().unwrap().unwrap();
    assert_eq!(
        result.after().row().state().observed_event_ts(&remote.id),
        Some("20.000002")
    );
    // A successful retry does not erase the failed first attempt.
    assert!(c.observe_success().is_err());
    assert_eq!(ops.retained().len(), count);
    assert!(ops.resume_observation_completion(&c.key()).is_ok());
}
#[test]
fn actual_malformed_observation_refuses_completion_without_fabricated_success() {
    let (ops, stream, review, remote) = fixture::setup();
    let mut invalid = event(&remote, 0);
    invalid.scope.key.channel = "COTHER".into();
    let root = ops
        .observe(ObservationInput::new(stream, review, invalid.clone()))
        .unwrap();
    root.submit().unwrap();
    assert!(matches!(root.wait().unwrap(), JobState::Failed(_)));
    assert_eq!(root.input().event(), &invalid);
    assert!(root.finish().is_err());
    assert!(ops.resume_observation_reserved(&root.key()).is_ok());
}
#[test]
fn issued_finish_survives_fence_but_ack_waits_for_admission_resume() {
    let (ops, stream, review, remote) = fixture::setup();
    let (root, c) = completion(
        &ops,
        ObservationInput::new(stream, review, event(&remote, 0)),
    );
    let fence = ops.queue.worker.begin_fence().unwrap();
    c.submit().unwrap();
    let result = c.wait().unwrap().unwrap();
    assert_eq!(
        result.after().row().state().observed_event_ts(&remote.id),
        Some("20.000002")
    );
    assert!(c.observe_success().is_err());
    ops.queue.worker.resume_admission(&fence).unwrap();
    let resumed = ops.resume_observation_completion(&c.key()).unwrap();
    // Repeating finish attaches the same existing child; submit cannot replay.
    let same = root.finish().unwrap();
    same.submit().unwrap();
    let (_, ack) = resumed.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    assert!(same.observe_success().is_err());
}
