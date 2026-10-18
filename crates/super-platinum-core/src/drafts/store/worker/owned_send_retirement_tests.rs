//! Actual Send consume/prepare/claim/record, same Owned account beyond 64.
use super::*;
fn sent(c: &MutationCompletion) -> p::Review {
    let claim = c.take_claimed().unwrap();
    c.bind(MutationOutcome {
        outcome: p::Outcome::Sent {
            client_msg_id: claim.operation().client_msg_id.clone(),
            draft_id: None,
            ts: "20.000001".into(),
        },
        known_until: 0,
    })
    .unwrap();
    c.submit().unwrap();
    c.wait().unwrap().unwrap()
}
#[test]
fn ninety_six_sent_families_retire_both_counts_without_replay() {
    let mut f = Fixture::new();
    let count = f.ops.retained().len();
    let accounting = f.ops.queue.diagnostics().unwrap().records.len();
    let mut previous = None;
    for n in 0..96 {
        if n > 0 {
            let job = f
                .ops
                .checkpoint(CheckpointInput {
                    writer: f.writer.clone(),
                    serial: n + 1,
                    expected: f.review.clone(),
                    local: sync::tests::local("next ordinary Send"),
                    direct: None,
                })
                .unwrap();
            job.submit().unwrap();
            assert_eq!(job.wait().unwrap(), JobState::Ready);
            let (review, ack) = job.observe_checkpoint().unwrap();
            f.review = review;
            assert!(ack.acknowledge().is_ok());
            // Sent changed the protected identity. An edit does not inherit
            // the prior identity's verification; use the actual owned proof.
            assert_eq!(f.review.row().lineage().verified_epoch(), 0);
            f.review = acknowledged(
                f.ops
                    .verify_local(MutationInput {
                        stream: f.stream.clone(),
                        expected: f.review.clone(),
                    })
                    .unwrap(),
            );
            assert_eq!(f.review.row().lineage().verified_epoch(), f.stream.epoch());
        }
        let c = f.send();
        let before = sent(&c);
        let (observed, ack) = c.observe_sent().unwrap();
        assert!(observed.same(&before));
        let key = c.key();
        assert!(ack.acknowledge().is_ok());
        assert!(matches!(
            f.ops.resume_mutation(&key),
            Err(AdmissionError::Stale)
        ));
        assert!(matches!(c.submit(), Err(AdmissionError::Stale)));
        assert!(matches!(c.take_claimed(), Err(AdmissionError::Stale)));
        assert!(c.result().unwrap().unwrap().unwrap().same(&observed));
        assert_eq!(f.ops.retained().len(), count);
        assert_eq!(f.ops.queue.diagnostics().unwrap().records.len(), accounting);
        f.review = observed;
        previous = Some(c);
    }
    assert!(previous.unwrap().observe_sent().is_err());
}
#[test]
fn family_drop_fence_duplicate_and_held_child_preserve_custody() {
    let f = Fixture::new();
    let c = f.send();
    assert!(c.observe_sent().is_err());
    let held = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    let held_key = held.key();
    sent(&c);
    let count = f.ops.retained().len();
    let (_, dropped) = c.observe_sent().unwrap();
    drop(dropped);
    assert_eq!(f.ops.retained().len(), count);
    let (_, a) = c.observe_sent().unwrap();
    let (_, b) = c.observe_sent().unwrap();
    let fence = f.ops.queue.worker.begin_fence().unwrap();
    let (reason, a) = a.acknowledge().err().unwrap();
    assert_eq!(reason, AdmissionError::Fencing);
    assert_eq!(f.ops.retained().len(), count);
    f.ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(a.acknowledge().is_ok());
    assert!(b.acknowledge().is_err());
    assert_eq!(f.ops.retained().len(), count - 4);
    assert_eq!(
        f.ops.resume_consume_send(&held_key).unwrap().state(),
        JobState::Waiting
    );
}
#[test]
fn unknown_send_family_cannot_issue_success_acknowledgment() {
    let f = Fixture::new();
    let c = f.send();
    c.take_claimed().unwrap();
    c.bind(MutationOutcome {
        outcome: p::Outcome::Uncertain,
        known_until: 0,
    })
    .unwrap();
    c.submit().unwrap();
    c.wait().unwrap().unwrap();
    let count = f.ops.retained().len();
    assert!(c.observe_sent().is_err());
    assert_eq!(f.ops.retained().len(), count);
    assert!(f.ops.resume_mutation(&c.key()).is_ok());
}

fn acknowledged<T: Clone + Send + 'static, I: Send + Sync + 'static>(j: StorageJob<I, T>) -> T {
    j.submit().unwrap();
    assert_eq!(j.wait().unwrap(), JobState::Ready);
    let (value, ack) = j.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    value
}
#[test]
fn ninety_six_full_send_local_handoff_reenrollment_cycles_release_all_success_slots() {
    let mut f = Fixture::new();
    let count = f.ops.retained().len();
    let accounting = f.ops.queue.diagnostics().unwrap().records.len();
    for n in 0..96 {
        let c = f.send();
        sent(&c);
        let (review, ack) = c.observe_sent().unwrap();
        assert!(ack.acknowledge().is_ok());
        let release = f
            .ops
            .release(ReleaseInput {
                expected: review,
                writer: f.writer.clone(),
            })
            .unwrap();
        release.submit().unwrap();
        assert_eq!(release.wait().unwrap(), JobState::Ready);
        // A Ready parent still has a reserved child; generic single-step
        // acknowledgment must not discard that unsubmitted continuation.
        assert!(matches!(
            release.observe_success(),
            Err(AdmissionError::Unsupported)
        ));
        let local = release.start_local().unwrap();
        assert!(local.observe_handoff().is_err());
        local.submit().unwrap();
        local.wait().unwrap().unwrap();
        let (lease, ack) = local.observe_handoff().unwrap();
        let key = local.key();
        assert_eq!(lease.writer_id(), f.writer.id());
        assert!(ack.acknowledge().is_ok());
        assert!(matches!(
            f.ops.resume_local(&key),
            Err(AdmissionError::Stale)
        ));
        assert!(local.submit().is_err());
        assert!(release.start_local().is_err());
        // Use the actual returned lease after slot retirement, rather than
        // reconstructing a lease from IDs or manufacturing an ordinary store.
        let before_write = acknowledged(f.ops.local_inventory(f.scope.clone()).unwrap());
        assert!(before_write.row().is_none());
        let write = f
            .ops
            .write_local(LocalWriteInput {
                lease: lease.clone(),
                predecessor: lease.initial_serial(),
                serial: lease.initial_serial() + 1,
                expected: before_write.local().clone(),
                local: sync::tests::local("next full Send cycle"),
            })
            .unwrap();
        write.submit().unwrap();
        assert_eq!(write.wait().unwrap(), JobState::Ready);
        let (inventory, ack) = write.observe_local_write().unwrap();
        assert!(ack.acknowledge().is_ok());
        let enrolled = acknowledged(f.ops.enroll(inventory).unwrap());
        f.review = acknowledged(
            f.ops
                .verify_local(MutationInput {
                    stream: f.stream.clone(),
                    expected: enrolled,
                })
                .unwrap(),
        );
        f.writer = acknowledged(f.ops.register(f.review.clone()).unwrap());
        let job = f
            .ops
            .checkpoint(CheckpointInput {
                writer: f.writer.clone(),
                serial: 1,
                expected: f.review.clone(),
                local: sync::tests::local("next full Send cycle"),
                direct: None,
            })
            .unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Ready);
        let (review, ack) = job.observe_checkpoint().unwrap();
        f.review = review;
        assert!(ack.acknowledge().is_ok());
        assert_eq!(f.ops.retained().len(), count, "cycle {n}");
        assert_eq!(
            f.ops.queue.diagnostics().unwrap().records.len(),
            accounting,
            "cycle {n}"
        );
    }
}
