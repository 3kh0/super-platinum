//! Actual isolated Owned worker/SQLite and public facade. Synthetic transport
//! reports use the existing once-attempt test seam; no HTTP/native activation.
use super::*;
use crate::slash_commands::{controller, journal};
use controller::{Delivery, Phase};
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

mod fixture {
    use super::super::*;
    use crate::drafts::store::worker::admission::*;
    use std::sync::atomic::AtomicBool;
    mod support {
        include!("completion_test_support.rs");
    }
    pub(super) fn setup() -> (Operations, SlashInput, Arc<AtomicBool>) {
        // Reuse the exact existing Protected7 inspection/open/cold fixture.
        // It owns a real temporary store and never uses the default account path.
        let f = support::Fixture::for_slash();
        let ops = Operations::from_initialized(f.queue.clone());
        let register = ops.register(f.current()).unwrap();
        register.submit().unwrap();
        assert_eq!(register.wait().unwrap(), JobState::Ready);
        let writer = register.result().unwrap().unwrap();
        let checkpoint = ops
            .checkpoint(CheckpointInput {
                writer: writer.clone(),
                serial: 1,
                expected: f.current(),
                local: sync::tests::local("/example argument"),
                direct: None,
            })
            .unwrap();
        checkpoint.submit().unwrap();
        assert_eq!(checkpoint.wait().unwrap(), JobState::Ready);
        let WriteOutcome::Applied(review) = checkpoint.write_result().unwrap().unwrap() else {
            panic!("actual checkpoint")
        };
        let alive = Arc::new(AtomicBool::new(true));
        let plan = Arc::new(crate::slash_commands::controller::fixture_plan(
            &review,
            alive.clone(),
        ));
        (
            ops,
            SlashInput::from_prepared(f.stream, review, writer, 1, plan),
            alive,
        )
    }
}

fn until<T>(mut action: impl FnMut() -> Result<T, AdmissionError>) -> T {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match action() {
            Ok(value) => return value,
            Err(AdmissionError::Incomplete) => {
                assert!(Instant::now() < end, "actual worker timeout");
                std::thread::yield_now();
            }
            Err(error) => panic!("unexpected admission {error:?}"),
        }
    }
}
fn completion(ops: &Operations, input: SlashInput) -> SlashCompletion {
    let reserved = ops.slash(input).unwrap();
    reserved.submit().unwrap();
    assert_eq!(reserved.wait().unwrap(), JobState::Ready);
    let claimed = reserved.claim().unwrap();
    claimed.submit().unwrap();
    assert_eq!(claimed.wait().unwrap(), JobState::Ready);
    claimed.completion().unwrap()
}
fn settled(completion: &SlashCompletion) -> JobState {
    until(|| match completion.state() {
        JobState::Waiting => Err(AdmissionError::Incomplete),
        state => Ok(state),
    })
}
fn records(ops: &Operations) -> Vec<Record> {
    journal::load(
        &mut DraftStore::open(ops.queue.owner.path()).unwrap().0,
        ops.account(),
    )
    .unwrap()
}
fn current(ops: &Operations, input: &SlashInput) -> p::Review {
    let job = ops.review(input.review().scope().clone()).unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}

#[tokio::test]
async fn facade_rejoins_exact_steps_and_commits_one_clear_without_replay() {
    let (ops, input, _) = fixture::setup();
    let (foreign, _, _) = fixture::setup();
    let reserved = ops.slash(input.clone()).unwrap();
    let key = reserved.key();
    drop(reserved);
    assert!(matches!(
        foreign.resume_slash_reserved(&key),
        Err(AdmissionError::Stale)
    ));
    assert!(matches!(
        ops.resume_slash_claimed(&key),
        Err(AdmissionError::Stale)
    ));
    assert!(matches!(
        ops.resume_slash_completion(&key),
        Err(AdmissionError::Stale)
    ));
    let reserved = ops.resume_slash_reserved(&key).unwrap();
    assert!(Arc::ptr_eq(&input.inner, &reserved.input().inner));
    assert_eq!(reserved.input().serial(), 1);
    reserved.submit().unwrap();
    assert_eq!(reserved.wait().unwrap(), JobState::Ready);
    let claimed = reserved.claim().unwrap();
    claimed.submit().unwrap();
    assert_eq!(claimed.wait().unwrap(), JobState::Ready);
    let key = claimed.key();
    drop(claimed);
    let claimed = ops.resume_slash_claimed(&key).unwrap();
    assert!(Arc::ptr_eq(&input.inner, &claimed.input().inner));
    let completion = claimed.completion().unwrap();
    let second = claimed.completion().unwrap();
    let calls = AtomicUsize::new(0);
    completion
        .execute_fixture(|permit| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert!(permit.current(input.inner.plan.transport()));
            let crate::slack::client::RequestBody::Form(fields) = &permit.request().body else {
                panic!("actual request builder")
            };
            assert_eq!(
                fields.iter().find(|(key, _)| key == "command").unwrap().1,
                "/example"
            );
            assert_eq!(
                url::Url::parse(&permit.request().url).unwrap().path(),
                "/api/chat.command"
            );
            async move { permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None) }
        })
        .await
        .unwrap();
    let key = completion.key();
    drop(completion);
    drop(second);
    let completion = ops.resume_slash_completion(&key).unwrap();
    assert!(Arc::ptr_eq(&input.inner, &completion.input().inner));
    assert_eq!(settled(&completion), JobState::Ready);
    let terminal = completion.terminal().unwrap();
    let clear = completion.take_fresh_clear().unwrap();
    assert!(clear.matches(input.prior(), &terminal));
    let again = ops.resume_slash_completion(&key).unwrap();
    assert!(again.take_fresh_clear().is_none());
    assert!(matches!(
        foreign.resume_slash_completion(&key),
        Err(AdmissionError::Stale)
    ));
    assert!(matches!(
        ops.resume_mutation(&key),
        Err(AdmissionError::Stale)
    ));
    again.submit_storage().unwrap();
    assert!(
        again
            .execute_fixture(|_| async { panic!("no second HTTP") })
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(records(&ops), vec![terminal]);
    assert!(current(&ops, &input).local().empty());
}

#[test]
fn facade_refusal_returns_same_input_and_cannot_cross_worker() {
    let (ops, input, _) = fixture::setup();
    let (foreign, _, _) = fixture::setup();
    let refused = foreign.slash(input.clone()).err().unwrap();
    assert_eq!(refused.reason, AdmissionError::Stale);
    assert!(Arc::ptr_eq(&input.inner, &refused.input.inner));
    let _fence = ops.queue.worker.begin_fence().unwrap();
    let refused = ops.slash(input.clone()).err().unwrap();
    assert_eq!(refused.reason, AdmissionError::Fencing);
    assert!(Arc::ptr_eq(&input.inner, &refused.input.inner));
    assert!(records(&ops).is_empty());
    assert!(records(&foreign).is_empty());
}

#[tokio::test]
async fn revoked_or_held_new_input_cannot_execute_a_retained_claim() {
    for revoke in [false, true] {
        let (ops, input, alive) = fixture::setup();
        let completion = completion(&ops, input.clone());
        let held = if revoke {
            alive.store(false, Ordering::Release);
            None
        } else {
            Some(
                ops.checkpoint(CheckpointInput {
                    writer: input.inner.writer.clone(),
                    serial: 2,
                    expected: input.review().clone(),
                    local: sync::tests::local("new unsubmitted typing"),
                    direct: None,
                })
                .unwrap(),
            )
        };
        assert!(matches!(
            completion
                .execute_fixture(|_| async { panic!("must refuse before HTTP") })
                .await,
            Err(SlashExecuteError::BeforeDispatch(_))
        ));
        assert!(completion.report().is_none());
        assert!(completion.take_fresh_clear().is_none());
        assert_eq!(records(&ops), vec![input.inner.plan.next().clone()]);
        drop(held);
    }
}

#[tokio::test]
async fn unknown_keep_input_and_not_sent_preserve_the_exact_canonical_draft() {
    for delivery in [
        Delivery::Unknown,
        Delivery::NotSent,
        Delivery::Reply(serde_json::json!({"ok":true,"keep_input":true})),
    ] {
        let (ops, input, _) = fixture::setup();
        let completion = completion(&ops, input.clone());
        completion
            .execute_fixture(|permit| async move { permit.report(delivery, None) })
            .await
            .unwrap();
        assert_eq!(settled(&completion), JobState::Ready);
        assert!(completion.report().is_some());
        assert!(completion.take_fresh_clear().is_none());
        assert_eq!(records(&ops), vec![completion.terminal().unwrap()]);
        assert_eq!(
            serde_json::to_value(current(&ops, &input).local()).unwrap(),
            serde_json::to_value(input.review().local()).unwrap()
        );
    }
}

#[tokio::test]
async fn failed_settlement_retries_only_storage_and_keeps_original_report() {
    let (ops, input, _) = fixture::setup();
    let completion = completion(&ops, input.clone());
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    db.0.execute_batch("CREATE TRIGGER slash_facade_fail BEFORE INSERT ON composer_draft_slash_completions BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    let calls = AtomicUsize::new(0);
    completion
        .execute_fixture(|permit| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move { permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None) }
        })
        .await
        .unwrap();
    assert_eq!(settled(&completion), JobState::Failed(p::Error::Storage));
    let report = completion.report().unwrap();
    let key = completion.key();
    assert!(completion.take_fresh_clear().is_none());
    assert_eq!(records(&ops), vec![input.inner.plan.next().clone()]);
    drop(completion);
    let recovered = ops.resume_slash_completion(&key).unwrap();
    assert!(Arc::ptr_eq(&report, &recovered.report().unwrap()));
    assert!(
        recovered
            .execute_fixture(|_| async { panic!("storage failure cannot resend") })
            .await
            .is_err()
    );
    db.0.execute_batch("DROP TRIGGER slash_facade_fail")
        .unwrap();
    until(|| recovered.retry_storage());
    assert_eq!(settled(&recovered), JobState::Ready);
    assert!(Arc::ptr_eq(&report, &recovered.report().unwrap()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(recovered.take_fresh_clear().is_some());
    assert!(recovered.take_fresh_clear().is_none());
    assert!(matches!(
        recovered.retry_storage(),
        Err(AdmissionError::Stale)
    ));
    assert!(current(&ops, &input).local().empty());
    assert_eq!(records(&ops), vec![recovered.terminal().unwrap()]);
}

#[test]
fn canceled_actual_attempt_future_retains_dispatch_possible_without_replay() {
    use std::future::Future;
    let (ops, input, _) = fixture::setup();
    let completion = completion(&ops, input.clone());
    let mut future = Box::pin(completion.execute_fixture(|permit| async move {
        std::future::pending::<()>().await;
        permit.report(Delivery::Unknown, None)
    }));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
    drop(future);
    let key = completion.key();
    drop(completion);
    let recovered = ops.resume_slash_completion(&key).unwrap();
    let mut second = Box::pin(recovered.execute_fixture(|_| async { panic!("no replay") }));
    assert!(matches!(
        second.as_mut().poll(&mut context),
        std::task::Poll::Ready(Err(SlashExecuteError::BeforeDispatch(_)))
    ));
    assert!(recovered.report().is_none());
    assert!(recovered.terminal().is_none());
    assert!(recovered.take_fresh_clear().is_none());
    assert_eq!(records(&ops), vec![input.inner.plan.next().clone()]);
    assert!(matches!(records(&ops)[0].phase(), Phase::DispatchPossible));
}

#[path = "owned_slash_wait_tests.rs"]
mod wait_tests;

#[path = "owned_slash_retirement_tests.rs"]
mod retirement_tests;
