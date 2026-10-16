//! Actual retained inventory reads: no normalization, synthetic Loaded or mode.
use super::*;
fn waited<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}
#[test]
fn inventory_observes_actual_enrollment_and_lost_result_reuses_same_arc() {
    let f = Fixture::new();
    let before = waited(f.ops.review(f.scope.clone()).unwrap());
    let job = f.ops.inventory().unwrap();
    let key = job.key();
    job.submit().unwrap();
    drop(job);
    let resumed = f.ops.resume_inventory(&key).unwrap();
    assert_eq!(resumed.wait().unwrap(), JobState::Ready);
    let loaded = resumed.result().unwrap().unwrap();
    assert!(loaded.managed().get(&f.scope.key).unwrap().same(&before));
    assert!(loaded.locals().is_empty());
    assert!(loaded.uncommitted().is_empty());
    let again = f
        .ops
        .resume_inventory(&key)
        .unwrap()
        .result()
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&loaded, &again));
    assert!(f.stream.is_live());
    assert!(waited(f.ops.review(f.scope.clone()).unwrap()).same(&before));
}
#[test]
fn inventory_does_not_normalize_actual_inflight_send_or_revoke_live_stream() {
    let f = Fixture::new();
    let send = f
        .ops
        .consume_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    waited(send.clone());
    let prepare = send.prepare().unwrap();
    prepare.submit().unwrap();
    assert_eq!(prepare.wait().unwrap(), JobState::Ready);
    let before = waited(f.ops.review(f.scope.clone()).unwrap());
    assert!(before.row().state().pending().is_some());
    let loaded = waited(f.ops.inventory().unwrap());
    assert!(loaded.managed().get(&f.scope.key).unwrap().same(&before));
    assert!(f.stream.is_live());
    let claim = prepare.claim().unwrap();
    claim.submit().unwrap();
    assert_eq!(claim.wait().unwrap(), JobState::Ready);
    let completion = claim.completion().unwrap();
    assert_eq!(
        completion.take_claimed().unwrap().operation(),
        before.row().state().pending().unwrap()
    );
}
#[test]
fn inventory_capacity_refuses_without_destroying_prior_actual_inventory() {
    let f = Fixture::new();
    let loaded = waited(f.ops.inventory().unwrap());
    let mut jobs = Vec::new();
    loop {
        match f.ops.inventory() {
            Ok(job) => jobs.push(job),
            Err(e) => {
                assert_eq!(e.reason, AdmissionError::Capacity);
                assert_eq!(e.input, f.ops.account());
                break;
            }
        }
    }
    assert!(loaded.managed().get(&f.scope.key).is_some());
    assert!(f.stream.is_live());
    assert!(!jobs.is_empty());
}
