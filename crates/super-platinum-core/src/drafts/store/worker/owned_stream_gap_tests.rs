//! Actual owned worker and SQLite; no socket or production paths.
use super::*;
fn fixture() -> (Operations, Scope) {
    let route = p::owned_store::OwnedRoute::fresh_fixture().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    route.commit(&review).unwrap().wait().unwrap();
    let ready = route
        .inspect()
        .unwrap()
        .wait()
        .unwrap()
        .open()
        .unwrap()
        .initialize_cold(&p::owned_store::FactoryIdentity::default())
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
    // A real destination enrollment establishes the team continuity row.
    // Acknowledge setup jobs so the tests measure only their own stream work.
    let local = acknowledged(ops.local_inventory(scope.clone()).unwrap());
    let _review = acknowledged(ops.enroll(local).unwrap());
    (ops, scope)
}
fn acknowledged<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (v, ack) = job.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    v
}
fn open(ops: &Operations, scope: &Scope) -> p::Stream {
    let expected = acknowledged(
        ops.stream_state(TeamInput {
            scope: scope.clone(),
        })
        .unwrap(),
    );
    acknowledged(
        ops.open_stream(OpenInput {
            scope: scope.clone(),
            expected,
        })
        .unwrap(),
    )
}
#[test]
fn ninety_six_real_stream_retirements_release_success_capacity_without_new_authority() {
    let (ops, scope) = fixture();
    let base = ops.retained().len();
    let records = ops.queue.diagnostics().unwrap().records.len();
    for _ in 0..96 {
        let stream = open(&ops, &scope);
        assert!(stream.is_live());
        let job = ops
            .mark_gap(GapInput {
                stream: stream.clone(),
                reason: p::GapReason::Disconnect,
            })
            .unwrap();
        assert!(!stream.is_live());
        assert!(job.observe_success().is_err());
        let key = job.key();
        drop(job);
        let job = ops.resume_mark_gap(&key).unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Ready);
        let (_, ack) = job.observe_success().unwrap();
        drop(ack);
        assert_eq!(ops.retained().len(), base + 1);
        assert!(job.observe_success().unwrap().1.acknowledge().is_ok());
        assert_eq!(ops.retained().len(), base);
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
        assert!(job.submit().is_err());
        assert!(ops.resume_mark_gap(&key).is_err());
    }
}
#[test]
fn gap_refusal_revokes_synchronously_and_retains_exact_opaque_input() {
    let (ops, scope) = fixture();
    let (foreign, _) = fixture();
    let stream = open(&ops, &scope);
    let refused = foreign
        .mark_gap(GapInput {
            stream: stream.clone(),
            reason: p::GapReason::SourceReplaced,
        })
        .err()
        .expect("refused");
    assert!(!stream.is_live());
    assert_eq!(refused.input.stream.account(), ops.account());
    assert!(foreign.retained().is_empty());
    let stream = open(&ops, &scope);
    let fence = ops.queue.worker.begin_fence().unwrap();
    let refused = ops
        .mark_gap(GapInput {
            stream: stream.clone(),
            reason: p::GapReason::Disconnect,
        })
        .err()
        .expect("refused");
    assert!(!stream.is_live());
    assert_eq!(refused.reason, AdmissionError::Fencing);
    ops.queue.worker.resume_admission(&fence).unwrap();
    // Explicit storage-only retry uses the returned exact input; no stream reopen.
    let job = ops.mark_gap(refused.input).unwrap();
    job.submit().unwrap();
    job.wait().unwrap();
    let (_, ack) = job.observe_success().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    assert!(ack.acknowledge().is_err());
    assert_eq!(ops.retained().len(), 1);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(job.observe_success().unwrap().1.acknowledge().is_ok());
}
