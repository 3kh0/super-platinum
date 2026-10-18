//! Real SQLite failure/repair through the same retained worker archive job.
use super::*;

fn fail(db: &DraftStore) {
    db.0.execute_batch("CREATE TRIGGER archive_bridge_fail BEFORE INSERT ON composer_draft_slash_archives BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
}
fn repair(db: &DraftStore) {
    db.0.execute_batch("DROP TRIGGER archive_bridge_fail")
        .unwrap();
}
fn archived_bytes(db: &DraftStore) -> String {
    db.0.query_row(
        "SELECT content FROM composer_draft_slash_archives",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

#[tokio::test]
async fn repaired_archive_observes_every_attempt_retires_family_and_preserves_history() {
    let (ops, input, _) = fixture::setup();
    let initial_slots = ops.retained().len();
    let initial_records = ops.queue.diagnostics().unwrap().records.len();
    let c = executed(&ops, &input, true).await;
    let cp = checkpoint(&ops, &input, 2);
    let job = c.archive_after_checkpoint(&cp).unwrap();
    let key = job.key();
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    fail(&db);
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    assert!(matches!(job.result(), Some(Err(p::Error::Storage))));
    assert!(job.observe_repaired_success().is_err());
    let first = ops.queue.diagnostics().unwrap().records.last().unwrap().id;
    job.retry_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    let second = ops.queue.diagnostics().unwrap().records.last().unwrap().id;
    assert!(second > first);
    let (foreign, _, _) = fixture::setup();
    assert!(foreign.resume_slash_archive(&key).is_err());
    drop(job);
    repair(&db);
    let job = ops.resume_slash_archive(&key).unwrap();
    job.retry_storage().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    assert!(job.retry_storage().is_err());
    assert!(job.observe_success().is_err()); // no fabricated clean history
    let (archived, observed, acknowledgment) = job.observe_repaired_success().unwrap();
    assert_eq!(observed.failed_submissions(), vec![first, second]);
    assert!(observed.successful_submission() > second);
    // Even the narrow accounting helper rejects reordered/wrong-destination or
    // misclassified observations atomically; the real job remains repairable.
    {
        use crate::drafts::store::terminal::{Execution, Report};
        let reports = vec![
            (first, Report::new(Execution::StorageUnresolved)),
            (second, Report::new(Execution::StorageUnresolved)),
            (
                observed.successful_submission(),
                Report::new(Execution::StorageAccepted),
            ),
        ];
        let mut pending = ops.queue.worker.pending.lock().unwrap();
        let book = pending.accounting.as_mut().unwrap();
        let before = book.work_ids();
        let mut wrong_key = input.review().scope().key.clone();
        wrong_key.channel = "C_OTHER".into();
        assert!(!book.archive_repair(&wrong_key, &reports, true));
        let mut wrong = reports.clone();
        wrong.swap(0, 1);
        assert!(!book.archive_repair(&input.review().scope().key, &wrong, true));
        wrong = reports.clone();
        wrong[0].1 = Report::new(Execution::StorageAccepted);
        assert!(!book.archive_repair(&input.review().scope().key, &wrong, true));
        assert_eq!(book.work_ids(), before);
    }
    assert_eq!(archived.original_intent(), input.prior().id());
    assert!(matches!(job.result(), Some(Ok(_))));
    let durable = archived_bytes(&db);
    let (_, _, duplicate) = job.observe_repaired_success().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    let (error, acknowledgment) = acknowledgment.acknowledge().err().unwrap();
    assert_eq!(error, AdmissionError::Fencing);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(acknowledgment.acknowledge().is_ok());
    assert!(duplicate.acknowledge().is_err());
    assert!(ops.resume_slash_archive(&key).is_err());
    assert!(job.submit().is_err());
    assert!(job.retry_storage().is_err());
    assert!(c.archive_after_checkpoint(&cp).is_err());
    let (_, ack) = cp.observe_checkpoint().unwrap();
    assert!(ack.acknowledge().is_ok());
    let (_, ack) = c.observe_acknowledged().unwrap();
    assert!(ack.acknowledge().is_ok());
    assert_eq!(ops.retained().len(), initial_slots);
    assert_eq!(
        ops.queue.diagnostics().unwrap().records.len(),
        initial_records
    );
    assert_eq!(archived_bytes(&db), durable);
    assert_eq!(count(&ops, "composer_draft_slash_archives"), 1);
    assert_eq!(count(&ops, "composer_draft_slash_bindings"), 0);
    assert_eq!(records(&ops).len(), 1);
}

#[tokio::test]
async fn archive_retry_refuses_unsubmitted_clean_success_and_changed_checkpoint() {
    for stale in [false, true] {
        let (ops, input, _) = fixture::setup();
        let c = executed(&ops, &input, true).await;
        let cp = checkpoint(&ops, &input, 2);
        let job = c.archive_after_checkpoint(&cp).unwrap();
        assert!(job.retry_storage().is_err());
        let db = DraftStore::open(ops.queue.owner.path()).unwrap();
        if stale {
            fail(&db);
        }
        job.submit().unwrap();
        if stale {
            assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
            repair(&db);
            let _newer = checkpoint(&ops, &input, 3);
            job.retry_storage().unwrap();
            assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Stale));
            assert!(job.retry_storage().is_err());
            assert!(job.observe_repaired_success().is_err());
            assert_eq!(count(&ops, "composer_draft_slash_bindings"), 1);
            assert_eq!(count(&ops, "composer_draft_slash_archives"), 0);
        } else {
            assert_eq!(job.wait().unwrap(), JobState::Ready);
            assert!(job.retry_storage().is_err());
            assert!(job.observe_repaired_success().is_err());
            let (_, ack) = job.observe_success().unwrap();
            assert!(ack.acknowledge().is_ok());
        }
    }

    // Unlike historical acknowledgment, a retry issues a new local request.
    // Canceling a fence must not refresh the old slot's admission generation.
    let (ops, input, _) = fixture::setup();
    let c = executed(&ops, &input, true).await;
    let cp = checkpoint(&ops, &input, 2);
    let job = c.archive_after_checkpoint(&cp).unwrap();
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    fail(&db);
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
    repair(&db);
    let before = ops.queue.diagnostics().unwrap().records.len();
    let fence = ops.queue.worker.begin_fence().unwrap();
    assert_eq!(job.retry_storage(), Err(AdmissionError::Fencing));
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert_eq!(job.retry_storage(), Err(AdmissionError::Stale));
    assert_eq!(job.state(), JobState::Failed(p::Error::Storage));
    assert_eq!(ops.queue.diagnostics().unwrap().records.len(), before);
    assert!(ops.resume_slash_archive(&job.key()).is_ok());
    assert_eq!(count(&ops, "composer_draft_slash_bindings"), 1);
    assert_eq!(count(&ops, "composer_draft_slash_archives"), 0);
}

#[tokio::test]
async fn archive_retry_rechecks_stream_and_pending_input_without_losing_binding() {
    for pending in [false, true] {
        let (ops, input, _) = fixture::setup();
        let c = executed(&ops, &input, true).await;
        let cp = checkpoint(&ops, &input, 2);
        let job = c.archive_after_checkpoint(&cp).unwrap();
        let db = DraftStore::open(ops.queue.owner.path()).unwrap();
        fail(&db);
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
        repair(&db);
        if pending {
            ops.queue
                .worker
                .pending
                .lock()
                .unwrap()
                .failed
                .insert(ops.account().into());
        } else {
            input.inner.stream.revoke();
        }
        job.retry_storage().unwrap();
        assert_eq!(
            job.wait().unwrap(),
            JobState::Failed(if pending {
                p::Error::Pending
            } else {
                p::Error::Stale
            })
        );
        assert!(job.observe_repaired_success().is_err());
        assert!(ops.resume_slash_archive(&job.key()).is_ok());
        assert_eq!(count(&ops, "composer_draft_slash_bindings"), 1);
        assert_eq!(count(&ops, "composer_draft_slash_archives"), 0);
    }
}
