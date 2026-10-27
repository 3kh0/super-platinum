use super::*;
use dioxus::prelude::{ReadableExt, Signal, VNode, VirtualDom, WritableExt, use_signal};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context as PollContext, Poll, Waker},
};

type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn harness() -> (VirtualDom, Signal<ShellState>, Arc<Context>, Authority) {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| super::tests::setup().0);
            slot.set(Some(state));
            VNode::empty()
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let c = Arc::new(Context::capture(&state.peek()).unwrap());
    let a = Authority::capture(&state.peek().core, &c.session.team_id).unwrap();
    (dom, state, c, a)
}
fn poll<F: Future>(dom: &VirtualDom, future: Pin<&mut F>) -> Poll<F::Output> {
    dom.in_runtime(|| future.poll(&mut PollContext::from_waker(Waker::noop())))
}
fn api_method(request: &super_platinum_core::slack::PreparedRequest) -> &str {
    assert_eq!(request.method, "POST");
    request
        .url
        .split_once("/api/")
        .unwrap()
        .1
        .split('?')
        .next()
        .unwrap()
}
fn raw(method: &str, c: &Context) -> Value {
    match method {
        "client.gantryBoot" => {
            json!({"ok":true,"self":{"id":c.session.user_id},"team":{"id":c.session.team_id},"workspaces":[{"id":c.session.team_id,"prefs":{"display_real_names":true}}]})
        }
        "users.prefs.get" => json!({"ok":true,"prefs":{"display_real_names_override":1}}),
        "experiments.getByUser" => json!({"ok":true,"assignments":{}}),
        other => panic!("unexpected complete producer method {other}"),
    }
}

#[test]
fn successful_shared_producer_installs_complete_then_offline_reuses_without_read() {
    let (dom, state, c, a) = harness();
    let calls = RefCell::new(vec![]);
    let read = |r: super_platinum_core::slack::PreparedRequest| {
        let method = api_method(&r);
        calls.borrow_mut().push(method.to_owned());
        std::future::ready(Ok(raw(method, &c)))
    };
    let mut first = Box::pin(acquire_with(
        state,
        c.clone(),
        a.clone(),
        &|_| true,
        &|_| true,
        &read,
    ));
    let Poll::Ready(Ok(first)) = poll(&dom, first.as_mut()) else {
        panic!("complete producer failed")
    };
    assert_eq!(first.source, "fresh");
    assert_eq!(first.reads.len(), 3);
    let never = |_: super_platinum_core::slack::PreparedRequest| {
        panic!("offline reuse dispatched");
        #[allow(unreachable_code)]
        std::future::ready(Err(unavailable()))
    };
    let mut next = Box::pin(acquire_with(
        state,
        c.clone(),
        a,
        &|_| true,
        &|_| false,
        &never,
    ));
    let Poll::Ready(Ok(next)) = poll(&dom, next.as_mut()) else {
        panic!("offline reuse failed")
    };
    assert_eq!(next.source, "reused");
    assert!(next.reads.is_empty());
    assert_eq!(next.store.complete().reads, first.reads);
    assert_eq!(
        &*calls.borrow(),
        &[
            "client.gantryBoot",
            "users.prefs.get",
            "experiments.getByUser"
        ]
    );
}

#[test]
fn offline_missing_never_dispatches_or_installs() {
    let (dom, state, c, a) = harness();
    let calls = Cell::new(0);
    let read = |_| {
        calls.set(calls.get() + 1);
        std::future::ready(Err(unavailable()))
    };
    let mut future = Box::pin(acquire_with(state, c, a, &|_| true, &|_| false, &read));
    assert!(matches!(poll(&dom, future.as_mut()), Poll::Ready(Err(_))));
    assert_eq!(calls.get(), 0);
    assert!(Store::capture(&state.peek()).is_none());
}

#[test]
fn joined_call_consumes_no_reads_and_dropping_waiter_does_not_cancel_producer() {
    let (dom, state, c, a) = harness();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let receiver = RefCell::new(Some(rx));
    let calls = RefCell::new(vec![]);
    let read = |r: super_platinum_core::slack::PreparedRequest| {
        let method = api_method(&r);
        calls.borrow_mut().push(method.to_owned());
        let response = raw(method, &c);
        let hold = if method == "users.prefs.get" {
            receiver.borrow_mut().take()
        } else {
            None
        };
        async move {
            if let Some(hold) = hold {
                hold.await.map_err(|_| unavailable())?;
            }
            Ok(response)
        }
    };
    let mut first = Box::pin(acquire_with(
        state,
        c.clone(),
        a.clone(),
        &|_| true,
        &|_| true,
        &read,
    ));
    assert!(poll(&dom, first.as_mut()).is_pending());
    let never = |_| std::future::ready(Err(unavailable()));
    let mut joined = Box::pin(acquire_with(
        state,
        c.clone(),
        a.clone(),
        &|_| true,
        &|_| true,
        &never,
    ));
    assert!(poll(&dom, joined.as_mut()).is_pending());
    drop(joined);
    let mut joined = Box::pin(acquire_with(
        state,
        c.clone(),
        a,
        &|_| true,
        &|_| true,
        &never,
    ));
    assert!(poll(&dom, joined.as_mut()).is_pending());
    tx.send(()).unwrap();
    assert!(matches!(poll(&dom, first.as_mut()), Poll::Ready(Ok(_))));
    let Poll::Ready(Ok(joined)) = poll(&dom, joined.as_mut()) else {
        panic!("join did not finish")
    };
    assert_eq!(joined.source, "reused");
    assert!(joined.reads.is_empty());
    assert_eq!(calls.borrow().len(), 3);
}

#[test]
fn cancelled_producer_wakes_joiner_without_publishing_partial() {
    let (dom, state, c, a) = harness();
    let pending = |_| std::future::pending::<Result<Value, Error>>();
    let mut first = Box::pin(acquire_with(
        state,
        c.clone(),
        a.clone(),
        &|_| true,
        &|_| true,
        &pending,
    ));
    assert!(poll(&dom, first.as_mut()).is_pending());
    let mut joined = Box::pin(acquire_with(
        state,
        c.clone(),
        a,
        &|_| true,
        &|_| true,
        &pending,
    ));
    assert!(poll(&dom, joined.as_mut()).is_pending());
    drop(first);
    assert!(matches!(poll(&dom, joined.as_mut()), Poll::Ready(Err(_))));
    assert!(Store::capture(&state.peek()).is_none());
    assert_eq!(snapshot(&state.peek())["pending"], false);
}

#[test]
fn retirement_between_reads_blocks_next_read_and_atomic_publication() {
    for offline in [false, true] {
        let (dom, state, c, a) = harness();
        let calls = Cell::new(0);
        let connected = Cell::new(true);
        let read = |r: super_platinum_core::slack::PreparedRequest| {
            calls.set(calls.get() + 1);
            if offline {
                connected.set(false);
            } else {
                let mut state = state;
                state.write().app_home.workflow_names_revision += 1;
            }
            std::future::ready(Ok(raw(api_method(&r), &c)))
        };
        let online = |_: &ShellState| connected.get();
        let mut future = Box::pin(acquire_with(state, c.clone(), a, &|_| true, &online, &read));
        assert!(matches!(poll(&dom, future.as_mut()), Poll::Ready(Err(_))));
        assert_eq!(calls.get(), 1);
        assert!(Store::capture(&state.peek()).is_none());
    }
}

#[test]
fn startup_reservation_survives_pending_but_not_accepted_preference_change() {
    let (mut s, c, a) = super::tests::setup();
    let startup = Startup::reserve(&mut s, c.clone(), a.clone())
        .unwrap()
        .unwrap();
    let p = Pending::capture(&mut s, c.clone(), a.clone()).unwrap();
    assert!(startup.current(&s));
    assert!(
        Startup::reserve(&mut s, c.clone(), a.clone())
            .unwrap()
            .is_none()
    );
    drop(p);
    assert!(
        Startup::reserve(&mut s, c.clone(), a.clone())
            .unwrap()
            .is_none()
    );
    s.core
        .workspaces
        .get_mut(&c.session.team_id)
        .unwrap()
        .cfm_payload
        .user_prefs
        .insert("frecency".into(), json!({"changed":true}));
    assert!(!startup.current(&s));
    assert!(Startup::reserve(&mut s, c, a).unwrap().is_some());
}

#[test]
fn admitted_event_retires_reservation_even_before_first_future_poll() {
    let (mut s, c, a) = super::tests::setup();
    let startup = Startup::reserve(&mut s, c.clone(), a).unwrap().unwrap();
    let generation = s.core.workspaces[&c.session.team_id].rt_generation;
    realtime(
        &mut s,
        &c.session.team_id,
        generation,
        &RtEvent::PreferencesChanged {
            name: "frecency".into(),
            value: json!({}),
        },
    );
    assert!(!startup.current(&s));
}
