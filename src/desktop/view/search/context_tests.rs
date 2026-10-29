//! Real generated controls and callbacks; not browser layout or trusted input proof.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations, ScopeId};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn mount(kind: SearchKind, alias: &'static str) -> (VirtualDom, Signal<ShellState>, Mutations) {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(slot, kind, alias): (Slot, SearchKind, &'static str)| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
                crate::bootstrap::search_fixture(&mut shell, alias);
                shell.set_search_kind(kind);
                if kind != SearchKind::People {
                    shell.search_filters = Some(SearchFilters::from_query(&shell.search_query));
                }
                shell
            });
            slot.set(Some(state));
            controls(state, &state.read())
        },
        (slot.clone(), kind, alias),
    );
    let edits = dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), edits)
}
fn field(edits: &Mutations, label: &str) -> Option<ElementId> {
    edits.edits.iter().find_map(|m| match m {
        Mutation::SetAttribute {
            id,
            name: "aria-label",
            value: AttributeValue::Text(value),
            ..
        } if value == label => Some(*id),
        _ => None,
    })
}
fn change(dom: &VirtualDom, event: &str, id: ElementId, value: &str) {
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
fn generated_context_controls_are_message_only_and_person_suggestions_do_not_follow_from_field() {
    for alias in ["search-context-filters", "search-context-filters-narrow"] {
        for kind in SearchKind::ALL {
            let (_dom, state, edits) = mount(kind, alias);
            assert_eq!(
                field(&edits, "With person").is_some(),
                kind == SearchKind::Messages
            );
            let threads = edits
                .edits
                .iter()
                .any(|m| matches!(m,Mutation::CreateTextNode{value,..} if value=="In threads"));
            assert_eq!(threads, kind == SearchKind::Messages);
            if kind == SearchKind::Messages {
                let id = field(&edits, "With person").unwrap();
                assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,value:AttributeValue::Text(value),..} if *other==id&&value=="@Maya Chen")));
                assert_eq!(
                    with_options(&state.peek(), "@Maya"),
                    vec![("@Maya Chen".into(), "Maya Chen".into())]
                );
                assert!(!sender_options(&state.peek(), "@Jules").is_empty());
                assert!(with_options(&state.peek(), "not a cached person").is_empty());
            }
        }
    }
}
#[test]
fn actual_person_input_and_thread_checkbox_do_not_change_file_or_link_flags_and_clear_remounts_values()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let (mut dom, mut state, edits) = mount(SearchKind::Messages, "search-context-filters");
    {
        let shell = state.peek();
        let filters = shell.search_filters.as_ref().unwrap();
        assert_eq!(filters.with_person, "@Maya Chen");
        assert!(filters.in_thread);
    }
    let id = field(&edits, "With person").unwrap();
    change(&dom, "input", id, "@Alex Morgan");
    change(
        &dom,
        "change",
        field(&edits, "In threads").unwrap(),
        "false",
    );
    {
        let shell = state.peek();
        let filters = shell.search_filters.as_ref().unwrap();
        assert_eq!(filters.with_person, "@Alex Morgan");
        assert!(!filters.in_thread);
        assert!(!filters.has_file);
        assert!(!filters.has_link);
    }
    change(&dom, "change", field(&edits, "Has files").unwrap(), "true");
    change(&dom, "change", field(&edits, "Has links").unwrap(), "true");
    {
        let shell = state.peek();
        let filters = shell.search_filters.as_ref().unwrap();
        assert!(filters.has_file && filters.has_link);
        assert!(!filters.in_thread);
    }
    dom.in_runtime(|| {
        state.write().search_filters = Some(SearchFilters::default());
        assert!(state.write().apply_search_filters());
    });
    dom.render_immediate_to_vec();
    dom.in_runtime(|| {
        let query = state.peek().search_query.clone();
        state.write().search_filters = Some(SearchFilters::from_query(&query));
    });
    dom.mark_dirty(ScopeId::ROOT);
    let reopened = dom.render_immediate_to_vec();
    let new_id = field(&reopened, "With person").unwrap();
    assert!(reopened.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id,value:AttributeValue::Text(value),..} if *id==new_id&&value.is_empty())));
    assert!(
        reopened
            .edits
            .iter()
            .filter(|m| matches!(
                m,
                Mutation::SetAttribute {
                    name: "checked",
                    value: AttributeValue::Bool(false),
                    ..
                }
            ))
            .count()
            >= 3
    );
}

#[test]
fn saved_and_pinned_controls_are_message_only_and_callbacks_preserve_the_other_flags() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    for kind in SearchKind::ALL {
        let (_dom, _state, edits) = mount(kind, "search-context-filters");
        assert_eq!(
            field(&edits, "Saved items").is_some(),
            kind == SearchKind::Messages
        );
        assert_eq!(
            field(&edits, "Pinned messages").is_some(),
            kind == SearchKind::Messages
        );
    }
    for alias in ["search-saved-pinned", "search-saved-pinned-narrow"] {
        let (_dom, state, edits) = mount(SearchKind::Messages, alias);
        assert_eq!(state.peek().search_query, "release is:saved has:pin");
        let shell = state.peek();
        let f = shell.search_filters.as_ref().unwrap();
        assert!(f.saved && f.pinned);
        for label in ["Saved items", "Pinned messages"] {
            let id = field(&edits, label).unwrap();
            assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:"checked",value:AttributeValue::Bool(true),..} if *other==id)));
        }
    }
    let (dom, state, edits) = mount(SearchKind::Messages, "search-context-filters");
    let saved = field(&edits, "Saved items").unwrap();
    let pinned = field(&edits, "Pinned messages").unwrap();
    assert!(!state.peek().search_filters.as_ref().unwrap().saved);
    assert!(!state.peek().search_filters.as_ref().unwrap().pinned);
    change(&dom, "change", saved, "true");
    change(&dom, "change", pinned, "true");
    {
        let shell = state.peek();
        let f = shell.search_filters.as_ref().unwrap();
        assert!(f.saved && f.pinned && f.in_thread);
        assert!(!f.has_file && !f.has_link);
    }
    change(&dom, "change", saved, "false");
    {
        let shell = state.peek();
        let f = shell.search_filters.as_ref().unwrap();
        assert!(!f.saved && f.pinned && f.in_thread);
        assert!(!f.has_file && !f.has_link);
    }
    change(&dom, "change", pinned, "false");
    let shell = state.peek();
    let f = shell.search_filters.as_ref().unwrap();
    assert!(!f.saved && !f.pinned && f.in_thread);
    assert!(!f.has_file && !f.has_link);
}
