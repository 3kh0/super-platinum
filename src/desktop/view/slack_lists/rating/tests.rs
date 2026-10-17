use super::*;
use dioxus::core::{AttributeValue, Mutation, ScopeId};
use std::{cell::Cell, rc::Rc};
#[test]
fn actual_rating_buttons_toggle_same_value_to_empty_and_disabled_callbacks_do_nothing() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    type Slot = Rc<Cell<Option<Signal<Option<u8>>>>>;
    for max in 1..=5 {
        for disabled in [false, true] {
            let slot = Slot::default();
            let mut dom = VirtualDom::new_with_props(
                |(slot, max, disabled): (Slot, u8, bool)| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "slack-list",
                        )
                    });
                    let selected = use_signal(|| None::<u8>);
                    slot.set(Some(selected));
                    fields(
                        state,
                        &RatingOptions::capture(":heart:", max, None).unwrap(),
                        selected,
                        disabled,
                    )
                },
                (slot.clone(), max, disabled),
            );
            let initial = dom.rebuild_to_vec();
            let buttons = initial
                .edits
                .iter()
                .filter_map(|e| match e {
                    Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(buttons.len(), usize::from(max) + 1);
            // Listener emission order is not DOM order: the outer Clear handler may
            // precede the dynamically created numeric buttons. Bind by emitted value.
            let mut choices = std::collections::BTreeMap::new();
            for edit in &initial.edits {
                if let Mutation::SetAttribute {
                    name: "data-rating-choice",
                    value: AttributeValue::Int(value),
                    id,
                    ..
                } = edit
                {
                    assert!(buttons.contains(id));
                    assert!(choices.insert(*value, *id).is_none());
                }
            }
            assert_eq!(
                choices.keys().copied().collect::<Vec<_>>(),
                (1..=i64::from(max)).collect::<Vec<_>>()
            );
            // fields() has exactly one click handler without a numeric choice: Clear.
            let clear = buttons
                .iter()
                .copied()
                .filter(|id| !choices.values().any(|choice| choice == id))
                .collect::<Vec<_>>();
            assert_eq!(clear.len(), 1);
            let click = |dom: &VirtualDom, id| {
                let data =
                    dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
                dom.runtime().handle_event(
                    "click",
                    dioxus::core::Event::new(data.into_any(), true),
                    id,
                );
            };
            let selected = slot.get().unwrap();
            click(&dom, choices[&i64::from(max)]);
            dom.in_runtime(|| assert_eq!(*selected.peek(), (!disabled).then_some(max)));
            // Same rendered closure reads the current local signal, never an old value.
            click(&dom, choices[&i64::from(max)]);
            dom.in_runtime(|| assert_eq!(*selected.peek(), None));
            click(&dom, choices[&1]);
            dom.in_runtime(|| assert_eq!(*selected.peek(), (!disabled).then_some(1)));
            dom.mark_dirty(ScopeId::ROOT);
            dom.render_immediate_to_vec();
            click(&dom, clear[0]);
            dom.in_runtime(|| assert_eq!(*selected.peek(), None));
        }
    }
}
#[test]
fn rating_keys_stay_inside_exact_quantity_and_do_not_claim_browser_tab_traversal() {
    for max in 1..=5 {
        assert_eq!(key_value(None, max, Key::Home), Some(None));
        assert_eq!(key_value(None, max, Key::End), Some(Some(max)));
        assert_eq!(key_value(Some(max), max, Key::ArrowRight), Some(Some(max)));
        assert_eq!(key_value(Some(1), max, Key::ArrowLeft), Some(None));
        assert_eq!(key_value(Some(max), max, Key::Delete), Some(None));
        assert_eq!(key_value(None, max, Key::Character("0".into())), Some(None));
        assert_eq!(key_value(None, max, Key::Character("6".into())), None);
        assert_eq!(key_value(None, max, Key::Tab), None);
        assert_eq!(key_value(None, max, Key::Escape), None);
    }
}
