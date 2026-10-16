//! Actual fixed handoff lease and retained local FIFO, no synthetic capability.
use super::*;
fn waited<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}
fn lease(f: &Fixture) -> p::LocalWriterLease {
    let c = f.send();
    let op = c.take_claimed().unwrap().operation().clone();
    c.bind(MutationOutcome {
        outcome: p::Outcome::Sent {
            client_msg_id: op.client_msg_id,
            draft_id: None,
            ts: "20.000001".into(),
        },
        known_until: 0,
    })
    .unwrap();
    c.submit().unwrap();
    let review = c.wait().unwrap().unwrap();
    let release = f
        .ops
        .release(ReleaseInput {
            expected: review,
            writer: f.writer.clone(),
        })
        .unwrap();
    waited(release.clone());
    let local = release.start_local().unwrap();
    local.submit().unwrap();
    local.wait().unwrap().unwrap()
}
fn input(f: &Fixture, lease: p::LocalWriterLease) -> LocalWriteInput {
    let before = waited(f.ops.local_inventory(f.scope.clone()).unwrap());
    LocalWriteInput {
        predecessor: lease.initial_serial(),
        serial: lease.initial_serial() + 1,
        lease,
        expected: before.local().clone(),
        local: sync::tests::local("local \u{65e5}\u{1f680}"),
    }
}
#[test]
fn retained_local_write_rollback_and_lost_handle_retry_preserve_exact_lease_input() {
    let f = Fixture::new();
    let input = input(&f, lease(&f));
    let before = waited(f.ops.local_inventory(f.scope.clone()).unwrap());
    waited(
        f.ops
            .fixture_fault(f.scope.clone(), p::test_support::FaultPoint::LocalCommit)
            .unwrap(),
    );
    let job = f.ops.write_local(input.clone()).unwrap();
    let key = job.key();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    drop(job);
    assert!(waited(f.ops.local_inventory(f.scope.clone()).unwrap()).same(&before));
    let job = f.ops.resume_write_local(&key).unwrap();
    assert_eq!(job.input().lease.generation(), input.lease.generation());
    assert_eq!(job.input().serial, input.serial);
    assert_eq!(
        serde_json::to_value(&job.input().local).unwrap(),
        serde_json::to_value(&input.local).unwrap()
    );
    job.retry_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let Some(Ok(LocalOutcome::Applied(after))) = job.local_result() else {
        panic!("actual local write")
    };
    assert_eq!(
        serde_json::to_value(after.local()).unwrap(),
        serde_json::to_value(&input.local).unwrap()
    );
    assert_eq!(
        f.ops
            .retained()
            .iter()
            .filter(|v| v.family == "protected-write-local")
            .count(),
        1
    );
}
#[test]
fn old_generation_local_retry_cannot_mint_new_write_but_keeps_exact_failed_input() {
    let f = Fixture::new();
    let input = input(&f, lease(&f));
    waited(
        f.ops
            .fixture_fault(f.scope.clone(), p::test_support::FaultPoint::LocalCommit)
            .unwrap(),
    );
    let job = f.ops.write_local(input).unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    let before = waited(f.ops.local_inventory(f.scope.clone()).unwrap());
    let fence = f.ops.queue.worker.begin_fence().unwrap();
    f.ops.queue.worker.resume_admission(&fence).unwrap();
    assert_eq!(job.retry_storage(), Err(AdmissionError::Stale));
    assert_eq!(job.state(), JobState::Failed(p::Error::Storage));
    assert!(waited(f.ops.local_inventory(f.scope.clone()).unwrap()).same(&before));
}
#[test]
fn foreign_local_lease_refusal_returns_original_input_before_any_enqueue() {
    let f = Fixture::new();
    let other = Fixture::new();
    let input = input(&f, lease(&f));
    let before = other.ops.retained().len();
    let refused = match other.ops.write_local(input.clone()) {
        Err(e) => e,
        Ok(_) => panic!("foreign lease admitted"),
    };
    assert_eq!(refused.reason, AdmissionError::Stale);
    assert_eq!(refused.input.lease.generation(), input.lease.generation());
    assert_eq!(
        serde_json::to_value(refused.input.local).unwrap(),
        serde_json::to_value(input.local).unwrap()
    );
    assert_eq!(other.ops.retained().len(), before);
}
