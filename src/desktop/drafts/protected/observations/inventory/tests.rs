use super::super::super::source::inventory::{Refused, tests::Fixture};
use super::*;
use serde_json::json;

fn work(f: &Fixture) -> Arc<Work> {
    let owner = f.store.owner.clone();
    let raw: RawEvent = serde_json::from_value(json!({"type":"draft_delete", "draft_id":"Dr1",
        "event_ts":"1000.000001", "client_msg_id":"00000000-0000-4000-8000-000000000397"}))
    .unwrap();
    Arc::new(Work {
        prepared: f
            .source
            .prepare(&f.shell, &owner.scope().key, &raw)
            .unwrap(),
        owner,
        editor: None,
    })
}

#[tokio::test]
async fn accepted_work_remains_actual_across_pending_active_result_and_take() {
    let f = Fixture::new().await;
    let h = f.source.ownership();
    let q = ObservationQueue::new(f.store.account.clone(), f.source.clone());
    let empty = h.snapshot(&f.store.account, &f.shell).unwrap();
    let input = work(&f);
    assert!(q.admit(input.clone()).unwrap());
    assert_eq!(
        empty.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Changed)
    );
    let pending = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert!(Arc::ptr_eq(&pending.queues[0].pending[0], &input));
    let running = q.next().unwrap();
    assert!(Arc::ptr_eq(&running, &input));
    assert_eq!(
        pending.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Changed)
    );
    let active = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert!(Arc::ptr_eq(
        active.queues[0].active.as_ref().unwrap(),
        &input
    ));
    let review = f.store.owner.review().await.unwrap();
    q.finish(Ok(Observed {
        before: review.clone(),
        after: review.clone(),
        barrier: None,
    }));
    let completed = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert!(!completed.queues[0].active());
    assert!(
        completed.queues[0].results[0]
            .result
            .as_ref()
            .unwrap()
            .after
            .same(&review)
    );
    assert!(q.take_result().unwrap().unwrap().before.same(&review));
    assert_eq!(
        completed.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Changed)
    );
    // Older snapshot retains exact result even after application takes it.
    assert!(
        completed.queues[0].results[0]
            .result
            .as_ref()
            .unwrap()
            .before
            .same(&review)
    );
    assert!(q.next().is_none());
    let delivered = q.inventory().unwrap();
    assert_eq!(delivered.delivered(), 1);
    assert!(Arc::ptr_eq(
        delivered.delivered[0].work.as_ref().unwrap(),
        &input
    ));
}

#[tokio::test]
async fn canceled_runner_before_poll_retains_ordered_unknown_inputs() {
    let f = Fixture::new().await;
    let q = ObservationQueue::new(f.store.account.clone(), f.source.clone());
    let first = work(&f);
    let second = work(&f);
    q.admit(first.clone()).unwrap();
    q.admit(second.clone()).unwrap();
    let refused = work(&f);
    let guard = Running {
        queue: q.clone(),
        finished: false,
    };
    drop(guard);
    let snapshot = q.inventory().unwrap();
    assert!(snapshot.unknown && snapshot.failed && !snapshot.running);
    assert_eq!(snapshot.abandoned(), 2);
    assert!(Arc::ptr_eq(&snapshot.abandoned[0], &first));
    assert!(Arc::ptr_eq(&snapshot.abandoned[1], &second));
    assert!(matches!(q.admit(refused), Err(durable::Error::Gap)));
}

#[tokio::test]
async fn canceled_active_runner_is_not_mistaken_for_empty_queue() {
    let f = Fixture::new().await;
    let q = ObservationQueue::new(f.store.account.clone(), f.source.clone());
    let input = work(&f);
    q.admit(input.clone()).unwrap();
    q.next().unwrap();
    drop(Running {
        queue: q.clone(),
        finished: false,
    });
    let snapshot = q.inventory().unwrap();
    assert!(snapshot.unknown && snapshot.active());
    assert!(Arc::ptr_eq(snapshot.active.as_ref().unwrap(), &input));
}

#[tokio::test]
async fn overflow_retains_all_accepted_work_and_registration_loss_is_unknown() {
    let f = Fixture::new().await;
    let h = f.source.ownership();
    let q = ObservationQueue::new(f.store.account.clone(), f.source.clone());
    let input = work(&f);
    for _ in 0..MAX_PENDING {
        q.admit(input.clone()).unwrap();
    }
    assert!(matches!(q.admit(input), Err(durable::Error::Gap)));
    assert_eq!(q.inventory().unwrap().abandoned(), MAX_PENDING);
    let before = h.snapshot(&f.store.account, &f.shell).unwrap();
    drop(q);
    assert_eq!(
        before.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Unknown)
    );
    assert!(matches!(
        h.snapshot(&f.store.account, &f.shell),
        Err(Refused::Unknown)
    ));
    assert_eq!(before.queues[0].abandoned(), MAX_PENDING);
}

#[tokio::test]
async fn foreign_account_work_is_refused_without_altering_original_queue() {
    let f = Fixture::new().await;
    let other = Fixture::new().await;
    let q = ObservationQueue::new(f.store.account.clone(), f.source.clone());
    let h = f.source.ownership();
    let before = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert!(matches!(q.admit(work(&other)), Err(durable::Error::Stale)));
    assert_eq!(before.validate(&h, &f.store.account, &f.shell), Ok(()));
    assert_eq!(q.inventory().unwrap().pending(), 0);
}

#[tokio::test]
async fn result_then_new_input_cannot_validate_an_old_equal_count_stamp() {
    let f = Fixture::new().await;
    let q = ObservationQueue::new(f.store.account.clone(), f.source.clone());
    let h = f.source.ownership();
    let before = h.snapshot(&f.store.account, &f.shell).unwrap();
    q.admit(work(&f)).unwrap();
    q.next().unwrap();
    let review = f.store.owner.review().await.unwrap();
    q.finish(Ok(Observed {
        before: review.clone(),
        after: review,
        barrier: None,
    }));
    q.take_result().unwrap().unwrap();
    assert!(q.next().is_none());
    let after = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert_eq!(before.queues[0].pending(), after.queues[0].pending());
    assert_eq!(before.queues[0].results(), after.queues[0].results());
    assert_eq!(
        before.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Changed)
    );
    q.admit(work(&f)).unwrap();
    assert_eq!(
        after.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Changed)
    );
}

#[tokio::test]
async fn poisoned_queue_is_unknown_and_cannot_look_empty() {
    let f = Fixture::new().await;
    let q = ObservationQueue::new(f.store.account.clone(), f.source.clone());
    let h = f.source.ownership();
    let before = h.snapshot(&f.store.account, &f.shell).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _locked = q.state.lock().unwrap();
            panic!("intentional isolated queue poison");
        }))
        .is_err()
    );
    assert!(matches!(
        h.snapshot(&f.store.account, &f.shell),
        Err(Refused::Unknown)
    ));
    assert!(q.take_result().is_none());
    assert_eq!(
        before.validate(&h, &f.store.account, &f.shell),
        Err(Refused::Unknown)
    );
}
