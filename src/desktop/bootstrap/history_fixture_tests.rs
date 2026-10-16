use super::*;
use dioxus::prelude::*;
use std::{cell::Cell, rc::Rc};

#[tokio::test]
async fn fixture_older_history_returns_before_state_or_request_admission() {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            slot.set(Some(state));
            VNode::empty()
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let before = state.peek().core.account_epoch;
    load_older_in_mode(state, true).await;
    assert_eq!(state.peek().core.account_epoch, before);
    assert!(!state.peek().loading_older);
    // A fixture scroll callback queued just before teardown must not even
    // borrow its retired Signal, much less use the synthetic session.
    drop(dom);
    load_older_in_mode(state, true).await;
}
