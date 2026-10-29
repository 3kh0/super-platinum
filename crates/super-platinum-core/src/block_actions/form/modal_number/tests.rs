use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "quantity".into(),
        action_id: "value".into(),
    }
}
fn view(initial: &str) -> Value {
    json!({"id":"V_QUANTITY","type":"modal","title":{"type":"plain_text","text":"Order quantity"},
        "submit":{"type":"plain_text","text":"Save quantity"},"blocks":[{
        "type":"input","block_id":"quantity","label":{"type":"plain_text","text":"Quantity"},
        "element":{"type":"number_input","action_id":"value","is_decimal_allowed":false,"initial_value":initial}}]})
}
fn parse(view: &Value) -> ModalForm {
    ModalForm::parse_with_required_integer_context(view, None, None).unwrap()
}
fn row(form: &ModalForm) -> Value {
    form.submission().unwrap()["values"]["quantity"]["value"].clone()
}

#[test]
fn required_integer_is_explicit_fresh_and_initial_literal_needs_no_callback() {
    for value in ["0", "1", "999999999"] {
        let v = view(value);
        for legacy in [
            ModalForm::parse(&v),
            ModalForm::parse_with_context(&v, None, None),
            ModalForm::parse_with_email_context(&v, None, None),
            ModalForm::parse_with_scalar_context(&v, None, None),
            ModalForm::parse_with_url_prefill_context(&v, None, None),
        ] {
            assert!(matches!(
                legacy.unwrap().submission(),
                Err(FormError::Unsupported(_))
            ));
        }
        let f = parse(&v);
        assert_eq!(row(&f), json!({"type":"number_input","value":value}));
        assert!(!f.fields[&key()].dirty);
        assert_eq!(f.integer_keys.len(), 1);
    }
    for value in [Value::Null, json!(1), json!(false)] {
        let mut v = view("1");
        v["blocks"][0]["element"]["initial_value"] = value;
        assert!(ModalForm::parse_with_required_integer_context(&v, None, None).is_err());
    }
}

#[test]
fn required_integer_rejects_optional_decimal_bounds_saved_and_interaction_profiles() {
    let mut denied = vec![];
    let mut v = view("1");
    v["blocks"][0]["optional"] = json!(true);
    denied.push(v);
    for (name, value) in [
        ("is_decimal_allowed", json!(true)),
        ("is_decimal_allowed", Value::Null),
        ("min_value", json!("0")),
        ("max_value", json!("9")),
        ("min_value", Value::Null),
        ("dispatch_action_config", Value::Null),
        ("confirm", Value::Null),
        ("visible_to_user_ids", Value::Null),
    ] {
        let mut v = view("1");
        v["blocks"][0]["element"][name] = value;
        denied.push(v);
    }
    let mut v = view("1");
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("is_decimal_allowed");
    denied.push(v);
    let mut v = view("1");
    v["blocks"][0]["dispatch_action"] = json!(true);
    denied.push(v);
    let mut v = view("1");
    v["type"] = json!("app_notification_subscription_configuration");
    denied.push(v);
    for value in ["", "01", "-1", "1.0", "1000000000"] {
        denied.push(view(value));
    }
    let mut v = view("1");
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_value");
    denied.push(v);
    for v in denied {
        assert!(
            matches!(parse(&v).submission(), Err(FormError::Unsupported(_))),
            "{v}"
        );
    }
    for saved in [
        json!({"type":"number_input"}),
        json!({"type":"number_input","value":"1"}),
        json!({"type":"number_input","value":null}),
        Value::Null,
    ] {
        let mut v = view("1");
        v["state"] = json!({"values":{"quantity":{"value":saved}}});
        match ModalForm::parse_with_required_integer_context(&v, None, None) {
            Ok(f) => assert!(matches!(f.submission(), Err(FormError::Unsupported(_)))),
            Err(_) => {}
        }
    }
    let mut v = view("0");
    v["state"] = json!({"values":{"quantity":{}}});
    assert_eq!(row(&parse(&v))["value"], json!("0"));
}

#[test]
fn required_integer_boundary_corpus_stores_invalid_edits_and_required_clear() {
    let cases: Value = serde_json::from_str(include_str!("validation_cases.json")).unwrap();
    let mut f = parse(&view("1"));
    for case in cases.as_array().unwrap() {
        let literal = case["value"].as_str().unwrap();
        let admitted = case["admitted"].as_bool().unwrap();
        assert_eq!(canonical(literal), admitted, "{literal:?}");
        f.set_value(&key(), InputValue::Number(Some(literal.into())))
            .unwrap();
        assert_eq!(
            f.fields[&key()].value,
            InputValue::Number(Some(literal.into()))
        );
        if admitted {
            assert_eq!(row(&f)["value"], json!(literal));
        } else {
            assert!(
                matches!(f.submission(), Err(FormError::Validation(_))),
                "{literal:?}"
            );
        }
    }
    f.set_value(&key(), InputValue::Number(None)).unwrap();
    assert_eq!(f.fields[&key()].value, InputValue::Number(None));
    assert_eq!(
        f.fields[&key()].error.as_deref(),
        Some("Please complete this required field.")
    );
    assert!(matches!(f.submission(), Err(FormError::Validation(_))));
    f.set_value(&key(), InputValue::Number(Some("0".into())))
        .unwrap();
    assert_eq!(row(&f)["value"], json!("0"));
}

#[test]
fn required_integer_retains_owned_pristine_dirty_and_server_error_rows_transactionally() {
    let mut f = parse(&view("1"));
    f.reconcile(&view("9")).unwrap();
    assert_eq!(row(&f)["value"], json!("1"));
    assert!(!f.fields[&key()].dirty);
    assert_eq!(f.fields[&key()].revision, 0);
    f.apply_server_errors(&[("quantity".into(), "Choose another quantity.".into())].into());
    let mut update = view("009");
    update["state"] = json!({"values":{"quantity":{"value":{"type":"number_input","value":"7"}}}});
    f.reconcile(&update).unwrap();
    assert_eq!(f.fields[&key()].value, InputValue::Number(Some("1".into())));
    assert_eq!(
        f.fields[&key()].error.as_deref(),
        Some("Choose another quantity.")
    );
    f.set_value(&key(), InputValue::Number(Some("24".into())))
        .unwrap();
    f.reconcile(&update).unwrap();
    assert_eq!(row(&f)["value"], json!("24"));
    assert!(f.fields[&key()].dirty);
    let before = f.clone();
    update["state"]["values"]["quantity"]["value"]["value"] = json!(7);
    assert!(f.reconcile(&update).is_err());
    assert_eq!(f, before);
    update = view("2");
    update["id"] = json!("V_FOREIGN");
    assert!(f.reconcile(&update).is_err());
    assert_eq!(f, before);
    f.set_value(&key(), InputValue::Number(None)).unwrap();
    f.reconcile(&view("3")).unwrap();
    assert_eq!(f.fields[&key()].value, InputValue::Number(None));
    assert!(matches!(f.submission(), Err(FormError::Validation(_))));
}

#[test]
fn required_integer_cannot_be_introduced_or_recreated_inside_retained_parent() {
    let mut empty = view("1");
    empty["blocks"] = json!([]);
    let mut f = parse(&empty);
    f.reconcile(&view("1")).unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    assert!(f.integer_keys.is_empty());
    let mut f = parse(&view("1"));
    f.reconcile(&empty).unwrap();
    assert!(f.integer_keys.is_empty());
    for initial in ["1", "9", ""] {
        f.reconcile(&view(initial)).unwrap();
        assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
        assert!(f.integer_keys.is_empty());
    }
    assert_eq!(row(&parse(&view("9")))["value"], json!("9"));
    let mut f = parse(&view("1"));
    let mut optional = view("2");
    optional["blocks"][0]["optional"] = json!(true);
    f.reconcile(&optional).unwrap();
    assert!(f.integer_keys.is_empty());
    f.reconcile(&view("3")).unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    let mut dispatch = view("1");
    dispatch["blocks"][0]["dispatch_action"] = json!(true);
    let mut f = parse(&dispatch);
    assert!(f.integer_keys.is_empty());
    f.reconcile(&view("1")).unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    let mut f = parse(&view(""));
    f.reconcile(&view("1")).unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
}
