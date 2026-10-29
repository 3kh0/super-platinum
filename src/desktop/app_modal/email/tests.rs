use super::*;
use crate::media::MediaRegistry;
use serde_json::json;
use std::rc::Rc;

fn key() -> InputKey {
    InputKey {
        block_id: "work_email".into(),
        action_id: "value".into(),
    }
}
fn shell() -> ShellState {
    let mut s = ShellState::fixture(MediaRegistry::default());
    fixture::install(&mut s, false);
    s
}
fn value(s: &ShellState) -> InputValue {
    s.app_modal.as_ref().unwrap().current_form().unwrap().fields[&key()]
        .value
        .clone()
}
fn update(s: &mut ShellState, v: Value) {
    let raw = serde_json::from_value(json!({"type":"view_updated","view_id":v["id"],
        "app_id":"A_FIXTURE","view_type":"modal","view":v}))
    .unwrap();
    let scope = scope(s).unwrap();
    realtime(
        s,
        &scope.team,
        scope.realtime_generation,
        &RtEvent::Unknown(raw),
    );
}

#[test]
fn owned_email_edits_survive_updates_and_unsupported_initial_cannot_be_bypassed() {
    let mut s = shell();
    let binding = Rc::new(());
    let owner = Owner::capture(&s, &key(), &binding).unwrap();
    owner.apply(&mut s, "local@example.com".into());
    let mut v = fixture::view();
    v["blocks"][0]["label"]["text"] = json!("Updated contact");
    update(&mut s, v.clone());
    assert!(owner.current(&s));
    assert_eq!(
        value(&s),
        InputValue::Email(Some("local@example.com".into()))
    );
    v["blocks"][0]["element"]["initial_value"] = json!("remote@example.com");
    update(&mut s, v.clone());
    assert!(!owner.current(&s));
    owner.apply(&mut s, "bypass@example.com".into());
    let f = s.app_modal.as_ref().unwrap().current_form().unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    assert_eq!(
        value(&s),
        InputValue::Email(Some("local@example.com".into()))
    );
    v["blocks"][0]["element"]["initial_value"] = json!("");
    update(&mut s, v);
    assert!(owner.current(&s));
    owner.apply(&mut s, "".into());
    assert_eq!(value(&s), InputValue::Email(Some("".into())));
    owner.apply(&mut s, "work@example.com".into());
    assert_eq!(
        requests::prepare_submission(&s).unwrap()["values"]["work_email"]["value"],
        json!({"type":"email_text_input","value":"work@example.com"})
    );
    // Same Slack IDs in a fresh presentation cannot revive the original listener.
    fixture::install(&mut s, false);
    owner.apply(&mut s, "stale@example.com".into());
    assert_eq!(value(&s), InputValue::Email(None));
    assert!(Owner::capture(&s, &key(), &binding).is_some());
    // Capturing a disabled binding is safe: admission is rechecked on every
    // edit, and repairing a supported same-ID payload can enable the widget.
    let mut v = fixture::view();
    v["blocks"][0]["element"]["initial_value"] = json!("prefilled@example.com");
    s.app_modal = Some(Session::fixture(v));
    let disabled = Owner::capture(&s, &key(), &binding).unwrap();
    assert!(!disabled.current(&s));
    disabled.apply(&mut s, "bypass@example.com".into());
    assert_eq!(value(&s), InputValue::Email(None));
    update(&mut s, fixture::view());
    assert!(disabled.current(&s));
    disabled.apply(&mut s, "enabled@example.com".into());
    assert_eq!(
        value(&s),
        InputValue::Email(Some("enabled@example.com".into()))
    );
}

#[test]
fn hidden_busy_replaced_field_and_account_changes_refuse_captured_email_edits() {
    for mode in 0..6 {
        let mut s = shell();
        let binding = Rc::new(());
        let owner = Owner::capture(&s, &key(), &binding).unwrap();
        match mode {
            0 => s.overlay = None,
            1 => s.app_modal.as_mut().unwrap().busy = true,
            2 => s.core.account_epoch += 1,
            3 => s.core.active_team = Some("T_OTHER".into()),
            4 => {
                let mut v = fixture::view();
                v["blocks"][0]["element"]["type"] = json!("plain_text_input");
                update(&mut s, v);
            }
            _ => s.app_modal.as_mut().unwrap().reset_revision = next_id(),
        }
        let before = value(&s);
        owner.apply(&mut s, "wrong@example.com".into());
        assert_eq!(value(&s), before, "mode{mode}");
    }
}

#[test]
fn email_native_owner_rejects_same_account_credential_replacement() {
    use super_platinum_core::config::{Session as Credentials, WorkspaceSession};
    let mut s = shell();
    s.core.transport = Some(crate::bootstrap::test_support::transport("synthetic-email"));
    s.core.session = Some(Credentials {
        d_cookie: "synthetic-email".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: s.core.workspaces["T1"].self_user_id.clone(),
                name: "Fixture".into(),
                url: "https://fixture.invalid".into(),
                token: "synthetic-email".into(),
                enterprise_id: None,
            },
        )]
        .into(),
    });
    let binding = Rc::new(());
    let owner = Owner::capture(&s, &key(), &binding).unwrap();
    assert!(owner.current(&s));
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "replaced-email".into();
    owner.apply(&mut s, "stale@example.com".into());
    assert_eq!(value(&s), InputValue::Email(None));
    // A separately captured live binding can edit; dropping it retires the
    // callback even with an otherwise identical current presentation/field.
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "synthetic-email".into();
    assert!(owner.current(&s));
    drop(binding);
    owner.apply(&mut s, "retired@example.com".into());
    assert_eq!(value(&s), InputValue::Email(None));
}

#[test]
fn generated_normal_and_narrow_email_controls_accept_serialized_local_edits() {
    use dioxus::core::{AttributeValue, Mutation};
    use std::{cell::Cell, rc::Rc};
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    for (alias, initially_unsupported) in [
        ("app-modal-email", false),
        ("app-modal-email-narrow", false),
        ("app-modal-email-error", false),
        ("app-modal-email-error-narrow", false),
        ("app-modal-email", true),
    ] {
        let slot = Rc::new(Cell::new(None));
        let copy = slot.clone();
        let mut dom = VirtualDom::new_with_props(
            move |(alias, initially_unsupported, slot): (
                &'static str,
                bool,
                Rc<Cell<Option<Signal<ShellState>>>>,
            )| {
                let state = use_signal(|| {
                    let mut shell = ShellState::fixture_variant(MediaRegistry::default(), alias);
                    if initially_unsupported {
                        let mut view = fixture::view();
                        view["blocks"][0]["element"]["initial_value"] =
                            json!("unsupported@example.com");
                        shell.app_modal = Some(Session::fixture(view));
                    }
                    shell
                });
                slot.set(Some(state));
                crate::view::app_modal::app_modal(state)
            },
            (alias, initially_unsupported, copy),
        );
        let edits = dom.rebuild_to_vec();
        let mut state = slot.get().unwrap();
        let id = edits
            .edits
            .iter()
            .find_map(|m| match m {
                Mutation::SetAttribute {
                    id,
                    name: "id",
                    value: AttributeValue::Text(value),
                    ..
                } if value == &crate::view::app_modal::field_id(&key()) => Some(*id),
                _ => None,
            })
            .expect("actual generated email input");
        if initially_unsupported {
            assert!(edits.edits.iter().any(|m| matches!(m,
                Mutation::SetAttribute{id:other,name:"disabled",value:AttributeValue::Bool(true),..} if *other==id)));
            let event = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
                "bypass@example.com".into(),
                Vec::new(),
            ));
            dom.runtime().handle_event(
                "input",
                dioxus::core::Event::new(event.into_any(), true),
                id,
            );
            assert_eq!(value(&state.read()), InputValue::Email(None));
            update(&mut state.write(), fixture::view());
            let repaired = dom.render_immediate_to_vec();
            assert!(repaired.edits.iter().any(|m| matches!(m,
                Mutation::SetAttribute{id:other,name:"disabled",value:AttributeValue::Bool(false),..} if *other==id)));
        }
        let event = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
            "Valid+tag@Example.com".into(),
            Vec::new(),
        ));
        dom.runtime().handle_event(
            "input",
            dioxus::core::Event::new(event.into_any(), true),
            id,
        );
        assert_eq!(
            value(&state.read()),
            InputValue::Email(Some("Valid+tag@Example.com".into()))
        );
        assert!(requests::prepare_submission(&state.read()).is_ok());
        dom.render_immediate_to_vec();
        let event = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
            "".into(),
            Vec::new(),
        ));
        dom.runtime().handle_event(
            "input",
            dioxus::core::Event::new(event.into_any(), true),
            id,
        );
        assert_eq!(value(&state.read()), InputValue::Email(Some("".into())));
        assert!(matches!(
            requests::prepare_submission(&state.read()),
            Err(FormError::Validation(_))
        ));
    }
}
