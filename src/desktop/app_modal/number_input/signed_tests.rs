use super::*;
use crate::media::MediaRegistry;
use dioxus::core::{AttributeValue, ElementId, Mutation};
use serde_json::json;
use std::{cell::Cell, rc::Rc};

fn key() -> InputKey {
    InputKey {
        block_id: "quantity".into(),
        action_id: "value".into(),
    }
}
fn input(dom: &VirtualDom, id: ElementId, literal: &str) {
    let event = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        literal.into(),
        Vec::new(),
    ));
    dom.runtime().handle_event(
        "input",
        dioxus::core::Event::new(event.into_any(), true),
        id,
    );
}

#[test]
fn generated_signed_adjustment_is_ready_without_input_and_invalid_edits_never_send_old_value() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    for alias in [
        "app-modal-signed-integer",
        "app-modal-signed-integer-narrow",
    ] {
        type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
        let slot = Slot::default();
        let copy = slot.clone();
        let mut dom = VirtualDom::new_with_props(
            |(alias, slot): (&'static str, Slot)| {
                let state =
                    use_signal(|| ShellState::fixture_variant(MediaRegistry::default(), alias));
                slot.set(Some(state));
                crate::view::app_modal::app_modal(state)
            },
            (alias, copy),
        );
        let edits = dom.rebuild_to_vec();
        let state = slot.get().unwrap();
        let id = edits
            .edits
            .iter()
            .find_map(|m| match m {
                Mutation::SetAttribute {
                    id,
                    name: "id",
                    value: AttributeValue::Text(v),
                    ..
                } if v == &crate::view::app_modal::field_id(&key()) => Some(*id),
                _ => None,
            })
            .expect("actual signed quantity input");
        assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:"initial_value",value:AttributeValue::Text(v),..} if *other==id&&v=="-1")));
        assert_eq!(
            requests::prepare_submission(&state.read()).unwrap()["values"]["quantity"]["value"],
            json!({"type":"number_input","value":"-1"})
        );
        input(&dom, id, "-24");
        assert_eq!(
            requests::prepare_submission(&state.read()).unwrap()["values"]["quantity"]["value"]["value"],
            json!("-24")
        );
        for invalid in ["-0", "--1", " -1", "-1.5", "-1000000000"] {
            input(&dom, id, invalid);
            assert_eq!(
                state
                    .read()
                    .app_modal
                    .as_ref()
                    .unwrap()
                    .current_form()
                    .unwrap()
                    .fields[&key()]
                    .value,
                InputValue::Number(Some(invalid.into()))
            );
            assert!(matches!(
                requests::prepare_submission(&state.read()),
                Err(FormError::Validation(_))
            ));
        }
        input(&dom, id, "");
        assert_eq!(
            state
                .read()
                .app_modal
                .as_ref()
                .unwrap()
                .current_form()
                .unwrap()
                .fields[&key()]
                .value,
            InputValue::Number(None)
        );
        assert!(matches!(
            requests::prepare_submission(&state.read()),
            Err(FormError::Validation(_))
        ));
        input(&dom, id, "0");
        assert_eq!(
            requests::prepare_submission(&state.read()).unwrap()["values"]["quantity"]["value"]["value"],
            json!("0")
        );
        dom.render_immediate_to_vec();
    }
}
