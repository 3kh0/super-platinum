use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn harness() -> (VirtualDom, Signal<ShellState>, Mutations) {
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list",
                );
                assert!(assignment::fixture::seed(&mut shell));
                shell
            });
            slot.set(Some(state));
            let view = crate::slack_lists::presentation(&state.read()).unwrap();
            if let Some(review) = view.editor.and_then(|e| e.assignment) {
                rsx! {AssignmentReview {state,review}}
            } else {
                rsx! {}
            }
        },
        copy,
    );
    let edits = dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), edits)
}
fn node(edits: &Mutations, attribute: &str, value: &str) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|e| match e {
            Mutation::SetAttribute {
                name,
                value: AttributeValue::Text(v),
                id,
                ..
            } if *name == attribute && v == value => Some(*id),
            _ => None,
        })
        .unwrap()
}
#[test]
fn actual_grant_select_has_explicit_selected_option_and_owned_callback() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let (mut dom, state, edits) = harness();
    assert_eq!(
        edits
            .edits
            .iter()
            .filter(|e| matches!(
                e,
                Mutation::SetAttribute {
                    name: "selected",
                    value: AttributeValue::Bool(true),
                    ..
                }
            ))
            .count(),
        2
    );
    let person = node(&edits, "data-assignment-user", "U00000002");
    let event = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        "read".into(),
        Vec::new(),
    ));
    dom.runtime().handle_event(
        "change",
        dioxus::core::Event::new(event.into_any(), true),
        person,
    );
    dom.render_immediate_to_vec();
    let review = crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .editor
        .unwrap()
        .assignment
        .unwrap();
    assert_eq!(
        review.people[0].grant,
        super_platinum_core::slack::api::AssigneeGrant::Read
    );
    assert_eq!(
        review.people[1].grant,
        super_platinum_core::slack::api::AssigneeGrant::Write
    );
    assert!(state.peek().core.transport.is_none() && state.peek().core.session.is_none());
}
#[test]
fn actual_retired_not_now_callback_cannot_convert_the_replaced_editor() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let (dom, state, edits) = harness();
    let button = node(&edits, "data-assignment-action", "not-now");
    let generation = crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .editor
        .unwrap()
        .generation;
    dom.in_runtime(|| crate::slack_lists::cancel_editor(state, generation));
    let event = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.runtime().handle_event(
        "click",
        dioxus::core::Event::new(event.into_any(), true),
        button,
    );
    let view = crate::slack_lists::presentation(&state.peek()).unwrap();
    assert!(view.editor.is_none());
    assert!(view.mutation.is_none());
    assert!(!assignment::blocks_mutation(&state.peek()));
}

#[test]
fn actual_abandon_callback_releases_review_without_assigning() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let (mut dom, state, edits) = harness();
    let button = node(&edits, "data-assignment-action", "abandon");
    let event = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.runtime().handle_event(
        "click",
        dioxus::core::Event::new(event.into_any(), true),
        button,
    );
    dom.render_immediate_to_vec();
    let view = crate::slack_lists::presentation(&state.peek()).unwrap();
    assert!(view.editor.is_none() && view.mutation.is_none());
    assert!(!assignment::blocks_mutation(&state.peek()));
}
#[test]
fn actual_abandon_callback_cannot_retire_replaced_editor() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let (dom, mut state, edits) = harness();
    let button = node(&edits, "data-assignment-action", "abandon");
    let generation = crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .editor
        .unwrap()
        .generation;
    dom.in_runtime(|| {
        crate::slack_lists::cancel_editor(state, generation);
        assert!(assignment::fixture::seed(&mut state.write()));
    });
    let newer = assignment::recovery(&state.peek()).unwrap().serial;
    let event = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.runtime().handle_event(
        "click",
        dioxus::core::Event::new(event.into_any(), true),
        button,
    );
    assert_eq!(assignment::recovery(&state.peek()).unwrap().serial, newer);
    assert!(
        crate::slack_lists::presentation(&state.peek())
            .unwrap()
            .mutation
            .is_none()
    );
}

#[test]
fn actual_detached_notice_releases_retained_fence_without_reopening_cell() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list",
                );
                assert!(assignment::fixture::seed(&mut shell));
                assignment::fixture::retain_detached(&mut shell);
                shell
            });
            slot.set(Some(state));
            recovery_notice(state)
        },
        slot.clone(),
    );
    let edits = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    assert!(
        crate::slack_lists::presentation(&state.peek())
            .unwrap()
            .editor
            .is_none()
    );
    assert!(assignment::blocks_mutation(&state.peek()));
    let button = node(&edits, "data-assignment-action", "abandon");
    let event = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.runtime().handle_event(
        "click",
        dioxus::core::Event::new(event.into_any(), true),
        button,
    );
    dom.render_immediate_to_vec();
    assert!(!assignment::blocks_mutation(&state.peek()));
    assert!(
        crate::slack_lists::presentation(&state.peek())
            .unwrap()
            .mutation
            .is_none()
    );
}
