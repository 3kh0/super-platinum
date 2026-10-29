use super::*;
use crate::media::MediaRegistry;
use dioxus::core::{AttributeValue, Mutation};
use serde_json::json;
use std::{cell::Cell, rc::Rc};

#[test]
fn generated_fresh_url_prefill_is_visible_and_submittable_without_input_callback() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    for alias in ["app-modal-url-prefill", "app-modal-url-prefill-narrow"] {
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
        let mut state = slot.get().unwrap();
        let key = InputKey {
            block_id: "project_url".into(),
            action_id: "value".into(),
        };
        let mut id = edits
            .edits
            .iter()
            .find_map(|m| match m {
                Mutation::SetAttribute {
                    id,
                    name: "id",
                    value: AttributeValue::Text(v),
                    ..
                } if v == &crate::view::app_modal::field_id(&key) => Some(*id),
                _ => None,
            })
            .expect("actual prefilled URL input");
        assert!(edits.edits.iter().any(|m|matches!(m,
            Mutation::SetAttribute{id:other,name:"initial_value",value:AttributeValue::Text(v),..} if *other==id&&v==fixture::PREFILL)));
        assert!(
            !state
                .read()
                .app_modal
                .as_ref()
                .unwrap()
                .current_form()
                .unwrap()
                .fields[&key]
                .dirty
        );
        let payload = requests::prepare_submission(&state.read()).unwrap();
        assert_eq!(
            payload["values"]["project_url"]["value"],
            json!({"type":"url_text_input","value":fixture::PREFILL})
        );
        // Two real view updates before rendering can leave the old uncontrolled
        // widget alive. A recreated optional blank field must not submit an
        // omitted value behind its still-visible old prefill.
        let update = |shell: &mut ShellState, view: Value| {
            let raw = serde_json::from_value(json!({"type":"view_updated","view_id":view["id"],
                "app_id":"A_FIXTURE","view_type":"modal","view":view}))
            .unwrap();
            let scope = scope(shell).unwrap();
            realtime(
                shell,
                &scope.team,
                scope.realtime_generation,
                &RtEvent::Unknown(raw),
            );
        };
        let mut removed = fixture::view();
        removed["blocks"].as_array_mut().unwrap().remove(0);
        update(&mut state.write(), removed);
        let mut recreated = fixture::view();
        recreated["blocks"][0]["optional"] = json!(true);
        update(&mut state.write(), recreated.clone());
        assert!(matches!(
            requests::prepare_submission(&state.read()),
            Err(FormError::Unsupported(_))
        ));
        recreated["blocks"][0]["label"]["text"] = json!("Recreated optional link");
        update(&mut state.write(), recreated);
        assert!(matches!(
            requests::prepare_submission(&state.read()),
            Err(FormError::Unsupported(_))
        ));
        // A newly owned modal parent restores the eligible prefill boundary.
        fixture::install_prefill(&mut state.write());
        let rebuilt = dom.render_immediate_to_vec();
        id = rebuilt
            .edits
            .iter()
            .find_map(|m| match m {
                Mutation::SetAttribute {
                    id,
                    name: "id",
                    value: AttributeValue::Text(v),
                    ..
                } if v == &crate::view::app_modal::field_id(&key) => Some(*id),
                _ => None,
            })
            .expect("new-parent prefilled input");
        assert_eq!(
            requests::prepare_submission(&state.read()).unwrap()["values"]["project_url"]["value"]
                ["value"],
            json!(fixture::PREFILL)
        );
        let literal = "typed.example/project?raw=one%20two";
        let event = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
            literal.into(),
            Vec::new(),
        ));
        dom.runtime().handle_event(
            "input",
            dioxus::core::Event::new(event.into_any(), true),
            id,
        );
        assert_eq!(
            requests::prepare_submission(&state.read()).unwrap()["values"]["project_url"]["value"]
                ["value"],
            json!(literal)
        );
        dom.render_immediate_to_vec();
    }
}
