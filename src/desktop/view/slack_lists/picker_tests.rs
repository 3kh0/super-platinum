//! Actual ListDialog VDom mutations, no WebView, transport or session.
//! Pinned Dioxus writes select.value before its dynamic options; each option
//! must carry its own selected property when it is inserted afterwards.
use super::*;
use dioxus::core::{AttributeValue, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn harness(narrow: bool) -> (VirtualDom, Signal<ShellState>, Mutations) {
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |(slot, narrow): (Slot, bool)| {
            let state = use_signal(move || {
                let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
                crate::slack_lists::fixture::view_rename::install(
                    &mut shell,
                    if narrow {
                        "slack-list-view-rename-narrow"
                    } else {
                        "slack-list-view-rename"
                    },
                );
                shell
            });
            slot.set(Some(state));
            dialog(state)
        },
        (copy, narrow),
    );
    let edits = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    (dom, state, edits)
}
fn selected_value(edits: &Mutations, value: &str) -> bool {
    edits.edits.iter().any(|edit|{let Mutation::SetAttribute{id,name:"value",value:AttributeValue::Text(v),..}=edit else{return false;};v==value&&edits.edits.iter().any(|e|matches!(e,Mutation::SetAttribute{id:other,name:"selected",value:AttributeValue::Bool(true),..} if other==id))})
}
#[test]
fn custom_picker_option_is_selected_after_parent_value_on_actual_mount() {
    for narrow in [false, true] {
        let (_dom, state, edits) = harness(narrow);
        assert_eq!(
            crate::slack_lists::presentation(&state.peek())
                .unwrap()
                .selected_view
                .as_deref(),
            Some("V_CUSTOM")
        );
        assert!(selected_value(&edits, "V_CUSTOM"));
        assert!(!selected_value(&edits, "V_OTHER"));
        assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
    }
}
#[test]
fn reopened_actual_dialog_reinserts_selected_option_for_new_visit() {
    let (mut dom, mut state, _) = harness(false);
    dom.in_runtime(|| crate::slack_lists::close_shell(&mut state.write()));
    dom.render_immediate_to_vec();
    // Reopen through the actual keyed dialog path, then inspect fresh option writes.
    dom.in_runtime(|| {
        crate::slack_lists::fixture::view_rename::install(
            &mut state.write(),
            "slack-list-view-rename",
        );
    });
    dom.mark_dirty(dioxus::core::ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert_eq!(
        crate::slack_lists::presentation(&state.peek())
            .unwrap()
            .selected_view
            .as_deref(),
        Some("V_CUSTOM")
    );
    assert!(selected_value(&edits, "V_CUSTOM"));
}
