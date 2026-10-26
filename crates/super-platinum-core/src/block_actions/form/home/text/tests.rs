use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "text".into(),
        action_id: "value".into(),
    }
}
fn raw() -> Value {
    json!({"id":"V1","type":"home","blocks":[{"type":"input","block_id":"text","dispatch_action":true,"label":{"type":"plain_text","text":"Text"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"initial","opaque":{"kept":true}}}]})
}
fn apply(form: &mut HomeForm, text: &str, trigger: TextTrigger) -> Option<TextEvent> {
    form.text_input(&key(), text.into(), trigger).unwrap()
}
fn value(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["text"]["value"]["value"].clone()
}

#[test]
fn literal_enter_and_trimmed_change_are_independent_of_config_enter_hint() {
    for triggers in [
        None,
        Some(json!([])),
        Some(json!(["on_enter_pressed"])),
        Some(json!(["on_character_entered"])),
        Some(json!(["on_enter_pressed", "on_character_entered"])),
    ] {
        let mut view = raw();
        if let Some(triggers) = triggers {
            view["blocks"][0]["element"]["dispatch_action_config"] =
                json!({"trigger_actions_on":triggers});
        }
        let mut form = HomeForm::parse(&view).unwrap();
        assert!(form.unsupported.is_empty());
        let change = apply(&mut form, "  value  ", TextTrigger::Change);
        assert_eq!(
            change.is_some(),
            matches!(
                form.dispatch(&key()),
                Some(Dispatch::Character | Dispatch::EnterOrCharacter)
            )
        );
        assert_eq!(value(&form), "value");
        let event = apply(&mut form, "  value  ", TextTrigger::Enter).unwrap();
        assert_eq!(
            event.action(),
            &json!({"type":"plain_text_input","action_id":"value","block_id":"text","value":"  value  ","initial_value":"initial"})
        );
        assert_eq!(value(&form), "  value  ");
        assert_eq!(
            form.fields[&key()].value,
            InputValue::Text("  value  ".into())
        );
        assert_eq!(form.raw_view(), &view);
    }
}
#[test]
fn trim_equal_change_preserves_previous_literal_committed_wire() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    apply(&mut form, " a ", TextTrigger::Enter).unwrap();
    assert!(apply(&mut form, "  a  ", TextTrigger::Change).is_none());
    assert_eq!(value(&form), " a ");
    assert_eq!(form.fields[&key()].value, InputValue::Text("  a  ".into()));
    let mut view = raw();
    view["hash"] = json!("new");
    form.reconcile(&view).unwrap();
    assert_eq!(value(&form), " a ");
}
#[test]
fn quiet_enter_is_noop_and_existing_quiet_trim_semantics_remain() {
    let mut view = raw();
    view["blocks"][0]["dispatch_action"] = json!(false);
    let mut form = HomeForm::parse(&view).unwrap();
    apply(&mut form, " value ", TextTrigger::Change);
    let before = form.clone();
    assert!(apply(&mut form, " value ", TextTrigger::Enter).is_none());
    assert_eq!(form, before);
    assert_eq!(value(&form), "value");
}
#[test]
fn invalid_callback_updates_state_before_validation_and_keeps_prior_event() {
    let mut view = raw();
    view["blocks"][0]["element"]["max_length"] = json!(2);
    let mut form = HomeForm::parse(&view).unwrap();
    let queued = apply(&mut form, "a", TextTrigger::Enter).unwrap();
    assert!(apply(&mut form, " abc ", TextTrigger::Enter).is_none());
    assert_eq!(value(&form), " abc ");
    assert!(form.fields[&key()].error.is_some());
    assert_eq!(queued.action()["value"], "a");
    assert!(apply(&mut form, " abc ", TextTrigger::Change).is_none());
    assert_eq!(value(&form), " abc ");
}
#[test]
fn callback_validation_uses_emitted_utf16_empty_and_whitespace() {
    for (literal, optional, min, max, valid) in [
        ("", true, Some(3), None, true),
        ("", false, None, None, false),
        ("   ", false, None, None, true),
        (" a ", false, Some(3), None, true),
        ("😀", false, None, Some(1), false),
    ] {
        let mut view = raw();
        view["blocks"][0]["optional"] = json!(optional);
        if let Some(min) = min {
            view["blocks"][0]["element"]["min_length"] = json!(min);
        }
        if let Some(max) = max {
            view["blocks"][0]["element"]["max_length"] = json!(max);
        }
        let mut form = HomeForm::parse(&view).unwrap();
        assert_eq!(
            apply(&mut form, literal, TextTrigger::Enter).is_some(),
            valid
        );
        assert_eq!(form.fields[&key()].error.is_none(), valid);
        assert_eq!(
            value(&form),
            if literal.is_empty() {
                Value::Null
            } else {
                json!(literal)
            }
        );
    }
    let mut view = raw();
    view["blocks"][0]["element"]["min_length"] = json!(3);
    let mut form = HomeForm::parse(&view).unwrap();
    apply(&mut form, " a ", TextTrigger::Change);
    assert!(form.fields[&key()].error.is_some());
    assert_eq!(value(&form), "a");
}
#[test]
fn no_accessor_nontext_foreign_key_or_mutated_shape_mints_text_event() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    let mut foreign = key();
    foreign.action_id = "other".into();
    assert!(
        form.text_input(&foreign, "x".into(), TextTrigger::Enter)
            .is_err()
    );
    form.fields.get_mut(&key()).unwrap().kind = InputKind::Text {
        multiline: true,
        min_length: None,
        max_length: None,
    };
    assert!(
        form.text_input(&key(), "x".into(), TextTrigger::Enter)
            .is_err()
    );
    for change in [
        json!({"confirm":{}}),
        json!({"visible_to_user_ids":["U1"]}),
        json!({"dispatch_action_config":{"trigger_actions_on":["unknown"]}}),
    ] {
        let mut view = raw();
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        match HomeForm::parse(&view) {
            Err(_) => {}
            Ok(mut form) => assert!(
                form.text_input(&key(), "x".into(), TextTrigger::Enter)
                    .is_err()
            ),
        }
    }
}
