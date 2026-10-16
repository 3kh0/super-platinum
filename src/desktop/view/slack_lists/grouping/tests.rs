//! Actual shared editor VDOM and native-form event payloads; no WebView or IO.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations, ScopeId};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn harness() -> (VirtualDom, Signal<ShellState>, Mutations) {
    // Headless VDOM does not run Desktop's serialized event-converter setup.
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    });
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-view-grouping",
                )
            });
            slot.set(Some(state));
            super::super::rename::panel(state)
        },
        copy,
    );
    let edits = dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), edits)
}
fn select(edits: &Mutations) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|e| match e {
            Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
            _ => None,
        })
        .expect("Actual grouping select change listener")
}
fn event(dom: &VirtualDom, id: ElementId, value: &str) {
    let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.runtime().handle_event(
        "change",
        dioxus::core::Event::new(payload.into_any(), true),
        id,
    );
}
fn desired(state: Signal<ShellState>) -> ViewGroupingChoice {
    crate::slack_lists::rename::presentation(&state.peek())
        .unwrap()
        .view_grouping
        .unwrap()
        .desired
}
#[test]
fn actual_select_events_retain_control_node_and_render_selected_option() {
    let (mut dom, state, initial) = harness();
    let id = select(&initial);
    event(&dom, id, "C_SELECT");
    assert_eq!(
        desired(state),
        ViewGroupingChoice::Column {
            column_id: "C_SELECT".into()
        }
    );
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(!edits.edits.iter().any(
        |e| matches!(e,Mutation::Remove{id:old}|Mutation::ReplaceWith{id:old,..} if *old==id)
    ));
    assert!(edits.edits.iter().any(|e|matches!(e,Mutation::SetAttribute{id:node,name:"value",value:AttributeValue::Text(value),..} if *node==id&&value=="C_SELECT")));
    assert!(edits.edits.iter().any(|e| matches!(
        e,
        Mutation::SetAttribute {
            name: "selected",
            value: AttributeValue::Bool(true),
            ..
        }
    )));
    event(&dom, id, "");
    assert_eq!(desired(state), ViewGroupingChoice::None);
    dom.in_runtime(|| {
        assert!(
            crate::slack_lists::rename::fixture::grouping::close_static_for_test(&state.peek())
        );
    });
}
#[test]
fn consecutive_old_render_choice_event_cannot_override_new_epoch() {
    let (dom, state, initial) = harness();
    let id = select(&initial);
    event(&dom, id, "C_SELECT");
    event(&dom, id, "");
    assert_eq!(
        desired(state),
        ViewGroupingChoice::Column {
            column_id: "C_SELECT".into()
        }
    );
    dom.in_runtime(|| {
        assert!(
            crate::slack_lists::rename::fixture::grouping::close_static_for_test(&state.peek())
        );
    });
}
