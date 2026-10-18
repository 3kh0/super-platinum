//! Actual worker registration custody. The barrier is explicit unit setup;
//! these tests do not manufacture RendererApplied or claim native projection.
use super::super::owned_route::{Held, o};
use super::*;

async fn setup() -> (Arc<Editor>, o::Operations, ProjectionTicket) {
    let initialized = tokio::task::spawn_blocking(|| {
        let route = o::OwnedRoute::fresh_fixture().unwrap();
        let before = route.inspect().unwrap().wait().unwrap();
        route.commit(&before).unwrap().wait().unwrap();
        route
            .inspect()
            .unwrap()
            .wait()
            .unwrap()
            .open()
            .unwrap()
            .initialize_cold(&o::FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap()
    })
    .await
    .unwrap();
    let ops = initialized.operations();
    let account = Account::from_retained(initialized);
    let key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    };
    let owner = account.owner(key).unwrap();
    let local = Held::new(ops.local_inventory(owner.scope().clone()))
        .observed_success()
        .await
        .unwrap();
    Held::new(ops.enroll(local))
        .observed_success()
        .await
        .unwrap();
    let editor = Editor::open(owner).await.unwrap();
    let captured = editor.capture(Visit::new()).unwrap();
    let review = captured.review.clone();
    let barrier = Arc::new(ProjectionBarrier {
        capture: captured,
        before: review.clone(),
        after: review.clone(),
        id: "owned-registration-test".into(),
        renew_writer: true,
        owned_finish: None,
        authority: None,
    });
    {
        let mut state = editor.state.lock().unwrap();
        state.barrier = Some(barrier.clone());
        state.parked = true;
    }
    (editor, ops, ProjectionTicket { barrier, review })
}

#[tokio::test]
async fn same_projection_rejoins_one_actual_registration_and_does_not_retire_on_observation() {
    let (editor, ops, ticket) = setup().await;
    let before = ops.retained().len();
    let first = editor
        .projection_registration(&ticket, &ticket.review)
        .unwrap()
        .unwrap();
    let second = editor
        .projection_registration(&ticket, &ticket.review)
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(ops.retained().len(), before + 1);
    let writer = first.held.value().await.unwrap().unwrap();
    assert_eq!(writer.scope(), editor.scope());
    assert_eq!(
        writer.enrollment(),
        ticket.review.row().lineage().enrollment()
    );
    assert_eq!(ops.retained().len(), before + 1);
    assert!(Arc::ptr_eq(
        editor
            .state
            .lock()
            .unwrap()
            .projection_registration
            .as_ref()
            .unwrap(),
        &first
    ));
    assert!(matches!(
        editor.owner.client(),
        Err(durable::Error::Unsupported)
    ));
}

#[tokio::test]
async fn stale_serial_and_replacement_barrier_never_replace_the_retained_registration() {
    let (editor, ops, ticket) = setup().await;
    let first = editor
        .projection_registration(&ticket, &ticket.review)
        .unwrap()
        .unwrap();
    first.held.value().await.unwrap().unwrap();
    let count = ops.retained().len();
    {
        let mut state = editor.state.lock().unwrap();
        state.serial += 1;
    }
    assert!(matches!(
        editor.projection_registration(&ticket, &ticket.review),
        Err(durable::Error::Stale)
    ));
    {
        let mut state = editor.state.lock().unwrap();
        state.serial -= 1;
    }
    let replacement = Arc::new(ProjectionBarrier {
        capture: ticket.barrier.capture.clone(),
        before: ticket.review.clone(),
        after: ticket.review.clone(),
        id: "replacement".into(),
        renew_writer: true,
        owned_finish: None,
        authority: None,
    });
    editor.state.lock().unwrap().barrier = Some(replacement.clone());
    let replacement = ProjectionTicket {
        barrier: replacement,
        review: ticket.review.clone(),
    };
    assert!(matches!(
        editor.projection_registration(&replacement, &replacement.review),
        Err(durable::Error::Pending)
    ));
    assert_eq!(ops.retained().len(), count);
    assert!(Arc::ptr_eq(
        editor
            .state
            .lock()
            .unwrap()
            .projection_registration
            .as_ref()
            .unwrap(),
        &first
    ));
}
