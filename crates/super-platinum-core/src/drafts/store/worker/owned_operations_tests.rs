//! Public facade over real temporary migration/cold/Worker/SQLite. No HTTP.
use super::*;
use p::owned_store::{FactoryIdentity, OwnedRoute};
use std::time::{Duration, Instant};
fn until<T>(mut f: impl FnMut() -> Result<T, AdmissionError>) -> T {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match f() {
            Ok(v) => return v,
            Err(AdmissionError::Incomplete) => {
                assert!(Instant::now() < end, "actual Worker timeout");
                std::thread::yield_now();
            }
            Err(e) => panic!("admission {e:?}"),
        }
    }
}
fn done<I: Send + Sync + 'static, T: Send + Clone + 'static>(j: StorageJob<I, T>) -> T {
    j.submit().unwrap();
    until(|| match j.state() {
        JobState::Waiting => Err(AdmissionError::Incomplete),
        JobState::Ready => Ok(j.result().unwrap().unwrap()),
        v => panic!("actual job {v:?}"),
    })
}
fn ready<I, T>(j: &StorageJob<I, T>) {
    until(|| match j.state() {
        JobState::Waiting => Err(AdmissionError::Incomplete),
        JobState::Ready => Ok(()),
        v => panic!("actual ticket {v:?}"),
    })
}
struct Fixture {
    ops: Operations,
    scope: Scope,
    stream: p::Stream,
    writer: p::Writer,
    review: p::Review,
}
impl Fixture {
    fn new() -> Self {
        let route = OwnedRoute::fresh_fixture().unwrap();
        let first = route.inspect().unwrap().wait().unwrap();
        route.commit(&first).unwrap().wait().unwrap();
        let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
        let initialized = opened
            .initialize_cold(&FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
        let ops = initialized.operations();
        let scope = Scope {
            account_id: ops.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        };
        let local = done(ops.local_inventory(scope.clone()).unwrap());
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
        let job = ops
            .checkpoint(CheckpointInput {
                writer: writer.clone(),
                serial: 1,
                expected: review,
                local: sync::tests::local("exact \u{65e5}\u{1f680}"),
                direct: None,
            })
            .unwrap();
        job.submit().unwrap();
        let WriteOutcome::Applied(review) =
            until(|| job.write_result().ok_or(AdmissionError::Incomplete)).unwrap()
        else {
            panic!("checkpoint")
        };
        Self {
            ops,
            scope,
            stream,
            writer,
            review,
        }
    }
    fn send(&self) -> MutationCompletion {
        let consumed = self
            .ops
            .consume_send(SendInput {
                stream: self.stream.clone(),
                expected: self.review.clone(),
            })
            .unwrap();
        consumed.submit().unwrap();
        ready(&consumed);
        assert!(
            consumed
                .result()
                .unwrap()
                .unwrap()
                .local()
                .composer
                .text
                .is_empty()
        );
        let prepared = until(|| consumed.prepare());
        prepared.submit().unwrap();
        ready(&prepared);
        let claimed = until(|| prepared.claim());
        claimed.submit().unwrap();
        ready(&claimed);
        until(|| claimed.completion())
    }
}
#[test]
fn public_initialized_facade_transfers_actual_claim_once_and_records() {
    let f = Fixture::new();
    let c = f.send();
    let claim = c.take_claimed().unwrap();
    assert_eq!(
        claim.operation().kind,
        crate::drafts::sync::OperationKind::Send
    );
    let op = claim.operation().clone();
    assert!(matches!(c.take_claimed(), Err(AdmissionError::Stale)));
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
    let review = until(|| c.result()?.ok_or(AdmissionError::Incomplete)).unwrap();
    assert!(review.row().state().pending().is_none());
    assert!(c.retained_outcome().is_some());
}
#[test]
fn opaque_inventory_recovers_last_unsent_register_and_same_result() {
    let f = Fixture::new();
    let j = f.ops.register(f.review.clone()).unwrap();
    let key = j.key();
    drop(j);
    let j = f.ops.resume_register(&key).unwrap();
    let writer = done(j.clone());
    drop(j);
    let keys = f.ops.retained();
    let mut matches = 0;
    for k in keys {
        if k.family == "protected-register-writer" {
            let j = f.ops.resume_register(&k.key).unwrap();
            if j.result().is_some_and(|r| r.unwrap().id() == writer.id()) {
                matches += 1;
            }
        }
    }
    assert_eq!(matches, 1);
}
#[test]
fn foreign_route_key_and_wrong_family_cannot_extract_or_submit() {
    let f = Fixture::new();
    let other = Fixture::new();
    let c = f.send();
    assert!(matches!(
        other.ops.resume_mutation(&c.key()),
        Err(AdmissionError::Stale)
    ));
    assert!(matches!(
        f.ops.resume_local(&c.key()),
        Err(AdmissionError::Stale)
    ));
    assert!(matches!(
        f.ops.resume_register(&c.key()),
        Err(AdmissionError::Stale)
    ));
    assert!(c.take_claimed().is_ok());
}
#[test]
fn fenced_claim_remains_recoverable_but_cannot_dispatch() {
    let f = Fixture::new();
    let c = f.send();
    let key = c.key();
    drop(c);
    let fence = f.ops.queue.worker.begin_fence().unwrap();
    f.ops.queue.worker.resume_admission(&fence).unwrap();
    let c = f.ops.resume_mutation(&key).unwrap();
    assert!(matches!(c.take_claimed(), Err(AdmissionError::Stale)));
    c.cancel_before_dispatch().unwrap();
    c.submit().unwrap();
    let review = until(|| c.result()?.ok_or(AdmissionError::Incomplete)).unwrap();
    assert!(matches!(
        review.row().state().workflow(),
        crate::drafts::sync::Workflow::Uncertain(_)
    ));
}
#[test]
fn raw_refusal_returns_exact_second_outcome_and_retains_first() {
    let f = Fixture::new();
    let c = f.send();
    let _claim = c.take_claimed().unwrap();
    c.bind(MutationOutcome {
        outcome: p::Outcome::Uncertain,
        known_until: 123,
    })
    .unwrap();
    let refused = c
        .bind(MutationOutcome {
            outcome: p::Outcome::Rejected,
            known_until: 999,
        })
        .unwrap_err();
    assert_eq!(refused.reason, AdmissionError::Stale);
    assert_eq!(refused.input.known_until, 999);
    assert_eq!(c.retained_outcome().unwrap().known_until, 123);
}
#[test]
fn public_release_handoff_retains_newer_text_and_actual_lease() {
    let f = Fixture::new();
    let c = f.send();
    let claim = c.take_claimed().unwrap();
    let current = done(f.ops.review(f.scope.clone()).unwrap());
    let newer = sync::tests::local("newer \u{65e5}\u{1f680}");
    let j = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.writer.clone(),
            serial: 2,
            expected: current,
            local: newer.clone(),
            direct: None,
        })
        .unwrap();
    j.submit().unwrap();
    assert!(matches!(
        until(|| j.write_result().ok_or(AdmissionError::Incomplete)),
        Ok(WriteOutcome::Applied(_))
    ));
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
    let review = until(|| c.result()?.ok_or(AdmissionError::Incomplete)).unwrap();
    let r = f
        .ops
        .release(ReleaseInput {
            writer: f.writer.clone(),
            expected: review,
        })
        .unwrap();
    r.submit().unwrap();
    ready(&r);
    let local = until(|| r.start_local());
    let key = local.key();
    local.submit().unwrap();
    drop(local);
    drop(r);
    let local = f.ops.resume_local(&key).unwrap();
    let lease = until(|| local.result()?.ok_or(AdmissionError::Incomplete)).unwrap();
    assert_eq!(lease.writer_id(), f.writer.id());
    assert!(matches!(
        f.ops.resume_mutation(&key),
        Err(AdmissionError::Stale)
    ));
    let inventory = done(f.ops.local_inventory(f.scope.clone()).unwrap());
    assert!(inventory.row().is_none());
    assert_eq!(
        serde_json::to_value(inventory.local()).unwrap(),
        serde_json::to_value(newer).unwrap()
    );
}
#[test]
fn generic_job_type_cannot_change_actual_resume_input_family() {
    let f = Fixture::new();
    let j = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.writer.clone(),
            serial: 2,
            expected: f.review.clone(),
            local: sync::tests::local("retained"),
            direct: None,
        })
        .unwrap();
    let key = j.key();
    let input = j.input();
    drop(j);
    assert!(matches!(
        f.ops.resume_preserve(&key),
        Err(AdmissionError::Stale)
    ));
    let j = f.ops.resume_checkpoint(&key).unwrap();
    assert!(Arc::ptr_eq(&input, &j.input()));
    j.submit().unwrap();
    assert!(matches!(
        until(|| j.write_result().ok_or(AdmissionError::Incomplete)),
        Ok(WriteOutcome::Applied(_))
    ));
}

#[path = "owned_wait_tests.rs"]
mod waiting;

#[path = "owned_inventory_tests.rs"]
mod inventory;

#[path = "owned_local_fifo_tests.rs"]
mod local_fifo;
