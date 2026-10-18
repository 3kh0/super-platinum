//! Real pinned Dioxus Signal borrowing across the deliberately held child.
//! This is a borrow-lifetime regression, not native worker execution evidence.
use super::*;
use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use super_platinum_core::slack::client::PreparedRequest;

async fn held(request: PreparedRequest, release: Rc<Cell<bool>>) -> String {
    std::future::poll_fn(move |_| {
        if release.get() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    request.method.into()
}
// Retain the failing call boundary as a negative control.
async fn original(
    state: Signal<ShellState>,
    permit: ListDefaultViewPreferencePermit,
    release: Rc<Cell<bool>>,
) -> String {
    held(
        api::list_default_view_preference(&state.peek().core.client, &fixture::session(), permit),
        release,
    )
    .await
}
async fn owned_write(
    state: Signal<ShellState>,
    permit: ListDefaultViewPreferencePermit,
    release: Rc<Cell<bool>>,
) -> String {
    let request =
        api::list_default_view_preference(&state.peek().core.client, &fixture::session(), permit);
    held(request, release).await
}
async fn owned_read(state: Signal<ShellState>, release: Rc<Cell<bool>>) -> String {
    let request =
        api::list_default_view_preference_read(&state.peek().core.client, &fixture::session());
    held(request, release).await
}
fn permit(shell: &ShellState) -> ListDefaultViewPreferencePermit {
    let doc = &shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document;
    let prefs = json!({"ok":true,"prefs":{"lists_default_view":"{}"}});
    let intent = ListDefaultViewPreference::capture(&prefs, doc, "V_LANE").unwrap();
    ListDefaultViewPreferenceAttempt::new(intent)
        .admit(&prefs, doc)
        .unwrap()
}
#[test]
fn request_statement_releases_real_signal_for_parent_reset_while_child_is_held() {
    let mut dom = VirtualDom::new(|| rsx! {div{}});
    dom.rebuild_in_place();
    dom.in_scope(ScopeId::ROOT, || {
        let shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-board-refinement-save",
        );
        let mut state = Signal::new(shell);
        let release = Rc::new(Cell::new(false));
        let mut cx = Context::from_waker(Waker::noop());
        let old_permit = permit(&state.peek());
        let mut bad = Box::pin(original(state, old_permit, release.clone()));
        assert!(bad.as_mut().poll(&mut cx).is_pending());
        assert!(
            state.try_write().is_err(),
            "original awaited argument retains the real read borrow"
        );
        drop(bad);
        assert!(state.try_write().is_ok());
        let new_permit = permit(&state.peek());
        let mut good = Box::pin(owned_write(state, new_permit, release.clone()));
        assert!(good.as_mut().poll(&mut cx).is_pending());
        state
            .try_write()
            .expect("parent reset can borrow while child held")
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .layout
            .refinements
            .reset();
        release.set(true);
        assert_eq!(good.as_mut().poll(&mut cx), Poll::Ready("POST".into()));
        drop(good);
        release.set(false);
        let mut read = Box::pin(owned_read(state, release.clone()));
        assert!(read.as_mut().poll(&mut cx).is_pending());
        assert!(
            state.try_write().is_ok(),
            "read request also owns its built form before awaiting"
        );
        release.set(true);
        assert_eq!(read.as_mut().poll(&mut cx), Poll::Ready("POST".into()));
    });
}
