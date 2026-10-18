//! Actual owned executor/SQLite; no caller-made evidence or generic eviction.
use super::*;
use crate::drafts::store::sync::RemoteChoice;
use p::owned_store::{FactoryIdentity, OwnedRoute};
fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (value, ack) = job.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    value
}
fn fixture(outcome: BaselineFixtureOutcome) -> (Operations, p::Stream, p::Review, BaselineFixture) {
    let (route, fixture) =
        OwnedRoute::fresh_baseline_fixture(BaselineFixtureTarget::Channel, outcome).unwrap();
    let inspected = route.inspect().unwrap().wait().unwrap();
    route.commit(&inspected).unwrap().wait().unwrap();
    let initialized = route
        .inspect()
        .unwrap()
        .wait()
        .unwrap()
        .open()
        .unwrap()
        .initialize_cold(&FactoryIdentity::default())
        .unwrap()
        .wait()
        .unwrap();
    let review = initialized.inventory().managed()[&fixture.scope().key].clone();
    let ops = initialized.operations();
    let expected = done(
        ops.stream_state(TeamInput {
            scope: fixture.scope().clone(),
        })
        .unwrap(),
    );
    let stream = done(
        ops.open_stream(OpenInput {
            scope: fixture.scope().clone(),
            expected,
        })
        .unwrap(),
    );
    (ops, stream, review, fixture)
}
fn reserve(ops: &Operations, stream: &p::Stream, review: &p::Review, now: u64) -> BaselineReserved {
    let r = ops
        .baseline(BaselineInput::new(stream.clone(), review.clone(), now))
        .unwrap();
    r.submit().unwrap();
    r.wait().unwrap();
    r
}
fn claim(r: &BaselineReserved) -> BaselineCompletion {
    let c = r.claim().unwrap();
    c.submit().unwrap();
    assert_eq!(c.wait().unwrap(), JobState::Ready);
    c.completion().unwrap()
}
fn sql(ops: &Operations, text: &str) {
    rusqlite::Connection::open(ops.queue.owner.path())
        .unwrap()
        .execute_batch(text)
        .unwrap();
}
fn fail_read(ops: &Operations) {
    sql(
        ops,
        "CREATE TRIGGER fail_baseline BEFORE INSERT ON composer_draft_baseline_evidence BEGIN SELECT RAISE(ABORT,'fixture'); END;",
    );
}
fn records(ops: &Operations) -> usize {
    ops.queue.diagnostics().unwrap().records.len()
}
#[test]
fn observed_cooldown_retires_only_three_unissued_slots_and_one_accounted_failure() {
    let (ops, stream, review, fixture) = fixture(BaselineFixtureOutcome::RateLimitedThenChanged);
    let c = claim(&reserve(&ops, &stream, &review, 1000));
    c.bind(fixture.read(&c, 1000).unwrap()).unwrap();
    c.submit().unwrap();
    assert!(c.wait().unwrap().unwrap().evidence().is_none());
    assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
    let baseline_slots = ops.retained().len();
    let baseline_records = records(&ops);
    let (foreign, _, _, _) = self::fixture(BaselineFixtureOutcome::Changed);
    // More than the bounded diagnostic capacity over time, without accumulation.
    for i in 0..70 {
        let r = reserve(&ops, &stream, &review, 60999);
        let key = r.key();
        assert_eq!(r.state(), JobState::Failed(p::Error::Cooldown(61000)));
        assert!(r.claim().is_err());
        assert!(foreign.resume_baseline_reserved(&key).is_err());
        assert_eq!(ops.retained().len(), baseline_slots + 3);
        assert_eq!(records(&ops), baseline_records + 1);
        let (observed, ack) = r.observe_cooldown().unwrap();
        assert_eq!(observed.until_ms(), 61000);
        assert_eq!(
            ops.resume_baseline_reserved(observed.key())
                .unwrap()
                .state(),
            r.state()
        );
        let (_, duplicate) = r.observe_cooldown().unwrap();
        let ack = if i == 0 {
            let fence = ops.queue.worker.begin_fence().unwrap();
            let (e, ack) = ack.acknowledge().err().unwrap();
            assert_eq!(e, AdmissionError::Fencing);
            ops.queue.worker.resume_admission(&fence).unwrap();
            ack
        } else {
            ack
        };
        assert!(ack.acknowledge().is_ok());
        assert!(duplicate.acknowledge().is_err());
        assert!(r.submit().is_err());
        assert!(r.claim().is_err());
        assert!(r.observe_cooldown().is_err());
        assert!(ops.resume_baseline_reserved(&key).is_err());
        assert_eq!(ops.retained().len(), baseline_slots);
        assert_eq!(records(&ops), baseline_records);
    }
    let r = reserve(&ops, &stream, &review, 61000);
    assert_eq!(r.state(), JobState::Ready);
    assert!(r.observe_cooldown().is_err());
    let c = claim(&r);
    c.bind(fixture.read(&c, 61000).unwrap()).unwrap();
    c.submit().unwrap();
    let evidence = c.wait().unwrap().unwrap().evidence().unwrap();
    assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
    let settled = done(
        ops.finish_baseline(BaselineFinishInput::new(evidence, RemoteChoice::Remote))
            .unwrap(),
    );
    assert_eq!(settled.row().lineage().basis(), p::Basis::Remote);
    assert_eq!(fixture.reads().len(), 2);
}
#[test]
fn repaired_read_preserves_raw_and_evidence_then_retires_exact_observed_attempts() {
    let (ops, stream, review, fixture) = fixture(BaselineFixtureOutcome::Changed);
    let slots = ops.retained().len();
    let count = records(&ops);
    let c = claim(&reserve(&ops, &stream, &review, 1000));
    let key = c.key();
    let raw = fixture.read(&c, 1000).unwrap();
    fail_read(&ops);
    c.bind(raw.clone()).unwrap();
    c.submit().unwrap();
    assert!(matches!(c.wait().unwrap(), Err(p::Error::Storage)));
    assert!(c.observe_repaired_success().is_err());
    c.retry_storage().unwrap();
    assert!(matches!(c.wait().unwrap(), Err(p::Error::Storage)));
    sql(&ops, "DROP TRIGGER fail_baseline;");
    c.retry_storage().unwrap();
    let value = c.wait().unwrap().unwrap();
    assert!(c.observe_success().is_err());
    assert!(c.retained_read().unwrap().response == raw.response);
    assert!(c.take_request().is_err());
    let (observed, repair, ack) = c.observe_repaired_success().unwrap();
    assert!(ack.matches(&value));
    assert_eq!(repair.failed_submissions().len(), 2);
    assert!(
        repair
            .failed_submissions()
            .iter()
            .all(|v| *v < repair.successful_submission())
    );
    let mut forged = repair.reports.clone();
    forged[0].0 += 1;
    assert!(
        ops.queue
            .worker
            .observe_baseline_retirement(&c.held, Some(&value.value), Some(&forged), true, true)
            .is_err()
    );
    let fence = ops.queue.worker.begin_fence().unwrap();
    let (e, ack) = ack.acknowledge().err().unwrap();
    assert_eq!(e, AdmissionError::Fencing);
    ops.queue.worker.resume_admission(&fence).unwrap();
    let (_, _, duplicate) = c.observe_repaired_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    assert!(duplicate.acknowledge().is_err());
    assert!(ops.resume_baseline_completion(&key).is_err());
    assert!(c.retry_storage().is_err());
    assert_eq!(ops.retained().len(), slots);
    assert_eq!(records(&ops), count);
    // Evidence outlives memory acknowledgment, with actual explicit settlement.
    let settled = done(
        ops.finish_baseline(BaselineFinishInput::new(
            observed.evidence().unwrap(),
            RemoteChoice::Remote,
        ))
        .unwrap(),
    );
    assert!(settled.local().composer.text.contains("Remote"));
    assert_eq!(fixture.reads().len(), 1);
}
#[test]
fn finish_repair_is_same_job_and_cannot_skip_failure_observation_or_fence() {
    let (ops, stream, review, fixture) = fixture(BaselineFixtureOutcome::Changed);
    let c = claim(&reserve(&ops, &stream, &review, 1000));
    c.bind(fixture.read(&c, 1000).unwrap()).unwrap();
    c.submit().unwrap();
    let evidence = c.wait().unwrap().unwrap().evidence().unwrap();
    assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
    let slots = ops.retained().len();
    let count = records(&ops);
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
    assert!(finish.observe_repaired_success().is_err());
    drop(finish);
    let finish = ops.resume_finish_baseline(&key).unwrap();
    finish.retry_storage().unwrap();
    assert_eq!(finish.wait().unwrap(), JobState::Failed(p::Error::Storage));
    sql(&ops, "DROP TRIGGER fail_choice;");
    finish.retry_storage().unwrap();
    assert_eq!(finish.wait().unwrap(), JobState::Ready);
    assert!(finish.observe_success().is_err());
    let (value, repair, ack) = finish.observe_repaired_success().unwrap();
    assert_eq!(repair.failed_submissions().len(), 2);
    assert!(ack.matches(&value));
    assert!(!ack.matches(&review));
    assert!(
        ops.queue
            .worker
            .observe_baseline_finish_repair(&finish.step, Some((&review, &repair.reports)), true)
            .is_err()
    );
    let fence = ops.queue.worker.begin_fence().unwrap();
    let (_, ack) = ack.acknowledge().err().unwrap();
    ops.queue.worker.resume_admission(&fence).unwrap();
    let (_, _, duplicate) = finish.observe_repaired_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    assert!(duplicate.acknowledge().is_err());
    assert!(finish.submit().is_err());
    assert!(ops.resume_finish_baseline(&key).is_err());
    assert_eq!(ops.retained().len(), slots);
    assert_eq!(records(&ops), count);
    assert!(done(ops.review(fixture.scope().clone()).unwrap()).same(&value));
}
#[test]
fn actual_deferred_claim_history_and_repaired_read_are_observed_together() {
    let (ops, stream, review, fixture) = fixture(BaselineFixtureOutcome::Changed);
    let slots = ops.retained().len();
    let count = records(&ops);
    let r = reserve(&ops, &stream, &review, 1000);
    sql(
        &ops,
        "INSERT INTO composer_draft_known_read_limit(team,until_ms) VALUES('T1',50000) ON CONFLICT(team) DO UPDATE SET until_ms=50000;",
    );
    let c = claim(&r);
    assert_eq!(c.deferred_until(), Some(50000));
    assert!(c.take_request().is_err());
    c.resume_deferred(50000).unwrap();
    assert_eq!(c.wait_deferred().unwrap().unwrap(), None);
    let raw = fixture.read(&c, 50000).unwrap();
    fail_read(&ops);
    c.bind(raw).unwrap();
    c.submit().unwrap();
    assert!(matches!(c.wait().unwrap(), Err(p::Error::Storage)));
    sql(&ops, "DROP TRIGGER fail_baseline;");
    c.retry_storage().unwrap();
    c.wait().unwrap().unwrap();
    let (_, repair, ack) = c.observe_repaired_success().unwrap();
    assert_eq!(repair.failed_submissions().len(), 1);
    assert_eq!(repair.reports.len(), 5);
    assert!(ack.acknowledge().is_ok());
    assert_eq!(ops.retained().len(), slots);
    assert_eq!(records(&ops), count);
}
#[test]
fn clean_or_nonstorage_results_never_mint_repair_or_cooldown_receipts() {
    let (ops, stream, review, fixture) = fixture(BaselineFixtureOutcome::Unchanged);
    let c = claim(&reserve(&ops, &stream, &review, 1000));
    c.bind(fixture.read(&c, 1000).unwrap()).unwrap();
    c.submit().unwrap();
    let evidence = c.wait().unwrap().unwrap().evidence().unwrap();
    assert!(c.observe_repaired_success().is_err());
    assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
    let finish = ops
        .finish_baseline(BaselineFinishInput::new(evidence, RemoteChoice::Remote))
        .unwrap();
    finish.submit().unwrap();
    assert_eq!(finish.wait().unwrap(), JobState::Ready);
    assert!(finish.observe_repaired_success().is_err());
    assert!(finish.observe_success().unwrap().1.acknowledge().is_ok());
    done(
        ops.mark_gap(GapInput {
            stream: stream.clone(),
            reason: p::GapReason::Disconnect,
        })
        .unwrap(),
    );
    let failed = reserve(&ops, &stream, &review, 1000);
    assert!(matches!(failed.state(), JobState::Failed(_)));
    assert!(failed.observe_cooldown().is_err());
    assert!(ops.resume_baseline_reserved(&failed.key()).is_ok());
}
