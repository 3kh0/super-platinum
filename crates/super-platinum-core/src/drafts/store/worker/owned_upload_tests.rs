//! Actual owned worker/SQLite, no configured paths or network calls.
use super::*;
use crate::drafts::store::{
    DraftStore,
    protected::owned_store::{FactoryIdentity, OwnedRoute},
    uploads,
};
#[test]
fn more_than_64_exact_cancellation_families_release_only_observed_success() {
    let ops = fixture();
    let baseline = ops.queue.diagnostics().unwrap().records.len();
    for n in 0..96 {
        let parent = reserve(&ops, n);
        let child = parent.cancel_reservation().unwrap();
        assert!(parent.cancel_reservation().is_err());
        assert!(parent.finalize(vec!["F1".into()]).is_err());
        let key = child.key();
        drop(child);
        let child = ops.resume_upload_cancellation(&parent, &key).unwrap();
        assert!(child.observe_canceled().is_err());
        child.submit().unwrap();
        assert!(matches!(child.wait().unwrap().as_ref(), Ok(true)));
        let ack = child.observe_canceled().unwrap();
        drop(ack);
        assert_eq!(ops.retained().len(), 2);
        assert!(child.observe_canceled().unwrap().acknowledge().is_ok());
        assert!(ops.retained().is_empty());
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), baseline);
        assert!(parent.submit().is_err());
        assert!(child.submit().is_err());
        assert!(ops.resume_upload_cancellation(&parent, &key).is_err());
    }
    assert!(
        DraftStore::open(ops.queue.owner.path())
            .unwrap()
            .load_uploads(ops.account())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn cancellation_rejects_existing_other_owner_finalized_and_fenced_reservations() {
    let ops = fixture();
    let other = fixture();
    let parent = reserve(&ops, 1);
    let again = ops.reserve_upload(draft(&ops, 1)).unwrap();
    again.submit().unwrap();
    again.wait().unwrap();
    assert!(again.cancel_reservation().is_err());
    assert!(again.observe_existing().unwrap().1.acknowledge().is_ok());
    let child = parent.cancel_reservation().unwrap();
    assert!(
        other
            .resume_upload_cancellation(&parent, &child.key())
            .is_err()
    );
    child.submit().unwrap();
    child.wait().unwrap();
    let ack = child.observe_canceled().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    assert!(ack.acknowledge().is_err());
    assert_eq!(ops.retained().len(), 2);
    ops.queue.worker.resume_admission(&fence).unwrap();
    assert!(child.observe_canceled().unwrap().acknowledge().is_ok());
    let parent = reserve(&ops, 2);
    let finalized = parent.finalize(vec!["F1".into()]).unwrap();
    assert!(parent.cancel_reservation().is_err());
    finalized.submit().unwrap();
    finalized.wait().unwrap();
    assert!(
        finalized
            .observe_finalized()
            .unwrap()
            .1
            .acknowledge()
            .is_ok()
    );
    let parent = reserve(&ops, 3);
    let fence = ops.queue.worker.begin_fence().unwrap();
    assert!(parent.cancel_reservation().is_err());
    ops.queue.worker.resume_admission(&fence).unwrap();
    // Historical generation remains historical even after admission resumes.
    assert!(parent.cancel_reservation().is_err());
    assert!(parent.finalize(vec!["F1".into()]).is_err());
}
#[test]
fn cancellation_false_result_preserves_parent_child_and_actual_changed_row() {
    let ops = fixture();
    let parent = reserve(&ops, 1);
    let reserved = parent.wait().unwrap();
    let original = &reserved.as_ref().as_ref().unwrap().stored;
    let child = parent.cancel_reservation().unwrap();
    // Synthetic concurrent durable mutation, not a network or timing claim.
    let mut db = DraftStore::open(ops.queue.owner.path()).unwrap();
    let changed = db
        .finalize_upload(&original.ticket, vec!["FOTHER".into()])
        .unwrap()
        .unwrap();
    child.submit().unwrap();
    assert!(matches!(child.wait().unwrap().as_ref(), Ok(false)));
    assert!(child.observe_canceled().is_err());
    assert_eq!(ops.retained().len(), 2);
    assert_eq!(db.load_uploads(ops.account()).unwrap(), vec![changed]);
}
fn fixture() -> Operations {
    let route = OwnedRoute::fresh_fixture().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    route.commit(&review).unwrap().wait().unwrap();
    route
        .inspect()
        .unwrap()
        .wait()
        .unwrap()
        .open()
        .unwrap()
        .initialize_cold(&FactoryIdentity::default())
        .unwrap()
        .wait()
        .unwrap()
        .operations()
}
fn draft(ops: &Operations, n: usize) -> Draft {
    let mut draft = uploads::tests::draft();
    draft.account_id = ops.account().into();
    draft.client_msg_id = format!("super-platinum-file-{}", 1000000 + n);
    draft
}
fn reserve(ops: &Operations, n: usize) -> UploadJob<Draft, Reservation> {
    let job = ops.reserve_upload(draft(ops, n)).unwrap();
    job.submit().unwrap();
    assert!(job.wait().unwrap().as_ref().as_ref().unwrap().created);
    job
}
#[test]
fn more_than_64_upload_families_reclaim_capacity_only_after_actual_finalization_observed() {
    let ops = fixture();
    let baseline = ops.queue.diagnostics().unwrap().records.len();
    for n in 0..96 {
        let parent = reserve(&ops, n);
        assert!(parent.observe_existing().is_err());
        let key = parent.key();
        let child = parent.finalize(vec!["F1".into()]).unwrap();
        assert!(parent.finalize(vec!["F2".into()]).is_err());
        let child_key = child.key();
        drop(child);
        let child = ops.resume_upload_finalization(&parent, &child_key).unwrap();
        child.submit().unwrap();
        let actual = child.wait().unwrap();
        let (stored, ack) = child.observe_finalized().unwrap();
        assert_eq!(Some(&stored), actual.as_ref().as_ref().unwrap().as_ref());
        assert!(ack.acknowledge().is_ok());
        assert!(parent.submit().is_err());
        assert!(ops.resume_upload_reservation(&key).is_err());
        assert!(child.submit().is_err());
        let load = ops.upload_inventory().unwrap();
        load.submit().unwrap();
        load.wait().unwrap();
        let (rows, ack) = load.observe_loaded().unwrap();
        assert_eq!(rows, vec![stored.clone()]);
        assert!(ack.acknowledge().is_ok());
        let retire = ops.retire_upload(stored).unwrap();
        retire.submit().unwrap();
        assert!(matches!(retire.wait().unwrap().as_ref(), Ok(true)));
        assert!(retire.observe_retired().unwrap().acknowledge().is_ok());
        assert!(ops.retained().is_empty());
        assert_eq!(ops.queue.diagnostics().unwrap().records.len(), baseline);
    }
    let rows = DraftStore::open(ops.queue.owner.path())
        .unwrap()
        .load_uploads(ops.account())
        .unwrap();
    assert!(rows.is_empty());
}
#[test]
fn existing_reservation_cannot_finalize_and_dropped_ack_does_not_release_parent() {
    let ops = fixture();
    let parent = reserve(&ops, 1);
    let again = ops.reserve_upload(draft(&ops, 1)).unwrap();
    again.submit().unwrap();
    again.wait().unwrap();
    let (existing, ack) = again.observe_existing().unwrap();
    assert!(!existing.created);
    drop(ack);
    assert_eq!(ops.retained().len(), 2);
    assert!(again.finalize(vec!["F1".into()]).is_err());
    assert!(again.observe_existing().unwrap().1.acknowledge().is_ok());
    assert_eq!(ops.retained().len(), 1);
    assert!(parent.observe_existing().is_err());
}
#[test]
fn held_and_unobserved_jobs_survive_capacity_and_exact_resumption() {
    let ops = fixture();
    let first = ops.reserve_upload(draft(&ops, 0)).unwrap();
    let key = first.key();
    let payload = first.input();
    assert!(first.wait().is_err());
    assert!(first.observe_existing().is_err());
    drop(first);
    let resumed = ops.resume_upload_reservation(&key).unwrap();
    assert!(Arc::ptr_eq(&payload, &resumed.input()));
    let mut held = Vec::new();
    for n in 1..64 {
        held.push(ops.reserve_upload(draft(&ops, n)).unwrap());
    }
    assert!(ops.reserve_upload(draft(&ops, 65)).is_err());
    assert_eq!(ops.retained().len(), 64);
    resumed.submit().unwrap();
    assert!(resumed.wait().unwrap().as_ref().as_ref().unwrap().created);
    assert_eq!(ops.retained().len(), 64);
    assert!(ops.reserve_upload(draft(&ops, 66)).is_err());
}
#[test]
fn wrong_owner_stale_retirement_and_failed_finalize_retain_truthful_outcomes() {
    let ops = fixture();
    let other = fixture();
    let bad = draft(&other, 1);
    assert!(ops.reserve_upload(bad).is_err());
    let parent = reserve(&ops, 1);
    let original = parent
        .wait()
        .unwrap()
        .as_ref()
        .as_ref()
        .unwrap()
        .stored
        .clone();
    let child = parent.finalize(vec![]).unwrap();
    child.submit().unwrap();
    assert!(child.wait().unwrap().is_err());
    assert!(child.observe_finalized().is_err());
    assert_eq!(ops.retained().len(), 2);
    assert!(other.retire_upload(original.clone()).is_err());
    let mut changed = original;
    changed.draft.caption.push('!');
    let retire = ops.retire_upload(changed).unwrap();
    retire.submit().unwrap();
    assert!(matches!(retire.wait().unwrap().as_ref(), Ok(false)));
    assert!(retire.observe_retired().is_err());
}
#[test]
fn fence_between_observation_and_ack_retains_entire_exact_family() {
    let ops = fixture();
    let parent = reserve(&ops, 1);
    let child = parent.finalize(vec!["F1".into()]).unwrap();
    child.submit().unwrap();
    child.wait().unwrap();
    let (_, ack) = child.observe_finalized().unwrap();
    let fence = ops.queue.worker.begin_fence().unwrap();
    assert!(ack.acknowledge().is_err());
    assert_eq!(ops.retained().len(), 2);
    ops.queue.worker.resume_admission(&fence).unwrap();
}
#[test]
fn two_actual_waiters_own_receiver_and_condvar_before_sqlite_unlock() {
    let ops = fixture();
    let db = DraftStore::open(ops.queue.owner.path()).unwrap();
    db.0.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let job = ops.reserve_upload(draft(&ops, 1)).unwrap();
    let (entered, events) = std::sync::mpsc::channel();
    job.held.wait_probe(entered);
    job.submit().unwrap();
    let first = job.clone();
    let a = std::thread::spawn(move || first.wait());
    assert_eq!(
        events
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
        "receiver"
    );
    let second = ops.resume_upload_reservation(&job.key()).unwrap();
    let b = std::thread::spawn(move || second.wait());
    assert_eq!(
        events
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
        "condvar"
    );
    job.held.wait_probe_barrier();
    db.0.execute_batch("COMMIT").unwrap();
    let a = a.join().unwrap().unwrap();
    let b = b.join().unwrap().unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(a.as_ref().as_ref().unwrap().created);
    assert_eq!(
        ops.queue
            .diagnostics()
            .unwrap()
            .records
            .iter()
            .filter(|r| r.family == "upload-reserve")
            .count(),
        1
    );
}
