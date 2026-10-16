//! Actual product RSX callbacks through the pinned serialized event converter.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations, ScopeId};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn node(edits: &Mutations, event: &str) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|e| match e {
            Mutation::NewEventListener { name, id } if *name == event => Some(*id),
            _ => None,
        })
        .unwrap()
}
fn input(dom: &VirtualDom, id: ElementId, value: &str) {
    let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.runtime().handle_event(
        "input",
        dioxus::core::Event::new(payload.into_any(), true),
        id,
    );
}
fn click(dom: &VirtualDom, id: ElementId) {
    let payload = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.runtime().handle_event(
        "click",
        dioxus::core::Event::new(payload.into_any(), true),
        id,
    );
}
#[test]
fn actual_input_events_keep_consecutive_values_and_double_add_cannot_replay_consumed_text() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let slot = Slot::default();
    let copy = slot.clone();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let s = ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-text-field-create",
                );
                let p = rename::presentation(&s).unwrap();
                assert!(rename::text_create::kind_shell(
                    &s,
                    p.serial,
                    crate::slack_lists::generation(&s).unwrap(),
                    rename::selection(&s).unwrap(),
                    p.field_choice.unwrap(),
                    "select"
                ));
                s
            });
            slot.set(Some(state));
            let shell = state.read();
            let p = rename::presentation(&shell).unwrap();
            let visit = crate::slack_lists::generation(&shell).unwrap();
            let selection = rename::selection(&shell).unwrap();
            drop(shell);
            fields(
                state,
                p.serial,
                visit,
                selection,
                p.field_choice.unwrap(),
                p.select_draft.unwrap(),
                true,
            )
        },
        copy,
    );
    let initial = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let field = node(&initial, "input");
    let add = node(&initial, "click");
    input(&dom, field, "R");
    input(&dom, field, "Ready");
    assert_eq!(
        rename::presentation(&state.peek())
            .unwrap()
            .select_draft
            .unwrap()
            .pending,
        "Ready"
    );
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(edits.edits.iter().any(|e|matches!(e,Mutation::SetAttribute{id,name:"value",value:AttributeValue::Text(value),..} if *id==field&&value=="Ready")));
    click(&dom, add);
    click(&dom, add);
    input(&dom, field, "Late from consumed input");
    let draft = rename::presentation(&state.peek())
        .unwrap()
        .select_draft
        .unwrap();
    assert_eq!(draft.rows.len(), 1);
    assert_eq!(draft.rows[0].option.label(), "Ready");
    assert!(draft.pending.is_empty());
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(
        !edits
            .edits
            .iter()
            .any(|e| matches!(e,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==field))
    );
    assert!(edits.edits.iter().any(|e|matches!(e,Mutation::SetAttribute{id,name:"value",value:AttributeValue::Text(value),..} if *id==field&&value.is_empty())));
    input(&dom, field, "Next");
    assert_eq!(
        rename::presentation(&state.peek())
            .unwrap()
            .select_draft
            .unwrap()
            .pending,
        "Next"
    );
    dom.in_runtime(|| {
        assert!(rename::text_create::select::close_fixture(&state.peek()));
        assert!(rename::presentation(&state.peek()).is_none());
    });
}
