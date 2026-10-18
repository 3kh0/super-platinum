//! Actual named Slash family, real SQLite, synthetic sealed transport only.
use super::*;
async fn executed(ops: &Operations, input: &SlashInput, keep: bool) -> SlashCompletion {
    let c = completion(ops, input.clone());
    c.execute_fixture(|permit| async move {
        permit.report(
            Delivery::Reply(serde_json::json!({"ok":true,"keep_input":keep})),
            None,
        )
    })
    .await
    .unwrap();
    assert_eq!(c.wait().unwrap(), JobState::Ready);
    c
}
fn read_ack(ops: &Operations, input: &SlashInput) -> p::Review {
    let j = ops.review(input.review().scope().clone()).unwrap();
    j.submit().unwrap();
    j.wait().unwrap();
    let (r, a) = j.observe_success().unwrap();
    assert!(a.acknowledge().is_ok());
    r
}
#[tokio::test]
async fn ninety_six_slash_families_preserve_durable_history_without_capacity_leak() {
    let (ops, mut input, _) = fixture::setup();
    let count = ops.retained().len();
    let accounting = ops.queue.diagnostics().unwrap().records.len();
    for n in 0..96 {
        if n > 0 {
            let current = read_ack(&ops, &input);
            let j = ops
                .checkpoint(CheckpointInput {
                    writer: input.inner.writer.clone(),
                    serial: 2 * n + 1,
                    expected: current,
                    local: sync::tests::local("/example argument"),
                    direct: None,
                })
                .unwrap();
            j.submit().unwrap();
            assert_eq!(j.wait().unwrap(), JobState::Ready);
            let (review, a) = j.observe_checkpoint().unwrap();
            assert!(a.acknowledge().is_ok());
            let alive = Arc::new(AtomicBool::new(true));
            let plan = Arc::new(controller::fixture_plan(&review, alive));
            input = SlashInput::from_prepared(
                input.inner.stream.clone(),
                review,
                input.inner.writer.clone(),
                2 * n + 1,
                plan,
            );
        }
        // Alternating clear and keep-input exercises both exact witness and
        // acknowledged non-clearing dispositions on the same real worker.
        let keep = n % 2 == 1;
        let c = executed(&ops, &input, keep).await;
        if !keep {
            assert!(c.observe_acknowledged().is_err());
            let terminal = c.terminal().unwrap();
            let fresh = c.take_fresh_clear().unwrap();
            assert!(c.observe_acknowledged().is_err());
            assert!(fresh.consume(&c, input.prior(), &terminal).is_ok());
        }
        // Native clear/keep has been handled above. Capture the CURRENT actual
        // accepted checkpoint, not the pre-dispatch writer receipt.
        let before = read_ack(&ops, &input);
        let checkpoint = ops
            .checkpoint(CheckpointInput {
                writer: input.inner.writer.clone(),
                serial: 2 * n + 2,
                expected: before.clone(),
                local: before.local().clone(),
                direct: None,
            })
            .unwrap();
        checkpoint.submit().unwrap();
        assert_eq!(checkpoint.wait().unwrap(), JobState::Ready);
        let archive = c.archive_after_checkpoint(&checkpoint).unwrap();
        archive.submit().unwrap();
        assert_eq!(archive.wait().unwrap(), JobState::Ready);
        let (archived, ack) = archive.observe_success().unwrap();
        assert_eq!(archived.original_intent(), input.prior().id());
        assert_eq!(
            serde_json::to_value(archived.historical_review().local()).unwrap(),
            serde_json::to_value(before.local()).unwrap()
        );
        assert!(ack.acknowledge().is_ok());
        let (_, ack) = checkpoint.observe_checkpoint().unwrap();
        assert!(ack.acknowledge().is_ok());
        let (record, a) = c.observe_acknowledged().unwrap();
        let key = c.key();
        assert!(a.acknowledge().is_ok());
        assert!(matches!(
            ops.resume_slash_completion(&key),
            Err(AdmissionError::Stale)
        ));
        assert!(c.submit_storage().is_err());
        assert!(
            c.execute_fixture(|_| async { panic!("retired family cannot replay") })
                .await
                .is_err()
        );
        assert_eq!(c.terminal().unwrap(), record);
        assert_eq!(ops.retained().len(), count);
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), accounting);
    }
    assert_eq!(records(&ops).len(), 96);
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    let archives: i64 =
        db.0.query_row(
            "SELECT COUNT(*) FROM composer_draft_slash_archives",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let bindings: i64 =
        db.0.query_row(
            "SELECT COUNT(*) FROM composer_draft_slash_bindings",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(archives, 96);
    assert_eq!(bindings, 0);
}
#[tokio::test]
async fn dropped_fresh_witness_keeps_the_whole_family_recoverable() {
    let (ops, input, _) = fixture::setup();
    let c = executed(&ops, &input, false).await;
    let count = ops.retained().len();
    let fresh = c.take_fresh_clear().unwrap();
    drop(fresh);
    assert!(c.take_fresh_clear().is_none());
    assert!(c.observe_acknowledged().is_err());
    assert!(
        ops.resume_slash_completion(&c.key())
            .unwrap()
            .observe_acknowledged()
            .is_err()
    );
    assert_eq!(ops.retained().len(), count);
}
#[tokio::test]
async fn duplicate_receipts_and_fence_cannot_retire_another_family() {
    let (ops, input, _) = fixture::setup();
    let c = executed(&ops, &input, true).await;
    let (_, a) = c.observe_acknowledged().unwrap();
    let (_, b) = c.observe_acknowledged().unwrap();
    let before = ops.retained().len();
    drop(b);
    assert_eq!(ops.retained().len(), before);
    let (_, b) = c.observe_acknowledged().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    let (error, a) = a.acknowledge().err().unwrap();
    assert_eq!(error, AdmissionError::Fencing);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(a.acknowledge().is_ok());
    assert!(b.acknowledge().is_err());
    assert_eq!(ops.retained().len(), before - 3);
}
#[tokio::test]
async fn unknown_rejected_unsubmitted_and_cancelled_families_remain() {
    for delivery in [
        Delivery::Unknown,
        Delivery::NotSent,
        Delivery::Reply(serde_json::json!({"ok":false,"error":"denied"})),
    ] {
        let (ops, input, _) = fixture::setup();
        let c = completion(&ops, input);
        assert!(c.observe_acknowledged().is_err());
        c.execute_fixture(|permit| async move { permit.report(delivery, None) })
            .await
            .unwrap();
        assert_eq!(c.wait().unwrap(), JobState::Ready);
        let before = ops.retained().len();
        assert!(c.observe_acknowledged().is_err());
        assert_eq!(ops.retained().len(), before);
        assert!(ops.resume_slash_completion(&c.key()).is_ok());
    }
    let (ops, input, _) = fixture::setup();
    let c = completion(&ops, input);
    let mut f = Box::pin(c.execute_fixture(|permit| async move {
        std::future::pending::<()>().await;
        permit.report(Delivery::Unknown, None)
    }));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(std::future::Future::poll(f.as_mut(), &mut cx).is_pending());
    drop(f);
    assert!(c.observe_acknowledged().is_err());
    assert!(ops.resume_slash_completion(&c.key()).is_ok());
}

#[path = "owned_slash_archive_tests.rs"]
mod archive_tests;
