//! Actual owned worker and SQLite. No network, caller-issued ticket or local proof substitute.
use super::*;
use crate::drafts::store::sync::RemoteChoice;
use crate::drafts::sync::RemoteDraft;
mod fixture {
    use super::super::*;
    use crate::drafts::store::worker::admission::BaselineInput;
    use crate::drafts::store::worker::admission::*;
    use crate::drafts::sync::RemoteDraft;
    mod support {
        include!("completion_test_support.rs");
    }
    pub(super) fn setup() -> (Operations, p::Stream, p::Review, RemoteDraft) {
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
fn sql(ops: &Operations, text: &str) {
    rusqlite::Connection::open(ops.queue.owner.path())
        .unwrap()
        .execute_batch(text)
        .unwrap();
}
fn read(remote: &RemoteDraft) -> BaselineRead {
    BaselineRead {
        response: p::BaselineResponse::Positive {
            raw: serde_json::json!({"ok":true,"unknown_root":{"keep":[1,2]},"draft":{
                "id":remote.id,"client_msg_id":remote.client_msg_id,"last_updated_ts":"11.000001",
                "team_id":"T1","user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,
                "blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,"destinations":remote.snapshot.destinations
            }}),
            user: "U1".into(),
        },
        known_until: 0,
    }
}
fn reserve(ops: &Operations, stream: p::Stream, review: p::Review) -> BaselineReserved {
    let r = ops
        .baseline(BaselineInput::new(stream, review, 1000))
        .unwrap();
    r.submit().unwrap();
    assert_eq!(r.wait().unwrap(), JobState::Ready);
    r
}
fn claim(r: &BaselineReserved) -> BaselineCompletion {
    let c = r.claim().unwrap();
    c.submit().unwrap();
    assert_eq!(c.wait().unwrap(), JobState::Ready);
    c.completion().unwrap()
}
fn current(ops: &Operations, review: &p::Review) -> p::Review {
    let job = ops.review(review.scope().clone()).unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (v, ack) = job.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    v
}
#[test]
fn dropped_handles_exact_positive_retention_ack_then_explicit_choice() {
    let (ops, stream, review, remote) = fixture::setup();
    let slots = ops.retained().len();
    let records = ops.queue.diagnostics().unwrap().records.len();
    let r = ops
        .baseline(BaselineInput::new(stream, review.clone(), 1000))
        .unwrap();
    let rk = r.key();
    assert!(r.claim().is_err());
    drop(r);
    let r = ops.resume_baseline_reserved(&rk).unwrap();
    r.submit().unwrap();
    assert_eq!(r.wait().unwrap(), JobState::Ready);
    let claimed = r.claim().unwrap();
    let ck = claimed.key();
    drop(claimed);
    let claimed = ops.resume_baseline_claimed(&ck).unwrap();
    claimed.submit().unwrap();
    assert_eq!(claimed.wait().unwrap(), JobState::Ready);
    let c = claimed.completion().unwrap();
    let key = c.key();
    drop(c);
    let c = ops.resume_baseline_completion(&key).unwrap();
    assert_eq!(c.state().unwrap(), BaselineState::Ready);
    let request = c.take_request().unwrap();
    assert_eq!(c.state().unwrap(), BaselineState::Requested);
    assert_eq!(request.scope(), review.scope());
    assert_eq!(request.draft_id(), remote.id);
    assert!(c.take_request().is_err());
    assert!(c.observe_success().is_err());
    let raw = read(&remote);
    c.bind(raw.clone()).unwrap();
    assert!(c.bind(raw.clone()).is_err());
    assert!(c.retained_read().unwrap().response == raw.response);
    c.submit().unwrap();
    let result = c.wait().unwrap().unwrap();
    assert!(current(&ops, &review).same(&review));
    let evidence = result.evidence().unwrap();
    assert!(evidence.review().same(&review));
    let (_, dropped) = c.observe_success().unwrap();
    drop(dropped);
    assert_eq!(ops.retained().len(), slots + 3);
    let (same, ack) = c.observe_success().unwrap();
    assert!(ack.matches(&result));
    assert!(ack.matches(&same));
    let (_, duplicate) = c.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    assert!(duplicate.acknowledge().is_err());
    assert_eq!(ops.retained().len(), slots);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
    assert!(ops.resume_baseline_completion(&key).is_err());
    assert!(r.submit().is_err());
    assert!(c.submit().is_err());
    // The memory family is gone, but exact durable evidence still requires and
    // accepts an explicit choice. A caller cannot synthesize it from raw JSON.
    let finish = ops
        .finish_baseline(BaselineFinishInput::new(evidence, RemoteChoice::Remote))
        .unwrap();
    let fk = finish.key();
    drop(finish);
    let finish = ops.resume_finish_baseline(&fk).unwrap();
    finish.submit().unwrap();
    assert_eq!(finish.wait().unwrap(), JobState::Ready);
    let (settled, ack) = finish.observe_success().unwrap();
    assert!(!settled.same(&review));
    assert!(ack.acknowledge().is_ok());
    assert!(current(&ops, &review).same(&settled));
    assert!(finish.submit().is_err());
    assert_eq!(ops.retained().len(), slots);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
}
#[test]
fn deferred_claim_resumes_same_ticket_and_accounts_every_explicit_attempt() {
    let (ops, stream, review, _) = fixture::setup();
    let slots = ops.retained().len();
    let records = ops.queue.diagnostics().unwrap().records.len();
    let r = reserve(&ops, stream, review);
    sql(
        &ops,
        "INSERT INTO composer_draft_known_read_limit(team,until_ms) VALUES('T1',50000) ON CONFLICT(team) DO UPDATE SET until_ms=50000;",
    );
    let c = claim(&r);
    let key = c.key();
    assert_eq!(c.deferred_until(), Some(50000));
    assert_eq!(c.state().unwrap(), BaselineState::Deferred(50000));
    assert!(c.take_request().is_err());
    assert!(c.cancel_before_dispatch().is_err());
    assert!(c.resume_deferred(49999).is_err());
    assert_eq!(ops.retained().len(), slots + 3);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records + 2);
    drop(c);
    let c = ops.resume_baseline_completion(&key).unwrap();
    sql(
        &ops,
        "UPDATE composer_draft_known_read_limit SET until_ms=60000 WHERE team='T1';",
    );
    c.resume_deferred(50000).unwrap();
    assert_eq!(
        c.state().unwrap(),
        BaselineState::ClaimPending {
            now_ms: 50000,
            submitted: true
        }
    );
    c.resume_deferred(50000).unwrap();
    assert!(c.resume_deferred(50001).is_err());
    assert_eq!(c.wait_deferred().unwrap().unwrap(), Some(60000));
    assert!(c.resume_deferred(59999).is_err());
    c.resume_deferred(60000).unwrap();
    assert_eq!(c.wait_deferred().unwrap().unwrap(), None);
    assert_eq!(c.state().unwrap(), BaselineState::Ready);
    assert!(c.resume_deferred(60000).is_err());
    assert_eq!(c.deferred_until(), None);
    c.cancel_before_dispatch().unwrap();
    c.submit().unwrap();
    assert!(c.wait().unwrap().unwrap().evidence().is_none());
    assert_eq!(ops.retained().len(), slots + 3);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records + 5);
    assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
    assert_eq!(ops.retained().len(), slots);
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), records);
}
#[test]
fn foreign_owner_and_wrong_family_cannot_reattach_or_retire() {
    let (ops, stream, review, _) = fixture::setup();
    let (foreign, _, _, _) = fixture::setup();
    let input = BaselineInput::new(stream, review.clone(), 1000);
    assert!(matches!(
        foreign.baseline(input.clone()),
        Err(AdmissionRefused {
            reason: AdmissionError::Stale,
            ..
        })
    ));
    let r = reserve(&ops, input.stream().clone(), review.clone());
    assert!(foreign.resume_baseline_reserved(&r.key()).is_err());
    assert!(ops.resume_baseline_claimed(&r.key()).is_err());
    let c = claim(&r);
    assert!(foreign.resume_baseline_completion(&c.key()).is_err());
    assert!(ops.resume_mutation(&c.key()).is_err());
    let wrong = ops.review(review.scope().clone()).unwrap();
    assert!(ops.resume_baseline_completion(&wrong.key()).is_err());
    c.cancel_before_dispatch().unwrap();
    assert!(c.take_request().is_err());
    c.submit().unwrap();
    c.wait().unwrap().unwrap();
    let (_, ack) = c.observe_success().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    let (error, ack) = ack.acknowledge().err().unwrap();
    assert_eq!(error, AdmissionError::Fencing);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(ack.acknowledge().is_ok());
    assert!(ops.resume_review(&wrong.key()).is_ok());
}
#[test]
fn late_positive_after_stream_gap_retains_evidence_but_cannot_apply() {
    let (ops, stream, review, remote) = fixture::setup();
    let c = claim(&reserve(&ops, stream.clone(), review.clone()));
    c.take_request().unwrap();
    let gap = ops
        .mark_gap(GapInput {
            stream,
            reason: p::GapReason::Disconnect,
        })
        .unwrap();
    gap.submit().unwrap();
    assert_eq!(gap.wait().unwrap(), JobState::Ready);
    let raw = read(&remote);
    c.bind(raw.clone()).unwrap();
    c.submit().unwrap();
    let result = c.wait().unwrap().unwrap();
    assert!(c.retained_read().unwrap().response == raw.response);
    assert!(current(&ops, &review).same(&review));
    let finish = ops
        .finish_baseline(BaselineFinishInput::new(
            result.evidence().unwrap(),
            RemoteChoice::Remote,
        ))
        .unwrap();
    finish.submit().unwrap();
    assert!(matches!(finish.wait().unwrap(), JobState::Failed(_)));
    assert!(finish.observe_success().is_err());
    assert!(current(&ops, &review).same(&review));
    assert!(c.take_request().is_err());
}
#[test]
fn storage_retry_keeps_exact_raw_and_failure_history_unacknowledged() {
    let (ops, stream, review, remote) = fixture::setup();
    let c = claim(&reserve(&ops, stream, review.clone()));
    c.take_request().unwrap();
    sql(
        &ops,
        "CREATE TRIGGER fail_baseline BEFORE INSERT ON composer_draft_baseline_evidence BEGIN SELECT RAISE(ABORT,'fixture'); END;",
    );
    let raw = read(&remote);
    c.bind(raw.clone()).unwrap();
    c.submit().unwrap();
    assert!(matches!(c.wait().unwrap(), Err(p::Error::Storage)));
    assert!(c.observe_success().is_err());
    assert!(c.take_request().is_err());
    sql(&ops, "DROP TRIGGER fail_baseline;");
    c.retry_storage().unwrap();
    assert!(c.wait().unwrap().unwrap().evidence().is_some());
    assert!(c.retained_read().unwrap().response == raw.response);
    assert!(c.observe_success().is_err());
    assert!(current(&ops, &review).same(&review));
}
#[test]
fn deferred_resumption_revalidates_stream_and_retains_failed_claim() {
    let (ops, stream, review, _) = fixture::setup();
    let r = reserve(&ops, stream.clone(), review);
    sql(
        &ops,
        "INSERT INTO composer_draft_known_read_limit(team,until_ms) VALUES('T1',50000) ON CONFLICT(team) DO UPDATE SET until_ms=50000;",
    );
    let c = claim(&r);
    let gap = ops
        .mark_gap(GapInput {
            stream,
            reason: p::GapReason::Disconnect,
        })
        .unwrap();
    gap.submit().unwrap();
    gap.wait().unwrap();
    c.resume_deferred(50000).unwrap();
    assert!(c.wait_deferred().unwrap().is_err());
    assert!(c.resume_deferred(50000).is_err());
    assert!(c.take_request().is_err());
    assert!(c.observe_success().is_err());
    assert!(ops.resume_baseline_completion(&c.key()).is_ok());
}

#[test]
fn explicit_choice_storage_retry_reuses_same_job_and_preserves_failure() {
    let (ops, stream, review, remote) = fixture::setup();
    let c = claim(&reserve(&ops, stream, review.clone()));
    c.take_request().unwrap();
    c.bind(read(&remote)).unwrap();
    c.submit().unwrap();
    let evidence = c.wait().unwrap().unwrap().evidence().unwrap();
    let finish = ops
        .finish_baseline(BaselineFinishInput::new(evidence, RemoteChoice::Remote))
        .unwrap();
    let key = finish.key();
    sql(
        &ops,
        "CREATE TRIGGER fail_choice BEFORE UPDATE ON composer_draft_baseline_evidence BEGIN SELECT RAISE(ABORT,'fixture'); END;",
    );
    finish.submit().unwrap();
    assert_eq!(finish.wait().unwrap(), JobState::Failed(p::Error::Storage));
    assert!(current(&ops, &review).same(&review));
    drop(finish);
    sql(&ops, "DROP TRIGGER fail_choice;");
    let finish = ops.resume_finish_baseline(&key).unwrap();
    finish.retry_storage().unwrap();
    assert_eq!(finish.wait().unwrap(), JobState::Ready);
    assert!(!finish.result().unwrap().unwrap().same(&review));
    assert!(finish.observe_success().is_err());
    assert!(finish.retry_storage().is_err());
    assert!(ops.resume_finish_baseline(&key).is_ok());
}
