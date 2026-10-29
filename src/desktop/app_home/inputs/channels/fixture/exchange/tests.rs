use super::super::test_support as support;
use super::*;
use crate::bootstrap::test_support::pending;

#[tokio::test]
async fn receiver_closes_before_replacement_registration_and_waiter_cancellation_leaves_slot_owned()
{
    for awaiting in [false, true] {
        let (h, ticket, serial, revision, scope) = support::setup();
        let mut state = h.state;
        let Read::Registered(first) = injected(state, &ticket, &scope, support::register).await
        else {
            panic!("first exchange")
        };
        let prepared =
            directory::prepare_channels_page(&ticket.owner.ctx, "literal", &ticket.policy, None)
                .unwrap();
        let requests = directory::aggregate::Requests {
            users: Ok(None),
            channels: Ok(Some(prepared.clone())),
            mpims: Ok(None),
        };
        let direct = super::super::request_closed(&mut state.write(), &ticket, &prepared).unwrap();
        let combined =
            super::super::request_conversations_closed(&mut state.write(), &ticket, &requests)
                .unwrap();
        for receiver in [direct, combined] {
            assert!(
                matches!(receiver.await.unwrap(),Err(Error::Offline(message)) if message=="Home fixture read already pending")
            );
        }
        let mut first_read = Box::pin(async move {
            let mut first = first;
            first.receive().await
        });
        if awaiting {
            pending(first_read.as_mut()).await;
        }
        let old_nonce = state
            .peek()
            .app_home
            .channels
            .fixture
            .as_ref()
            .unwrap()
            .pending_owner
            .unwrap();
        let mut cancelled = Box::pin(injected(state, &ticket, &scope, support::register));
        pending(cancelled.as_mut()).await;
        drop(cancelled);
        assert!(pending_open(&state.peek()));
        assert_eq!(
            state
                .peek()
                .app_home
                .channels
                .fixture
                .as_ref()
                .unwrap()
                .requests
                .len(),
            1
        );
        state
            .write()
            .app_home
            .channels
            .picker
            .as_mut()
            .unwrap()
            .query_revision = revision + 100;
        let replacement_scope = crate::app_home::inputs::channels::search::fixture_scope(
            state,
            &ticket,
            serial,
            revision + 100,
        );
        let mut replacement = Box::pin(injected(
            state,
            &ticket,
            &replacement_scope,
            support::register,
        ));
        pending(replacement.as_mut()).await;
        drop(first_read);
        assert!(super::super::pending_closed(&state.peek()));
        let Read::Registered(mut second) = replacement.await else {
            panic!("replacement exchange")
        };
        let new_nonce = state
            .peek()
            .app_home
            .channels
            .fixture
            .as_ref()
            .unwrap()
            .pending_owner
            .unwrap();
        assert_ne!(new_nonce, old_nonce);
        assert!(pending_open(&state.peek()));
        support::deliver(state, Ok(json!({"second":true})));
        assert_eq!(second.receive().await.unwrap()["second"], true);
        drop(second);
        assert_eq!(
            state
                .peek()
                .app_home
                .channels
                .fixture
                .as_ref()
                .unwrap()
                .requests
                .len(),
            2
        );
        let Read::Registered(mut third) =
            injected(state, &ticket, &replacement_scope, support::register).await
        else {
            panic!("third exchange")
        };
        let mut fourth = Box::pin(injected(
            state,
            &ticket,
            &replacement_scope,
            support::register,
        ));
        pending(fourth.as_mut()).await;
        support::deliver(state, Ok(json!({"third":true})));
        assert_eq!(third.receive().await.unwrap()["third"], true);
        // Receipt alone does not grant registration while this receiver owns the lane.
        pending(fourth.as_mut()).await;
        drop(third);
        let Read::Registered(mut fourth) = fourth.await else {
            panic!("fourth exchange")
        };
        support::deliver(state, Ok(json!({"fourth":true})));
        assert_eq!(fourth.receive().await.unwrap()["fourth"], true);
        drop(fourth);
    }
}

#[tokio::test]
async fn reinstalled_harness_identity_and_open_unowned_slot_cannot_be_overwritten_by_old_exchange()
{
    let (h, ticket, _, _, scope) = support::setup();
    let mut state = h.state;
    let Read::Registered(first) = injected(state, &ticket, &scope, support::register).await else {
        panic!("first")
    };
    let nonce = first.nonce;
    let mut stale_waiter = Box::pin(injected(state, &ticket, &scope, support::register));
    pending(stale_waiter.as_mut()).await;
    let (send, mut receive) = oneshot::channel();
    {
        let mut s = state.write();
        let f = s.app_home.channels.fixture.as_mut().unwrap();
        f.lane = Arc::new(Mutex::new(()));
        f.pending = Some(send);
        f.pending_owner = Some(nonce);
    }
    drop(first);
    assert!(matches!(stale_waiter.await, Read::Refused));
    assert!(pending_open(&state.peek()));
    assert_eq!(
        state
            .peek()
            .app_home
            .channels
            .fixture
            .as_ref()
            .unwrap()
            .pending_owner,
        Some(nonce)
    );
    assert!(matches!(
        injected(state, &ticket, &scope, support::register).await,
        Read::Unavailable(_)
    ));
    assert_eq!(
        state
            .peek()
            .app_home
            .channels
            .fixture
            .as_ref()
            .unwrap()
            .requests
            .len(),
        1
    );
    support::deliver(state, Ok(json!({"newHarness":true})));
    assert_eq!((&mut receive).await.unwrap().unwrap()["newHarness"], true);

    // Destruction must still close the receiver and release its lane when the
    // shell is borrowed or its owning VirtualDom has already been disposed.
    for disposed in [false, true] {
        let (h, ticket, _, _, scope) = support::setup();
        let state = h.state;
        let Read::Registered(exchange) = injected(state, &ticket, &scope, support::register).await
        else {
            panic!("cleanup exchange")
        };
        let lane = exchange.lane.clone();
        if disposed {
            drop(h);
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(exchange))).is_ok()
            );
        } else {
            let borrowed = state.read();
            drop(exchange);
            assert!(super::super::pending_closed(&borrowed));
            drop(borrowed);
        }
        assert!(lane.try_lock().is_ok());
    }
}
