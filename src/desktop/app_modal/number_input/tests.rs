use super::*;
use crate::{app_modal::scalar::Owner, media::MediaRegistry};
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
    let e = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        literal.into(),
        Vec::new(),
    ));
    dom.runtime()
        .handle_event("input", dioxus::core::Event::new(e.into_any(), true), id);
}

#[test]
fn generated_required_integer_controls_seed_zero_edit_literal_and_block_clear_or_other_formats() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    for alias in [
        "app-modal-required-integer",
        "app-modal-required-integer-narrow",
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
            .expect("actual generated number input");
        for (name, value) in [("type", "number"), ("initial_value", "1")] {
            assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:n,value:AttributeValue::Text(v),..} if *other==id&&*n==name&&v==value)));
        }
        assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:"required",value:AttributeValue::Bool(true),..} if *other==id)));
        assert!(!edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:"form"|"step"|"min"|"max"|"pattern",value:AttributeValue::Text(_),..} if *other==id)));
        assert!(!edits.edits.iter().any(|m| matches!(
            m,
            Mutation::SetAttribute {
                name: "novalidate",
                value: AttributeValue::Bool(true),
                ..
            }
        )));
        let p = requests::prepare_submission(&state.read()).unwrap();
        assert_eq!(
            p["values"]["quantity"]["value"],
            json!({"type":"number_input","value":"1"})
        );
        assert_eq!(
            p["values"]["reserve"]["value"],
            json!({"type":"number_input","value":"0"})
        );
        assert_eq!(p["values"]["note"]["value"]["value"], json!("Order"));
        input(&dom, id, "42");
        assert_eq!(
            requests::prepare_submission(&state.read()).unwrap()["values"]["quantity"]["value"]["value"],
            json!("42")
        );
        input(&dom, id, "1e2");
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
            InputValue::Number(Some("1e2".into()))
        );
        assert!(matches!(
            requests::prepare_submission(&state.read()),
            Err(FormError::Validation(_))
        ));
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
        let binding = Rc::new(());
        assert!(Owner::capture(&state.read(), &key(), &binding).is_none());
        assert!(Owner::capture_url(&state.read(), &key(), &binding).is_none());
        assert!(Owner::capture_integer(&state.read(), &key(), &binding).is_some());
        dom.render_immediate_to_vec();
    }
}

#[test]
fn realtime_integer_parent_retains_pristine_then_refuses_batched_recreation_and_old_owner() {
    let mut shell =
        ShellState::fixture_variant(MediaRegistry::default(), "app-modal-required-integer");
    let binding = Rc::new(());
    let owner = Owner::capture_integer(&shell, &key(), &binding).unwrap();
    let update = |shell: &mut ShellState, view: Value| {
        let raw=serde_json::from_value(json!({"type":"view_updated","view_id":view["id"],"app_id":"A_FIXTURE","view_type":"modal","view":view})).unwrap();
        let scope = scope(shell).unwrap();
        realtime(
            shell,
            &scope.team,
            scope.realtime_generation,
            &RtEvent::Unknown(raw),
        );
    };
    let mut v = fixture::view();
    v["blocks"][0]["element"]["initial_value"] = json!("9");
    update(&mut shell, v);
    assert!(owner.current(&shell));
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["quantity"]["value"]["value"],
        json!("1")
    );
    assert!(
        !shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .fields[&key()]
            .dirty
    );
    let mut removed = fixture::view();
    removed["blocks"].as_array_mut().unwrap().remove(0);
    update(&mut shell, removed);
    let mut recreated = fixture::view();
    recreated["blocks"][0]["element"]["initial_value"] = json!("7");
    update(&mut shell, recreated.clone());
    assert!(!owner.current(&shell));
    assert!(matches!(
        requests::prepare_submission(&shell),
        Err(FormError::Unsupported(_))
    ));
    recreated["blocks"][0]["label"]["text"] = json!("Recreated quantity");
    update(&mut shell, recreated);
    assert!(matches!(
        requests::prepare_submission(&shell),
        Err(FormError::Unsupported(_))
    ));
    fixture::install(&mut shell);
    assert!(!owner.current(&shell));
    owner.apply(&mut shell, "123".into());
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["quantity"]["value"]["value"],
        json!("1")
    );
}
