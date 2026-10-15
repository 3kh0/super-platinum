//! Real AdoptionPanel subscriptions under the pinned Dioxus VDom. No desktop,
//! process, environment mutation, worker, Session, Transport or fake admission.
use super::*;
use std::{cell::Cell, rc::Rc};
thread_local! {
    static RENDERS: Cell<usize> = const { Cell::new(0) };
    static PANEL: Cell<Option<Signal<controller::Panel>>> = const { Cell::new(None) };
}
pub(super) fn observed(panel: Signal<controller::Panel>) {
    RENDERS.with(|v| v.set(v.get() + 1));
    PANEL.with(|v| v.set(Some(panel)));
}
fn renders() -> usize {
    RENDERS.with(Cell::get)
}
fn harness() -> (VirtualDom, Signal<ShellState>, Signal<controller::Panel>) {
    assert!(
        BINDING.get().is_none(),
        "test has no native fixture binding"
    );
    RENDERS.with(|v| v.set(0));
    PANEL.with(|v| v.set(None));
    let slot = Rc::new(Cell::new(None));
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Rc<Cell<Option<Signal<ShellState>>>>| {
            let state = use_signal(|| ShellState::opening(crate::media::MediaRegistry::default()));
            slot.set(Some(state));
            rsx! { AdoptionPanel { state } }
        },
        copy,
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap(), PANEL.with(Cell::get).unwrap())
}
#[test]
fn shell_replacement_republishes_even_before_binding_exists() {
    let (mut dom, mut state, _) = harness();
    let before = renders();
    dom.in_runtime(|| *state.write() = ShellState::opening(crate::media::MediaRegistry::default()));
    dom.render_immediate_to_vec();
    assert_eq!(renders(), before + 1);
    assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
}
#[test]
fn each_host_frontier_publication_invalidates_the_actual_panel_only() {
    let (mut dom, state, mut panel) = harness();
    dom.in_runtime(|| panel.write().busy = true);
    dom.render_immediate_to_vec();
    let view = state.peek().draft_writes.view_id;
    for expected in 1..=4 {
        let before = renders();
        dom.in_runtime(|| controller::publish(panel));
        dom.render_immediate_to_vec();
        assert_eq!(renders(), before + 1);
        assert_eq!(panel.peek().publication, expected);
        assert!(panel.peek().busy);
        assert_eq!(state.peek().draft_writes.view_id, view);
    }
}
#[test]
fn idle_flush_does_not_poll_and_normal_completion_still_publishes() {
    let (mut dom, _, mut panel) = harness();
    let before = renders();
    for _ in 0..4 {
        dom.render_immediate_to_vec();
    }
    assert_eq!(renders(), before);
    dom.in_runtime(|| {
        let mut p = panel.write();
        p.busy = false;
        p.message = Some("Saved drafts reloaded.".into());
    });
    dom.render_immediate_to_vec();
    assert_eq!(renders(), before + 1);
}
