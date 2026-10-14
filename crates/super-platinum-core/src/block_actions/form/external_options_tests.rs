use super::*;
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Option {value}"),"emoji":true},"value":value,"description":{"type":"plain_text","text":"Exact description","emoji":false}})
}
fn typed(values: &[&str]) -> Vec<ExternalOption> {
    values
        .iter()
        .map(|v| ExternalOption::parse(&option(v)).unwrap())
        .collect()
}
fn key() -> InputKey {
    InputKey {
        block_id: "b".into(),
        action_id: "a".into(),
    }
}
fn view() -> Value {
    json!({"id":"V1","type":"modal","title":{"type":"plain_text","text":"Choices"},"blocks":[{"type":"input","block_id":"b","optional":true,"label":{"type":"plain_text","text":"Choices"},"element":{"type":"multi_external_select","action_id":"a","initial_options":[option("a"),option("b")]}},{"type":"input","block_id":"s","label":{"type":"plain_text","text":"Sibling"},"element":{"type":"plain_text_input","action_id":"t","initial_value":"unchanged"}}]})
}
#[test]
fn defaults_raw_order_clear_and_current_limits() {
    let mut raw = json!({"type":"multi_external_select","initial_options":[option("b"),option("a")],"max_selected_items":1});
    let mut field = parse_message_external_options(&raw).unwrap();
    assert_eq!(
        field.kind,
        InputKind::MultiExternalSelect {
            min_query_length: 3,
            max_selected_items: Some(1)
        }
    );
    assert_eq!(field.value, InputValue::ExternalOptions(typed(&["b", "a"])));
    assert!(validate_message_external_options(&field, &typed(&["b", "a"])).is_err());
    assert_eq!(
        validate_message_external_options(&field, &typed(&["b"])).unwrap()["selected_options"],
        json!([option("b")])
    );
    assert_eq!(
        validate_message_external_options(&field, &[]).unwrap()["selected_options"],
        json!([])
    );
    raw["required"] = json!(true);
    field = parse_message_external_options(&raw).unwrap();
    assert!(validate_message_external_options(&field, &[]).is_err());
}
#[test]
fn malformed_arrays_settings_and_unbounded_selections_fail_closed() {
    for initial in [
        json!({}),
        json!([null]),
        json!([{"value":"bad"}]),
        json!([option("a"), option("a")]),
        json!(vec![option("a"); 1001]),
    ] {
        assert!(
            parse_message_external_options(
                &json!({"type":"multi_external_select","initial_options":initial})
            )
            .is_err()
        );
    }
    for (name, value) in [
        ("max_selected_items", json!(0)),
        ("max_selected_items", json!(null)),
        ("min_query_length", json!(-1)),
        ("initial_option", json!(null)),
        ("response_url_enabled", json!(true)),
        ("filter", json!({})),
        ("options", json!([])),
    ] {
        let mut raw = json!({"type":"multi_external_select"});
        raw[name] = value;
        assert!(parse_message_external_options(&raw).is_err(), "{name}");
    }
    let field = parse_message_external_options(&json!({"type":"multi_external_select"})).unwrap();
    assert!(validate_message_external_options(&field, &typed(&["a", "a"])).is_err());
}
#[test]
fn saved_null_falls_back_empty_clears_and_full_state_keeps_raw() {
    for saved in [Value::Null, json!([]), json!([option("saved")])] {
        let mut raw = view();
        raw["state"] =
            json!({"values":{"b":{"a":{"type":"multi_external_select","selected_options":saved}}}});
        let expected = if saved.is_null() {
            json!([option("a"), option("b")])
        } else {
            saved
        };
        let form = ModalForm::parse(&raw).unwrap();
        let wire = form.submission().unwrap();
        assert_eq!(wire["values"]["b"]["a"]["selected_options"], expected);
        assert_eq!(wire["values"]["s"]["t"]["value"], "unchanged");
    }
    let mut raw = view();
    raw["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    raw["state"] =
        json!({"values":{"b":{"a":{"type":"multi_external_select","selected_options":null}}}});
    assert_eq!(
        ModalForm::parse(&raw).unwrap().fields[&key()].value,
        InputValue::ExternalOptions(vec![])
    );
}
#[test]
fn inline_pristine_updates_use_truthy_initial_while_dirty_clear_survives() {
    let mut raw = view();
    let mut pristine = ModalForm::parse(&raw).unwrap();
    raw["state"] = json!({"values":{"b":{"a":{"type":"multi_external_select","selected_options":[option("saved")]}}}});
    pristine.reconcile(&raw).unwrap();
    assert_eq!(
        pristine.fields[&key()].value,
        InputValue::ExternalOptions(typed(&["a", "b"]))
    );
    assert_eq!(
        ModalForm::parse(&raw).unwrap().fields[&key()].value,
        InputValue::ExternalOptions(typed(&["saved"]))
    );
    raw["blocks"][0]["element"]["initial_options"] = json!([option("changed")]);
    pristine.reconcile(&raw).unwrap();
    assert_eq!(
        pristine.fields[&key()].value,
        InputValue::ExternalOptions(typed(&["changed"]))
    );
    raw["blocks"][0]["element"]["initial_options"] = Value::Null;
    pristine.reconcile(&raw).unwrap();
    assert_eq!(
        pristine.fields[&key()].value,
        InputValue::ExternalOptions(typed(&["changed"]))
    );
    raw["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    pristine.reconcile(&raw).unwrap();
    assert_eq!(
        pristine.fields[&key()].value,
        InputValue::ExternalOptions(typed(&["changed"]))
    );
    raw["blocks"][0]["element"]["initial_options"] = json!([]);
    pristine.reconcile(&raw).unwrap();
    assert_eq!(
        pristine.fields[&key()].value,
        InputValue::ExternalOptions(vec![])
    );
    pristine
        .set_value(&key(), InputValue::ExternalOptions(vec![]))
        .unwrap();
    pristine.reconcile(&view()).unwrap();
    assert_eq!(
        pristine.submission().unwrap()["values"]["b"]["a"]["selected_options"],
        json!([])
    );
}
#[test]
fn modal_max_reduction_retains_correctable_values_and_block_requiredness() {
    let mut raw = view();
    raw["blocks"][0]["element"]["required"] = json!(true);
    let mut form = ModalForm::parse(&raw).unwrap();
    form.set_value(&key(), InputValue::ExternalOptions(vec![]))
        .unwrap();
    assert!(form.submission().is_ok()); // block optional overrides element required
    let mut form = ModalForm::parse(&view()).unwrap();
    raw["blocks"][0]["element"]["max_selected_items"] = json!(1);
    form.reconcile(&raw).unwrap();
    assert!(form.submission().is_err());
    assert_eq!(
        form.fields[&key()].value,
        InputValue::ExternalOptions(typed(&["a", "b"]))
    );
    form.set_value(&key(), InputValue::ExternalOptions(typed(&["b"])))
        .unwrap();
    assert!(form.submission().is_ok());
    raw["blocks"][0]["optional"] = json!(false);
    form.reconcile(&raw).unwrap();
    form.set_value(&key(), InputValue::ExternalOptions(vec![]))
        .unwrap();
    assert!(form.submission().is_err());
}
#[test]
fn malformed_saved_and_unsupported_contexts_do_not_submit() {
    let mut raw = view();
    raw["state"] = json!({"values":{"b":{"a":{"type":"multi_external_select","selected_options":[{"value":"bad"}]}}}});
    assert!(ModalForm::parse(&raw).is_err());
    for setting in ["confirm", "visible_to_user_ids", "dispatch_action_config"] {
        let mut raw = view();
        raw["blocks"][0]["element"][setting] = json!({});
        assert!(ModalForm::parse(&raw).unwrap().submission().is_err());
    }
    let mut raw = view();
    raw["type"] = json!("app_notification_subscription_configuration");
    assert!(ModalForm::parse(&raw).unwrap().submission().is_err());
    let mut form = ModalForm::parse(&view()).unwrap();
    form.set_value(&key(), InputValue::ExternalOptions(vec![]))
        .unwrap();
    let mut raw = view();
    raw["blocks"][0]["element"] = json!({"type":"external_select","action_id":"a"});
    assert_eq!(form.reconcile(&raw), Err(FormError::ConflictingUpdate));
}
