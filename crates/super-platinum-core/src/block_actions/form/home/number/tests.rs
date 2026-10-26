use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "number".into(),
        action_id: "value".into(),
    }
}
fn raw() -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[{
        "type":"input","block_id":"number","optional":true,"dispatch_action":true,
        "element":{"type":"number_input","action_id":"value","is_decimal_allowed":true,"initial_value":"1.20",
        "dispatch_action_config":{"trigger_actions_on":["on_character_entered","on_enter_pressed"]}}
    }]})
}
fn row(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["number"]["value"].clone()
}
fn input(form: &mut HomeForm, value: Option<&str>, trigger: NumberTrigger) -> Option<TextEvent> {
    form.number_input(&key(), value.map(str::to_owned), trigger)
        .unwrap()
}
fn reconcile(form: &mut HomeForm, view: &Value, parents: &[NumberParent]) -> Result<(), FormError> {
    form.reconcile_with_number_parents(view, &[], &[], &[], &[], &[], &[], &[], parents)
}
#[test]
fn pristine_omission_callback_null_and_local_empty_seed_are_distinct() {
    for initial in [None, Some("")] {
        let mut view = raw();
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
        if let Some(v) = initial {
            view["blocks"][0]["element"]["initial_value"] = json!(v);
        }
        let mut form = HomeForm::parse(&view).unwrap();
        assert_eq!(row(&form), json!({"type":"number_input"}));
        assert_eq!(form.number_seed(&key()).unwrap().as_deref(), initial);
        assert!(input(&mut form, None, NumberTrigger::Change).is_none());
        assert_eq!(row(&form), json!({"type":"number_input","value":null}));
        assert!(input(&mut form, None, NumberTrigger::Enter).is_some());
    }
}
#[test]
fn callback_value_is_verbatim_and_metadata_is_trigger_specific() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    let change = input(&mut form, Some("2.5"), NumberTrigger::Change).unwrap();
    assert_eq!(
        change.action(),
        &json!({"type":"number_input","block_id":"number","action_id":"value","value":"2.5","initial_value":"1.20"})
    );
    let enter = input(&mut form, Some("002.50"), NumberTrigger::Enter).unwrap();
    assert_eq!(enter.action()["value"], "002.50");
    assert!(enter.action().get("initial_value").is_none());
    let blur = input(&mut form, Some("2.5"), NumberTrigger::Blur).unwrap();
    assert_eq!(blur.action(), change.action());
    assert_eq!(row(&form)["value"], "2.5");
    // Equal admitted callbacks remain distinct occurrences; local Change suppression is renderer-owned.
    assert!(input(&mut form, Some("2.5"), NumberTrigger::Enter).is_some());
}
#[test]
fn config_empty_is_not_text_enter_fallback_but_blur_can_dispatch() {
    for (config, change, enter) in [
        (None, false, true),
        (Some(json!([])), false, false),
        (Some(json!(["on_character_entered"])), true, false),
        (Some(json!(["on_enter_pressed"])), false, true),
    ] {
        let mut view = raw();
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("dispatch_action_config");
        if let Some(config) = config {
            view["blocks"][0]["element"]["dispatch_action_config"] =
                json!({"trigger_actions_on":config});
        }
        let mut form = HomeForm::parse(&view).unwrap();
        assert_eq!(
            input(&mut form, Some("2"), NumberTrigger::Change).is_some(),
            change
        );
        assert_eq!(
            input(&mut form, Some("2.00"), NumberTrigger::Enter).is_some(),
            enter
        );
        assert!(input(&mut form, Some("2"), NumberTrigger::Blur).is_some());
        assert!(input(&mut form, None, NumberTrigger::Change).is_none());
    }
}
#[test]
fn quiet_callbacks_commit_and_required_empty_suppresses_after_store() {
    let mut view = raw();
    view["blocks"][0]["dispatch_action"] = json!(false);
    view["blocks"][0]["optional"] = json!(false);
    let mut form = HomeForm::parse(&view).unwrap();
    for trigger in [
        NumberTrigger::Change,
        NumberTrigger::Enter,
        NumberTrigger::Blur,
    ] {
        assert!(input(&mut form, Some("3"), trigger).is_none());
        assert_eq!(row(&form)["value"], "3");
    }
    assert!(input(&mut form, None, NumberTrigger::Enter).is_none());
    assert_eq!(row(&form)["value"], Value::Null);
    assert!(form.fields[&key()].error.is_some());
}
#[test]
fn bounds_store_invalid_state_and_zero_bounds_are_falsey() {
    let mut view = raw();
    view["blocks"][0]["element"]["min_value"] = json!("2");
    view["blocks"][0]["element"]["max_value"] = json!("5");
    let mut form = HomeForm::parse(&view).unwrap();
    let queued = input(&mut form, Some("3"), NumberTrigger::Change).unwrap();
    for value in ["1", "6"] {
        assert!(input(&mut form, Some(value), NumberTrigger::Change).is_none());
        assert_eq!(row(&form)["value"], value);
        assert!(form.fields[&key()].error.is_some());
    }
    assert_eq!(queued.action()["value"], "3");
    view["blocks"][0]["element"]["min_value"] = json!("-0");
    view["blocks"][0]["element"]["max_value"] = json!("0.0");
    let mut form = HomeForm::parse(&view).unwrap();
    for value in ["-2", "2"] {
        assert!(input(&mut form, Some(value), NumberTrigger::Change).is_some());
    }
}
#[test]
fn integer_local_rule_is_not_a_parent_fraction_validator() {
    let mut view = raw();
    view["blocks"][0]["element"]["is_decimal_allowed"] = json!(false);
    let mut form = HomeForm::parse(&view).unwrap();
    for value in ["1.2", "0.01", "1e-2", "-0", "1e+21"] {
        assert!(input(&mut form, Some(value), NumberTrigger::Enter).is_some());
        assert_eq!(row(&form)["value"], value);
    }
}
#[test]
fn fresh_seed_falls_back_without_rewriting_saved_parent() {
    for saved in [
        json!({"type":"number_input"}),
        json!({"type":"number_input","value":null}),
        json!({"type":"number_input","value":""}),
        json!({"type":"number_input","value":"0"}),
    ] {
        let mut view = raw();
        view["state"] = json!({"values":{"number":{"value":saved}}});
        let form = HomeForm::parse(&view).unwrap();
        assert_eq!(row(&form), saved);
        assert_eq!(
            form.number_seed(&key()).unwrap().as_deref(),
            Some(if saved["value"] == "0" { "0" } else { "1.20" })
        );
    }
}
#[test]
fn initialized_pristine_and_dirty_parent_survive_changed_initial_and_bounds() {
    for dirty in [false, true] {
        let view = raw();
        let mut form = HomeForm::parse(&view).unwrap();
        if dirty {
            input(&mut form, Some("2.50"), NumberTrigger::Enter);
        }
        let parent = form.number_parent(&key()).unwrap();
        let before = row(&form);
        let mut next = view.clone();
        next["blocks"][0]["element"]["initial_value"] = json!("9");
        next["blocks"][0]["element"]["max_value"] = json!("1");
        reconcile(&mut form, &next, &[parent]).unwrap();
        assert_eq!(row(&form), before);
        assert!(form.fields[&key()].error.is_some());
        assert_eq!(
            form.number_seed(&key()).unwrap(),
            Some(if dirty { "2.50" } else { "1.20" }.into())
        );
        let event = input(&mut form, Some("1"), NumberTrigger::Change).unwrap();
        assert_eq!(event.action()["initial_value"], "9");
    }
}
#[test]
fn tokens_are_owner_kind_and_inventory_bound() {
    let view = raw();
    let form = HomeForm::parse(&view).unwrap();
    let token = form.number_parent(&key()).unwrap();
    for changed in ["id", "app_id", "bot_id", "app_installed_team_id"] {
        let mut next = view.clone();
        next[changed] = json!("FOREIGN");
        let mut candidate = form.clone();
        assert!(reconcile(&mut candidate, &next, std::slice::from_ref(&token)).is_err());
        assert_eq!(candidate, form);
    }
    let mut candidate = form.clone();
    assert!(reconcile(&mut candidate, &view, &[token.clone(), token]).is_err());
    let mut next = view.clone();
    next["blocks"][0]["element"] = json!({"type":"plain_text_input","action_id":"value"});
    assert!(
        reconcile(
            &mut candidate,
            &next,
            &[form.number_parent(&key()).unwrap()]
        )
        .is_err()
    );
    let mut corrupt = form.clone();
    corrupt.fields.get_mut(&key()).unwrap().value = InputValue::Number(Some("8".into()));
    assert!(corrupt.state().is_err());
    let mut corrupt = form.clone();
    corrupt.fields.clear();
    assert!(corrupt.state().is_err());
    let mut corrupt = form.clone();
    corrupt.fields.get_mut(&key()).unwrap().raw["is_decimal_allowed"] = json!(false);
    assert!(corrupt.state().is_err());
}
#[test]
fn native_safety_ack_does_not_commit_buffer_or_erase_domain_error() {
    let mut view = raw();
    view["blocks"][0]["optional"] = json!(false);
    let mut form = HomeForm::parse(&view).unwrap();
    input(&mut form, None, NumberTrigger::Change);
    let before = form.wire[&key()].clone();
    form.set_native_error(&key(), "bad".into(), "Native invalid".into())
        .unwrap();
    assert!(form.state().is_err());
    assert!(
        form.number_acknowledge_native_buffer(&key(), Some("NaN"))
            .is_err()
    );
    form.number_acknowledge_native_buffer(&key(), Some("3"))
        .unwrap();
    assert_eq!(row(&form), before);
    assert!(form.fields[&key()].error.is_some());
}
#[test]
fn malformed_callback_and_raw_sources_do_not_widen_numeric_admission() {
    let form = HomeForm::parse(&raw()).unwrap();
    for value in [
        "NaN",
        "Infinity",
        "-Infinity",
        ".",
        "-",
        "0x10",
        " 2 ",
        "1e999",
        "garbage",
    ] {
        let mut candidate = form.clone();
        assert!(
            candidate
                .number_input(&key(), Some(value.into()), NumberTrigger::Change)
                .is_err()
        );
        assert_eq!(candidate, form);
    }
    for (property, value) in [
        ("initial_value", json!(null)),
        ("initial_value", json!(2)),
        ("initial_value", json!("NaN")),
        ("min_value", json!("")),
        ("max_value", json!("Infinity")),
        ("is_decimal_allowed", json!(null)),
    ] {
        let mut view = raw();
        view["blocks"][0]["element"][property] = value;
        assert!(HomeForm::parse(&view).is_err());
    }
    let mut view = raw();
    view["state"] = json!({"values":{"number":{"value":{"type":"number_input","value":2}}}});
    assert!(HomeForm::parse(&view).is_err());
}
#[test]
fn modal_and_noninput_admission_remain_closed() {
    let mut view = raw();
    view["blocks"][0]["type"] = json!("section");
    view["blocks"][0]["accessory"] = view["blocks"][0]["element"].take();
    assert!(HomeForm::parse(&view).unwrap().state().is_err());
    let mut modal = raw();
    modal["type"] = json!("modal");
    modal["title"] = json!({"type":"plain_text","text":"Modal"});
    modal["blocks"][0]["label"] = json!({"type":"plain_text","text":"Number"});
    let parsed = ModalForm::parse(&modal).unwrap();
    assert!(!parsed.unsupported.is_empty());
    assert!(matches!(
        parsed.fields[&key()].kind,
        InputKind::Unsupported { .. }
    ));
}
