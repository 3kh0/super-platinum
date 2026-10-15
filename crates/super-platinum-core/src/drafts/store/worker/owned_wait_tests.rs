//! Actual Owned Worker/SQLite, no polling or fake result in these tests.
use super::*;

#[track_caller]
fn waited<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}
#[test]
fn actual_wait_terminal_allows_immediate_fixed_chain_transfer_and_record() {
    let f = Fixture::new();
    let send = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    send.submit().unwrap();
    assert_eq!(send.wait().unwrap(), JobState::Ready);
    assert!(
        send.result()
            .unwrap()
            .unwrap()
            .local()
            .composer
            .text
            .is_empty()
    );
    // No yield/retry helper: reply success alone is insufficient to transfer.
    let prepared = send.prepare().unwrap();
    prepared.submit().unwrap();
    assert_eq!(prepared.wait().unwrap(), JobState::Ready);
    let claim = prepared.claim().unwrap();
    claim.submit().unwrap();
    assert_eq!(claim.wait().unwrap(), JobState::Ready);
    let completion = claim.completion().unwrap();
    let actual = completion.take_claimed().unwrap();
    let operation = actual.operation().clone();
    completion
        .bind(MutationOutcome {
            outcome: p::Outcome::Sent {
                client_msg_id: operation.client_msg_id,
                draft_id: None,
                ts: "20.000001".into(),
            },
            known_until: 0,
        })
        .unwrap();
    completion.submit().unwrap();
    let result = completion.wait().unwrap().unwrap();
    assert!(result.row().state().pending().is_none());
    let resumed = f.ops.resume_mutation(&completion.key()).unwrap();
    assert!(resumed.wait().unwrap().unwrap().same(&result));
    assert!(matches!(resumed.take_claimed(), Err(AdmissionError::Stale)));
}
#[test]
fn unsent_job_wait_does_not_enqueue_and_resumed_wait_is_same_result() {
    let f = Fixture::new();
    let job = f.ops.review(f.scope.clone()).unwrap();
    let before = f.ops.queue.diagnostics().unwrap().sequence;
    assert_eq!(job.wait().unwrap(), JobState::Waiting);
    assert_eq!(f.ops.queue.diagnostics().unwrap().sequence, before);
    let key = job.key();
    let result = waited(job);
    let resumed = f.ops.resume_review(&key).unwrap();
    assert_eq!(resumed.wait().unwrap(), JobState::Ready);
    assert!(resumed.result().unwrap().unwrap().same(&result));
    assert_eq!(f.ops.queue.diagnostics().unwrap().sequence, before + 1);
}

#[test]
fn newer_checkpoint_before_prepare_sends_original_intent_and_keeps_newer() {
    let f = Fixture::new();
    let send = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    let consumed = waited(send.clone());
    let intent = consumed.row().state().consumption().unwrap().clone();
    let newer = sync::tests::local("newer before Prepare \u{65e5}\u{1f680}");
    let write = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.writer.clone(),
            serial: 2,
            expected: consumed.clone(),
            local: newer.clone(),
            direct: None,
        })
        .unwrap();
    write.submit().unwrap();
    assert_eq!(write.wait().unwrap(), JobState::Ready);
    let WriteOutcome::Applied(after_input) = write.write_result().unwrap().unwrap() else {
        panic!("actual canonical typing");
    };
    assert!(!after_input.same(&consumed));
    assert_eq!(after_input.row().state().consumption(), Some(&intent));
    let prepared = send.prepare().unwrap();
    prepared.submit().unwrap();
    assert_eq!(prepared.wait().unwrap(), JobState::Ready);
    let claim = prepared.claim().unwrap();
    claim.submit().unwrap();
    assert_eq!(claim.wait().unwrap(), JobState::Ready);
    let completion = claim.completion().unwrap();
    let claimed = completion.take_claimed().unwrap();
    let operation = claimed.operation().clone();
    assert_eq!(operation.kind, crate::drafts::sync::OperationKind::Send);
    assert_eq!(operation.snapshot.as_ref(), Some(&intent.snapshot));
    let current = waited(f.ops.review(f.scope.clone()).unwrap());
    assert_eq!(
        serde_json::to_value(current.local()).unwrap(),
        serde_json::to_value(&newer).unwrap()
    );
    completion
        .bind(MutationOutcome {
            outcome: p::Outcome::Sent {
                client_msg_id: operation.client_msg_id,
                draft_id: None,
                ts: "20.000001".into(),
            },
            known_until: 0,
        })
        .unwrap();
    completion.submit().unwrap();
    let recorded = completion.wait().unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(recorded.local()).unwrap(),
        serde_json::to_value(&newer).unwrap()
    );
    assert!(recorded.row().state().pending().is_none());
}

#[test]
fn dirty_actual_inventory_cannot_rebase_consumed_send_or_erase_alternate() {
    let f = Fixture::new();
    let send = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    let consumed = waited(send.clone());
    let alternate = sync::tests::local("retained alternate");
    let preserve = f
        .ops
        .preserve(PreserveInput {
            writer: f.writer.clone(),
            serial: 2,
            local: alternate,
        })
        .unwrap();
    preserve.submit().unwrap();
    assert_eq!(preserve.wait().unwrap(), JobState::Ready);
    assert!(matches!(
        preserve.write_result().unwrap(),
        Ok(WriteOutcome::Preserved(_))
    ));
    let before = waited(f.ops.review(f.scope.clone()).unwrap());
    assert!(!before.alternates().is_empty());
    assert_eq!(
        before.row().state().consumption(),
        consumed.row().state().consumption()
    );
    let prepared = send.prepare().unwrap();
    prepared.submit().unwrap();
    assert_eq!(prepared.wait().unwrap(), JobState::Failed(p::Error::Stale));
    let after = waited(f.ops.review(f.scope.clone()).unwrap());
    assert!(after.same(&before));
    assert!(after.row().state().pending().is_none());
    assert!(matches!(prepared.claim(), Err(AdmissionError::Incomplete)));
}

#[test]
fn actual_revoked_stream_cannot_rebase_newer_input() {
    let f = Fixture::new();
    let send = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    let consumed = waited(send.clone());
    f.stream.revoke();
    let prepared = send.prepare().unwrap();
    prepared.submit().unwrap();
    assert_eq!(prepared.wait().unwrap(), JobState::Failed(p::Error::Gap));
    let after = waited(f.ops.review(f.scope.clone()).unwrap());
    assert!(after.same(&consumed));
    assert!(after.row().state().pending().is_none());
}

#[test]
fn old_consumed_proof_cannot_prepare_a_different_actual_send_intent() {
    stale_consumed_replay(false);
}
#[test]
fn old_consumed_proof_cannot_prepare_after_actual_release_and_reenrollment() {
    stale_consumed_replay(true);
}
fn stale_consumed_replay(reenroll: bool) {
    let f = Fixture::new();
    let send = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    let consumed = waited(send.clone());
    let write = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.writer.clone(),
            serial: 2,
            expected: consumed.clone(),
            local: sync::tests::local("a second genuine send"),
            direct: None,
        })
        .unwrap();
    write.submit().unwrap();
    assert_eq!(write.wait().unwrap(), JobState::Ready);
    let prepared = send.prepare().unwrap();
    prepared.submit().unwrap();
    prepared.wait().unwrap();
    let claimed = prepared.claim().unwrap();
    claimed.submit().unwrap();
    claimed.wait().unwrap();
    let completion = claimed.completion().unwrap();
    let op = completion.take_claimed().unwrap().operation().clone();
    completion
        .bind(MutationOutcome {
            outcome: p::Outcome::Sent {
                client_msg_id: op.client_msg_id,
                draft_id: None,
                ts: "20.000001".into(),
            },
            known_until: 0,
        })
        .unwrap();
    completion.submit().unwrap();
    let mut first_result = completion.wait().unwrap().unwrap();
    // Positive Send creates a new local identity for the newer input. It must
    // establish its own local basis before a second real Consume is admitted.
    assert_ne!(
        first_result.row().state().client_msg_id(),
        consumed.row().state().client_msg_id()
    );
    assert_eq!(first_result.row().lineage().verified_epoch(), 0);
    assert_eq!(first_result.row().lineage().basis(), p::Basis::Unverified);
    assert!(first_result.row().state().acknowledged().is_none());
    assert!(first_result.row().state().consumption().is_none());
    if reenroll {
        let release = f
            .ops
            .release(ReleaseInput {
                expected: first_result,
                writer: f.writer.clone(),
            })
            .unwrap();
        waited(release.clone());
        let local = release.start_local().unwrap();
        local.submit().unwrap();
        let lease = local.wait().unwrap().unwrap();
        assert_eq!(lease.writer_id(), f.writer.id());
        let inventory = waited(f.ops.local_inventory(f.scope.clone()).unwrap());
        assert!(inventory.row().is_none());
        first_result = waited(f.ops.enroll(inventory).unwrap());
        assert!(!first_result.row().same_enrollment(consumed.row()));
    }
    // The real verifier checks LocalIssued origin for this exact identity;
    // do not copy the old identity's epoch or replace the retained old proof.
    first_result = waited(
        f.ops
            .verify_local(MutationInput {
                stream: f.stream.clone(),
                expected: first_result,
            })
            .unwrap(),
    );
    assert_eq!(
        first_result.row().lineage().verified_epoch(),
        f.stream.epoch()
    );
    assert_eq!(first_result.row().lineage().basis(), p::Basis::Local);
    let second = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: first_result,
        })
        .unwrap();
    let before = waited(second);
    assert_ne!(
        before.row().state().consumption(),
        consumed.row().state().consumption()
    );
    // A crate-private replay probe exercises the real executor guard. There is
    // deliberately no facade constructor accepting an arbitrary consumed proof.
    let stream = f.stream.clone();
    let replay = f
        .ops
        .queue
        .worker
        .hold_consumed_replay(consumed, stream)
        .unwrap();
    f.ops.queue.worker.submit_step(&replay).unwrap();
    assert_eq!(
        f.ops.queue.worker.wait_step(&replay).unwrap(),
        a::StepState::Failed(p::Error::Stale)
    );
    let after = waited(f.ops.review(f.scope.clone()).unwrap());
    assert!(before.same(&after));
    assert!(after.row().state().pending().is_none());
}

#[test]
fn completion_wait_returns_exact_old_failure_after_an_explicit_retry_succeeds() {
    let f = Fixture::new();
    let completion = f.send();
    let op = completion.take_claimed().unwrap().operation().clone();
    let fault = f
        .ops
        .queue
        .worker
        .hold_local_fault(
            f.scope.clone(),
            p::test_support::FaultPoint::MutationReceipt,
        )
        .unwrap();
    f.ops.queue.worker.submit_step(&fault).unwrap();
    assert_eq!(
        f.ops.queue.worker.wait_step(&fault).unwrap(),
        a::StepState::Ready
    );
    completion
        .bind(MutationOutcome {
            outcome: p::Outcome::Sent {
                client_msg_id: op.client_msg_id,
                draft_id: None,
                ts: "20.000001".into(),
            },
            known_until: 0,
        })
        .unwrap();
    completion.submit().unwrap();
    let retained = completion.clone();
    let (captured, received) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let old = retained.wait().unwrap();
        captured.send(()).unwrap();
        resume.recv().unwrap();
        old
    });
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    completion.retry_storage().unwrap();
    let current = completion.wait().unwrap().unwrap();
    release.send(()).unwrap();
    assert!(matches!(waiter.join().unwrap(), Err(p::Error::Storage)));
    assert!(current.row().state().pending().is_none());
    assert!(
        completion
            .result()
            .unwrap()
            .unwrap()
            .unwrap()
            .same(&current)
    );
    assert!(matches!(
        completion.take_claimed(),
        Err(AdmissionError::Stale)
    ));
}
