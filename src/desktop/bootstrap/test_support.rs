//! Isolated in-memory shell and credentials for async search/profile journeys.
use crate::{
    media::MediaRegistry,
    state::{Overlay, ShellState},
};
use dioxus::prelude::*;
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::Poll,
};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    search::SearchKind,
    slack::Transport,
};

pub(crate) struct Harness {
    pub _dom: VirtualDom,
    pub state: Signal<ShellState>,
    pub transport: Arc<Transport>,
    pub session: WorkspaceSession,
}
pub(crate) fn transport(cookie: &str) -> Arc<Transport> {
    #[cfg(feature = "protected-native-fixture")]
    {
        Arc::new(Transport::disconnected_fixture(cookie))
    }
    #[cfg(not(feature = "protected-native-fixture"))]
    {
        Arc::new(Transport::new(cookie).unwrap())
    }
}
pub(crate) fn harness() -> Harness {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let transport = transport("synthetic");
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    shell.core.active_account = Some("people-profile-test".into());
    shell.core.transport = Some(transport.clone());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), session.clone())].into(),
    });
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("alex@example.test".into());
    shell.set_search_kind(SearchKind::People);
    shell.core.composer.text = "Keep draft".into();
    type Props = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
    );
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): Props| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_to_vec();
    Harness {
        _dom: dom,
        state: slot.get().unwrap(),
        transport,
        session,
    }
}
pub(crate) async fn pending<F: Future>(mut work: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(work.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
pub(crate) fn users(state: Signal<ShellState>) -> serde_json::Value {
    serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap()
}
