use super::*;
use crate::drafts::protected::operation::tests::Fixture;
use std::cell::Cell;
fn entry(
    root: &Root,
    operation: Operation,
    future: impl Future<Output = ()> + 'static,
) -> Rc<Entry> {
    let value = Entry::new(operation, async move {
        future.await;
        Ok(Status::ConsumedAwaitingDelivery)
    });
    root.entries.borrow_mut().push(value.clone());
    value
}
fn once(entry: &Rc<Entry>) -> Poll<()> {
    let mut cx = Context::from_waker(std::task::Waker::noop());
    poll(&Rc::downgrade(entry), &mut cx)
}
#[tokio::test]
async fn shutdown_before_first_poll_retains_unknown_and_original_input() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    let ran = Rc::new(Cell::new(false));
    let root = Root::default();
    let flag = ran.clone();
    let e = entry(&root, operation.clone(), async move {
        flag.set(true);
    });
    drop(root);
    assert!(!ran.get());
    assert_eq!(operation.ready().await.unwrap(), Status::Unknown);
    assert!(once(&e).is_ready());
    assert_eq!(operation.record().local.composer.text, "Original");
}
#[tokio::test]
async fn poll_can_reenter_entry_without_a_borrow_held_across_user_code() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    let root = Rc::new(Root::default());
    let weak = Rc::downgrade(&root);
    let checked = Rc::new(Cell::new(false));
    let flag = checked.clone();
    let e = entry(
        &root,
        operation.clone(),
        std::future::poll_fn(move |_| {
            let root = weak.upgrade().unwrap();
            let entries = root.entries.try_borrow().unwrap();
            let mut state = entries[0]
                .state
                .try_borrow_mut()
                .expect("poll must not retain entry borrow");
            assert!(state.polling);
            assert!(state.future.is_none());
            state.polling = true;
            flag.set(true);
            Poll::<()>::Pending
        }),
    );
    assert!(once(&e).is_pending());
    assert!(checked.get());
    assert!(e.state.borrow().future.is_some());
    assert!(!e.state.borrow().polling);
    drop(root);
    assert_eq!(operation.ready().await.unwrap(), Status::Unknown);
}
#[tokio::test]
async fn dropping_root_inside_poll_cannot_restore_future_or_finish_unknown() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    let holder = Rc::new(RefCell::new(Some(Rc::new(Root::default()))));
    let weak_holder = Rc::downgrade(&holder);
    let late = operation.clone();
    let root = holder.borrow().as_ref().unwrap().clone();
    let e = entry(
        &root,
        operation.clone(),
        std::future::poll_fn(move |_| {
            weak_holder.upgrade().unwrap().borrow_mut().take();
            late.finish(Ok(Status::Finished));
            Poll::<()>::Pending
        }),
    );
    drop(root);
    assert!(once(&e).is_pending());
    assert!(e.state.borrow().future.is_none());
    assert!(!operation.current());
    assert_eq!(operation.ready().await.unwrap(), Status::Unknown);
}
#[tokio::test]
async fn executor_weak_reference_does_not_keep_root_alive() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    let root = Rc::new(Root::default());
    let weak_root = Rc::downgrade(&root);
    let e = entry(&root, operation.clone(), std::future::pending());
    let executor = Rc::downgrade(&e);
    drop(e);
    drop(root);
    assert!(weak_root.upgrade().is_none());
    assert!(executor.upgrade().is_none());
    assert_eq!(operation.ready().await.unwrap(), Status::Unknown);
}

#[tokio::test]
async fn actual_owned_consume_completes_in_root_after_result_waiter_drops() {
    use crate::drafts::protected::{Action, Visit};
    use super_platinum_core::drafts::sync::ConsumeKind;
    let fixture = Fixture::new().await;
    // Actual isolated worker proofs; no constructed Review or stream flag.
    let stream = fixture.verified_stream().await;
    let operation = fixture.operation().await;
    let editor = fixture
        .account
        .retained_editors()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let lease = editor
        .prepare_review(Visit::new(), &operation.record().local)
        .await
        .unwrap();
    let root = Root::default();
    let owned = operation.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let e = entry(&root, operation.clone(), async move {
        let value = editor
            .apply_review_owned(
                lease,
                Action::Consume(ConsumeKind::Send),
                owned.clone(),
                move || Ok(Some(stream)),
            )
            .await
            .unwrap();
        owned.finish(Ok(Status::ConsumedAwaitingDelivery));
        let _ = sender.send(value);
    });
    // The one-use lease lives in Entry even before the executor's first poll.
    drop(receiver);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while once(&e).is_pending() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        operation.ready().await.unwrap(),
        Status::ConsumedAwaitingDelivery
    );
    let durable = fixture.owner.review().await.unwrap();
    assert!(durable.local().empty());
    assert!(durable.row().state().consumption().is_some());
    let evidence = operation.record().evidence.lock().unwrap();
    assert!(evidence.consumed.as_ref().unwrap().after.same(&durable));
    assert!(evidence.attempt.is_none());
    assert!(evidence.receipt.is_none());
    drop(evidence);
    assert!(matches!(
        root.acknowledge(&operation),
        Err(durable::Error::Pending)
    ));
}

#[tokio::test]
async fn full_root_returns_exact_request_pair_before_any_capture() {
    use crate::drafts::protected::{Visit, operation::Request};
    let fixture = Fixture::new().await;
    let root = Root::default();
    for _ in 0..MAX_OPERATIONS {
        entry(&root, fixture.operation().await, std::future::pending());
    }
    let review = fixture.owner.review().await.unwrap();
    let snapshot = review.row().state().desired().unwrap().clone();
    let visit = Visit::new();
    let route_owner = Rc::new(Cell::new(false));
    // This is the same ownership-returning boundary start invokes on the whole
    // Start structure, before its capture closure or Schedule clone executes.
    let request = (
        Request::Schedule {
            snapshot: snapshot.clone(),
            post_at: 12345,
        },
        visit.clone(),
        route_owner.clone(),
    );
    let (error, returned) = match root.admit_request(request) {
        Ok(_) => panic!("full"),
        Err(value) => value,
    };
    assert_eq!(error, durable::Error::Pending);
    assert!(
        matches!(returned.0, Request::Schedule { snapshot: value, post_at: 12345 } if value == snapshot)
    );
    assert!(returned.1.same(&visit));
    assert!(returned.1.current());
    assert!(Rc::ptr_eq(&returned.2, &route_owner));
    assert!(!route_owner.get());
    assert_eq!(
        fixture.account.retained_operations().unwrap().len(),
        MAX_OPERATIONS
    );
}
