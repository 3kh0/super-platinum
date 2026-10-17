use super::*;
use dioxus::core::{ElementId, Mutation, Mutations, ScopeId};
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
fn form(dom: &VirtualDom, id: ElementId, event: &str, value: &str) {
    let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.runtime().handle_event(
        event,
        dioxus::core::Event::new(payload.into_any(), true),
        id,
    );
}
#[test]
fn actual_settings_events_retire_old_configuration_handlers_and_preserve_query_node() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-people-field-create",
                )
            });
            slot.set(Some(state));
            let p = draft::presentation(&state.read()).unwrap();
            rsx! {Settings{state,p,enabled:true}}
        },
        slot.clone(),
    );
    let initial = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let query = node(&initial, "input");
    let multiple = node(&initial, "change");
    form(&dom, query, "input", "Alice");
    form(&dom, multiple, "change", "true");
    form(&dom, query, "input", "stale after format");
    form(&dom, multiple, "change", "false");
    let p = draft::presentation(&state.peek()).unwrap();
    assert_eq!(p.directory.query, "Alice");
    assert_eq!(p.options.format(), EntityCreateFormat::Multiple);
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(!edits.edits.iter().any(|e|matches!(e,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==query||*id==multiple)));
    form(&dom, query, "input", "Bob");
    assert_eq!(
        draft::presentation(&state.peek()).unwrap().directory.query,
        "Bob"
    );
    dom.in_runtime(|| {
        assert!(crate::slack_lists::rename::fixture::close_people_fixture(
            &state.peek()
        ))
    });
}
#[test]
fn removing_first_default_retains_actual_successor_button_and_query_nodes() {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-channel-settings",
                )
            });
            slot.set(Some(state));
            let p = draft::presentation(&state.read()).unwrap();
            rsx! {Settings{state,p,enabled:true}}
        },
        slot.clone(),
    );
    let initial = dom.rebuild_to_vec();
    let mut state = slot.get().unwrap();
    let query = node(&initial, "input");
    let buttons = initial
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Mutation::NewEventListener { name, id } if *name == "click" => Some(*id),
            _ => None,
        })
        .collect::<Vec<_>>();
    let p = draft::presentation(&state.peek()).unwrap();
    assert_eq!(p.selected.len(), 2);
    assert!(buttons.len() >= 2);
    assert!(settings::edit_shell(
        &state.peek(),
        p.stamp,
        Edit::Remove(p.selected[0].id.clone())
    ));
    state.write();
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(!edits.edits.iter().any(|e|matches!(e,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==query||*id==buttons[1])));
    assert!(
        !edits.edits.iter().any(
            |e| matches!(e, Mutation::NewEventListener{name,..} if *name=="click"||*name=="input")
        ),
        "surviving controls must not be remounted"
    );
    let after = draft::presentation(&state.peek()).unwrap();
    assert_eq!(after.selected, vec![p.selected[1].clone()]);
    dom.in_runtime(|| assert!(settings::close_test(&state.peek())));
}
