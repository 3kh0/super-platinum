use super::*;
use crate::{app_modal::scalar::Owner, media::MediaRegistry};
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations};
use serde_json::json;
use std::{cell::Cell, rc::Rc};

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
fn field(edits: &Mutations, block: &str) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|m| match m {
            Mutation::SetAttribute {
                id,
                name: "id",
                value: AttributeValue::Text(value),
                ..
            } if value == &crate::view::app_modal::field_id(&key(block)) => Some(*id),
            _ => None,
        })
        .expect("actual generated scalar input")
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
fn generated_url_controls_keep_schemeless_literal_and_only_url_detaches_from_form() {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    for alias in [
        "app-modal-url",
        "app-modal-url-narrow",
        "app-modal-url-error",
        "app-modal-url-error-narrow",
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
        let url = field(&edits, "project_url");
        let backup = field(&edits, "backup_url");
        let email = field(&edits, "contact");
        let name = field(&edits, "name");
        for id in [url, backup] {
            assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:"form",value:AttributeValue::Text(v),..} if *other==id&&v.is_empty())));
        }
        for id in [email, name] {
            assert!(!edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{id:other,name:"form",value:AttributeValue::Text(_),..} if *other==id)));
        }
        assert!(!edits.edits.iter().any(|m| matches!(
            m,
            Mutation::SetAttribute {
                name: "novalidate",
                value: AttributeValue::Bool(true),
                ..
            }
        )));
        let literal = "example.com/project?x=1&y=two";
        input(&dom, url, literal);
        let payload = requests::prepare_submission(&state.read()).unwrap();
        assert_eq!(
            payload["values"]["project_url"]["value"],
            json!({"type":"url_text_input","value":literal})
        );
        assert_eq!(
            payload["values"]["backup_url"]["value"],
            json!({"type":"url_text_input"})
        );
        input(&dom, url, "not a url");
        assert!(matches!(
            requests::prepare_submission(&state.read()),
            Err(FormError::Validation(_))
        ));
        assert_eq!(
            state
                .read()
                .app_modal
                .as_ref()
                .unwrap()
                .current_form()
                .unwrap()
                .fields[&key("project_url")]
                .value,
            InputValue::Url(Some("not a url".into()))
        );
        input(&dom, url, "");
        assert!(matches!(
            requests::prepare_submission(&state.read()),
            Err(FormError::Validation(_))
        ));
        input(&dom, url, literal);
        input(&dom, backup, "");
        input(&dom, email, "person@example.com");
        let payload = requests::prepare_submission(&state.read()).unwrap();
        assert_eq!(
            payload["values"]["backup_url"]["value"],
            json!({"type":"url_text_input","value":""})
        );
        assert_eq!(
            payload["values"]["contact"]["value"],
            json!({"type":"email_text_input","value":"person@example.com"})
        );
        assert_eq!(
            payload["values"]["name"]["value"]["value"],
            json!("Project")
        );
        dom.render_immediate_to_vec();
    }
}

#[test]
fn url_realtime_updates_retain_owned_draft_and_email_binding_cannot_claim_url() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    fixture::install(&mut shell, false);
    let binding = Rc::new(());
    let url = key("project_url");
    let contact = key("contact");
    assert!(Owner::capture(&shell, &url, &binding).is_none());
    assert!(Owner::capture_url(&shell, &contact, &binding).is_none());
    let owner = Owner::capture_url(&shell, &url, &binding).unwrap();
    owner.apply(&mut shell, "local.example/project".into());
    let mut view = fixture::view();
    view["blocks"][0]["label"]["text"] = json!("Updated link");
    view["state"] = json!({"values":{"project_url":{"value":{"type":"url_text_input","value":"remote.example"}}}});
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
    update(&mut shell, view.clone());
    assert!(owner.current(&shell));
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["project_url"]["value"]["value"],
        json!("local.example/project")
    );
    view["blocks"][0]["element"]["initial_value"] = json!("prefilled.example");
    update(&mut shell, view.clone());
    assert!(!owner.current(&shell));
    owner.apply(&mut shell, "bypass.example".into());
    assert!(matches!(
        requests::prepare_submission(&shell),
        Err(FormError::Unsupported(_))
    ));
    view["blocks"][0]["element"]["initial_value"] = json!("");
    update(&mut shell, view);
    assert!(owner.current(&shell));
    assert_eq!(
        requests::prepare_submission(&shell).unwrap()["values"]["project_url"]["value"]["value"],
        json!("local.example/project")
    );
    // Close/reopen same IDs and a retired mounted binding remain covered by
    // the unchanged Email tests against this shared Owner implementation.
}
