//! Pinned Dioxus borrow-lifetime regression, not native DOM readiness evidence.
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn snapshot(value: &Value) -> Result<Value, String> {
    Ok(value.clone())
}

async fn ready(release: Rc<Cell<bool>>) {
    std::future::poll_fn(move |_| {
        if release.get() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await
}

// Deliberately retains the original failing if-let boundary as a negative control.
async fn original(state: Signal<Value>, release: Rc<Cell<bool>>) -> Result<Option<u64>, String> {
    if let Some(g) = snapshot(&state.peek())?["generation"].as_u64() {
        ready(release).await;
        return Ok(Some(g));
    }
    Ok(None)
}

// The exact owned-statement boundary used by open(); no WebView or timer is mocked.
async fn owned(state: Signal<Value>, release: Rc<Cell<bool>>) -> Result<Option<u64>, String> {
    let generation = snapshot(&state.peek())?["generation"].as_u64();
    if let Some(g) = generation {
        ready(release).await;
        return Ok(Some(g));
    }
    Ok(None)
}

#[test]
fn owned_generation_releases_real_signal_before_pending_ready_wait() {
    let mut dom = VirtualDom::new(|| rsx! { div {} });
    dom.rebuild_in_place();
    dom.in_scope(ScopeId::ROOT, || {
        let mut state = Signal::new(json!({"generation": 41}));
        let release = Rc::new(Cell::new(false));
        let mut cx = Context::from_waker(Waker::noop());
        let mut bad = Box::pin(original(state, release.clone()));
        assert!(bad.as_mut().poll(&mut cx).is_pending());
        assert!(state.try_write().is_err());
        drop(bad);
        assert!(state.try_write().is_ok());

        let mut good = Box::pin(owned(state, release.clone()));
        assert!(good.as_mut().poll(&mut cx).is_pending());
        state
            .try_write()
            .expect("read borrow must end before await")["generation"] = json!(42);
        release.set(true);
        assert_eq!(good.as_mut().poll(&mut cx), Poll::Ready(Ok(Some(41))));
        assert_eq!(state.peek()["generation"], 42);
        drop(good);
        state.set(Value::Null);
        let mut absent = Box::pin(owned(state, Rc::new(Cell::new(false))));
        assert_eq!(absent.as_mut().poll(&mut cx), Poll::Ready(Ok(None)));
        assert!(state.try_write().is_ok());
    });
}
