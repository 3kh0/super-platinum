//! Exercise the production retry helper with actual isolated owned jobs.
//! The submit refusal is injected at the local helper boundary; these do not
//! claim to drive the hosted transaction or the native Editor removal UI.
use super::super::retirement_retry::Retry;
use super::*;

#[tokio::test]
async fn refused_admission_remains_visible_and_readmits_only_its_returned_input() {
    let f = Box::pin(Fixture::new("Retained caption")).await;
    let input = f.editor.scope().clone();
    let held = Held::<Scope, durable::Review>::new(Err(o::AdmissionRefused {
        reason: o::AdmissionError::Capacity,
        generation: None,
        input: input.clone(),
    }));
    let mut retry = Retry::default();
    retry.admission(&held);
    assert!(retry.available(Some(&held)));
    assert!(!retry.available::<Scope, durable::Review>(None));
    let next = retry.readmit(&held, |returned| {
        assert_eq!(returned, input);
        f.ops().review(returned)
    });
    assert!(!Arc::ptr_eq(&next, &held));
    assert_eq!(*next.job().unwrap().input(), input);
    assert_eq!(next.job().unwrap().state(), o::JobState::Waiting);
    assert!(!retry.available(Some(&next)));
    next.observed_success().await.unwrap();
}

#[tokio::test]
async fn initial_submit_refusal_reuses_exact_worker_key_and_input_without_readmission() {
    let f = Box::pin(Fixture::new("Retained caption")).await;
    let held = Held::new(f.ops().review(f.editor.scope().clone()));
    let key = held.job().unwrap().key();
    let input = held.job().unwrap().input();
    let mut retry = Retry::default();
    retry.admission(&held);
    assert!(!retry.available(Some(&held)));
    assert_eq!(
        retry.submission(Err(o::AdmissionError::Capacity)),
        Err(o::AdmissionError::Capacity)
    );
    assert!(retry.available(Some(&held)));
    let next = retry.readmit(&held, |_| {
        panic!("admitted input must keep its original job")
    });
    assert!(Arc::ptr_eq(&next, &held));
    assert!(Arc::ptr_eq(&input, &next.job().unwrap().input()));
    assert!(Arc::ptr_eq(
        &input,
        &f.ops().resume_review(&key).unwrap().input()
    ));
    retry.submission(next.submit()).unwrap();
    next.wait().await.unwrap();
    next.observed_success().await.unwrap();
    assert!(f.ops().resume_review(&key).is_err());
}

#[tokio::test]
async fn accepted_submission_clears_local_refusal_and_waiting_is_not_retryable() {
    let f = Box::pin(Fixture::new("")).await;
    let held = Held::new(f.ops().review(f.editor.scope().clone()));
    let mut retry = Retry::default();
    retry.admission(&held);
    assert_eq!(held.job().unwrap().state(), o::JobState::Waiting);
    assert!(!retry.available(Some(&held)));
    assert!(retry.submission(Err(o::AdmissionError::Fencing)).is_err());
    assert!(retry.available(Some(&held)));
    // Model the narrow interval after accepted submission and before worker
    // completion. Waiting alone cannot distinguish it from unsubmitted work.
    retry.submission(Ok(())).unwrap();
    assert_eq!(held.job().unwrap().state(), o::JobState::Waiting);
    assert!(!retry.available(Some(&held)));
    held.observed_success().await.unwrap();
}
