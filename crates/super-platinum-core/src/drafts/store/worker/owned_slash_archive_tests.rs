//! Actual worker/SQLite archive bridge; no HTTP or direct archive transaction.
#[path = "owned_slash_archive_retry_tests.rs"]
mod retry_tests;
use super::*;
fn checkpoint(
    ops: &Operations,
    input: &SlashInput,
    serial: u64,
) -> StorageJob<CheckpointInput, p::WriteResult> {
    let expected = read_ack(ops, input);
    let job = ops
        .checkpoint(CheckpointInput {
            writer: input.inner.writer.clone(),
            serial,
            local: expected.local().clone(),
            expected,
            direct: None,
        })
        .unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job
}
fn count(ops: &Operations, table: &str) -> i64 {
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    db.0.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[tokio::test]
async fn archive_rejoins_exact_holder_survives_drop_and_has_no_replay() {
    let (ops, input, _) = fixture::setup();
    let c = executed(&ops, &input, true).await;
    let checkpoint = checkpoint(&ops, &input, 2);
    let job = c.archive_after_checkpoint(&checkpoint).unwrap();
    let before = ops.retained().len();
    let rejoined = ops
        .resume_slash_completion(&c.key())
        .unwrap()
        .archive_after_checkpoint(&checkpoint)
        .unwrap();
    assert_eq!(ops.retained().len(), before);
    let key = job.key();
    drop(job);
    rejoined.submit().unwrap();
    let job = ops.resume_slash_archive(&key).unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (result, ack) = job.observe_success().unwrap();
    assert_eq!(result.original_intent(), input.prior().id());
    assert_eq!(count(&ops, "composer_draft_slash_archives"), 1);
    assert_eq!(count(&ops, "composer_draft_slash_bindings"), 0);
    let (_, duplicate) = job.observe_success().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    let (error, ack) = ack.acknowledge().err().unwrap();
    assert_eq!(error, AdmissionError::Fencing);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(ack.acknowledge().is_ok());
    assert!(duplicate.acknowledge().is_err());
    assert!(ops.resume_slash_archive(&key).is_err());
    assert!(rejoined.submit().is_err());
    assert!(c.archive_after_checkpoint(&checkpoint).is_err());
    assert_eq!(count(&ops, "composer_draft_slash_archives"), 1);
}
#[tokio::test]
async fn archive_rejects_foreign_old_checkpoint_and_pending_native_clear() {
    let (ops, input, _) = fixture::setup();
    let c = executed(&ops, &input, false).await;
    let job = checkpoint(&ops, &input, 2);
    assert!(c.archive_after_checkpoint(&job).is_err());
    let terminal = c.terminal().unwrap();
    c.take_fresh_clear()
        .unwrap()
        .consume(&c, input.prior(), &terminal)
        .ok()
        .unwrap();
    let old_key = ops
        .retained()
        .into_iter()
        .find(|v| v.family == "protected-checkpoint")
        .unwrap()
        .key;
    let old = ops.resume_checkpoint(&old_key).unwrap();
    assert!(matches!(
        c.archive_after_checkpoint(&old),
        Err(AdmissionError::Stale)
    ));
    let (foreign, other, _) = fixture::setup();
    let foreign_checkpoint = checkpoint(&foreign, &other, 2);
    assert!(c.archive_after_checkpoint(&foreign_checkpoint).is_err());
    let fence = ops.queue.worker.begin_fence().unwrap();
    assert!(matches!(
        c.archive_after_checkpoint(&job),
        Err(AdmissionError::Fencing)
    ));
    ops.queue.worker.resume_admission(&fence).unwrap();
    let archive = c.archive_after_checkpoint(&job).unwrap();
    // A later accepted checkpoint invalidates the exact retained archive input.
    let newer = checkpoint(&ops, &input, 3);
    assert!(c.archive_after_checkpoint(&newer).is_err());
    archive.submit().unwrap();
    assert_eq!(archive.wait().unwrap(), JobState::Failed(p::Error::Stale));
    assert!(archive.observe_success().is_err());
    assert_eq!(count(&ops, "composer_draft_slash_archives"), 0);
    assert_eq!(count(&ops, "composer_draft_slash_bindings"), 1);
}
#[tokio::test]
async fn archive_storage_failure_retains_exact_job_and_durable_binding() {
    let (ops, input, _) = fixture::setup();
    let c = executed(&ops, &input, true).await;
    let cp = checkpoint(&ops, &input, 2);
    let job = c.archive_after_checkpoint(&cp).unwrap();
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    db.0.execute_batch("CREATE TRIGGER archive_bridge_fail BEFORE INSERT ON composer_draft_slash_archives BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    assert!(job.observe_success().is_err());
    assert!(ops.resume_slash_archive(&job.key()).is_ok());
    assert_eq!(count(&ops, "composer_draft_slash_archives"), 0);
    assert_eq!(count(&ops, "composer_draft_slash_bindings"), 1);
    assert!(matches!(records(&ops)[0].phase(), Phase::Completed(_)));
}
#[tokio::test]
async fn archive_does_not_convert_unsubmitted_or_failed_delivery_to_success() {
    for delivery in [
        Delivery::Unknown,
        Delivery::NotSent,
        Delivery::Reply(serde_json::json!({"ok":false,"error":"denied"})),
    ] {
        let (ops, input, _) = fixture::setup();
        let c = completion(&ops, input.clone());
        let key = ops
            .retained()
            .into_iter()
            .find(|v| v.family == "protected-checkpoint")
            .unwrap()
            .key;
        let cp = ops.resume_checkpoint(&key).unwrap();
        assert!(c.archive_after_checkpoint(&cp).is_err());
        c.execute_fixture(|permit| async move { permit.report(delivery, None) })
            .await
            .unwrap();
        assert_eq!(c.wait().unwrap(), JobState::Ready);
        assert!(c.archive_after_checkpoint(&cp).is_err());
        assert_eq!(count(&ops, "composer_draft_slash_archives"), 0);
        assert_eq!(count(&ops, "composer_draft_slash_bindings"), 1);
    }
}

#[tokio::test]
async fn archive_rechecks_stream_and_actual_pending_input_at_worker_boundary() {
    for pending in [false, true] {
        let (ops, input, _) = fixture::setup();
        let c = executed(&ops, &input, true).await;
        let cp = checkpoint(&ops, &input, 2);
        let job = c.archive_after_checkpoint(&cp).unwrap();
        if pending {
            // Actual worker's retained failed-input fence, not a request flag.
            ops.queue
                .worker
                .pending
                .lock()
                .unwrap()
                .failed
                .insert(ops.account().to_owned());
        } else {
            input.inner.stream.revoke();
        }
        job.submit().unwrap();
        assert_eq!(
            job.wait().unwrap(),
            JobState::Failed(if pending {
                p::Error::Pending
            } else {
                p::Error::Stale
            })
        );
        assert!(job.observe_success().is_err());
        assert_eq!(count(&ops, "composer_draft_slash_bindings"), 1);
        assert_eq!(count(&ops, "composer_draft_slash_archives"), 0);
    }
}
