use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "adjustment".into(),
        action_id: "value".into(),
    }
}
fn view(initial: &str) -> Value {
    json!({"id":"V_ADJUSTMENT","type":"modal","title":{"type":"plain_text","text":"Inventory adjustment"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":[{"type":"input","block_id":"adjustment",
        "label":{"type":"plain_text","text":"Adjustment"},"element":{"type":"number_input","action_id":"value",
        "is_decimal_allowed":false,"initial_value":initial}}]})
}
fn parse(v: &Value) -> ModalForm {
    ModalForm::parse_with_signed_integer_context(v, None, None).unwrap()
}
fn row(f: &ModalForm) -> Value {
    f.submission().unwrap()["values"]["adjustment"]["value"].clone()
}

#[test]
fn signed_integer_domain_is_explicit_literal_and_keeps_all_initialization_guards() {
    let corpus: Value = serde_json::from_str(include_str!("signed_validation_cases.json")).unwrap();
    for case in corpus.as_array().unwrap() {
        let literal = case["value"].as_str().unwrap();
        let admitted = case["admitted"].as_bool().unwrap();
        assert_eq!(canonical_signed(literal), admitted, "{literal:?}");
        let f = parse(&view(literal));
        if admitted {
            assert_eq!(row(&f), json!({"type":"number_input","value":literal}));
            assert!(!f.fields[&key()].dirty);
            if literal.starts_with('-') {
                assert!(!canonical(literal));
                let old =
                    ModalForm::parse_with_required_integer_context(&view(literal), None, None)
                        .unwrap();
                assert!(matches!(old.submission(), Err(FormError::Unsupported(_))));
            }
        } else {
            assert!(
                matches!(f.submission(), Err(FormError::Unsupported(_))),
                "{literal:?}"
            );
        }
    }
    for (name, value) in [
        ("is_decimal_allowed", json!(true)),
        ("min_value", json!("-9")),
        ("max_value", Value::Null),
        ("confirm", Value::Null),
        ("dispatch_action_config", Value::Null),
    ] {
        let mut v = view("-1");
        v["blocks"][0]["element"][name] = value;
        assert!(matches!(
            parse(&v).submission(),
            Err(FormError::Unsupported(_))
        ));
    }
    let mut optional = view("-1");
    optional["blocks"][0]["optional"] = json!(true);
    assert!(matches!(
        parse(&optional).submission(),
        Err(FormError::Unsupported(_))
    ));
    let mut saved = view("-1");
    saved["state"] =
        json!({"values":{"adjustment":{"value":{"type":"number_input","value":"-1"}}}});
    assert!(matches!(
        parse(&saved).submission(),
        Err(FormError::Unsupported(_))
    ));
    let mut dispatch = view("-1");
    dispatch["blocks"][0]["dispatch_action"] = json!(true);
    assert!(matches!(
        parse(&dispatch).submission(),
        Err(FormError::Unsupported(_))
    ));
}

#[test]
fn signed_integer_context_covers_edits_retention_server_errors_and_accepted_steps() {
    let mut f = parse(&view("-1"));
    f.reconcile(&view("7")).unwrap();
    assert_eq!(row(&f)["value"], json!("-1"));
    assert!(!f.fields[&key()].dirty);
    f.set_value(&key(), InputValue::Number(Some("-24".into())))
        .unwrap();
    f.reconcile(&view("8")).unwrap();
    assert_eq!(row(&f)["value"], json!("-24"));
    f.apply_server_errors(&[("adjustment".into(), "Review this adjustment.".into())].into());
    f.reconcile(&view("9")).unwrap();
    assert_eq!(
        f.fields[&key()].value,
        InputValue::Number(Some("-24".into()))
    );
    assert_eq!(
        f.fields[&key()].error.as_deref(),
        Some("Review this adjustment.")
    );
    assert!(matches!(f.submission(), Err(FormError::Validation(_))));
    f.apply_server_errors(&BTreeMap::new());
    assert_eq!(row(&f)["value"], json!("-24"));
    for invalid in ["-0", "--1", "-01", "-1.5", "-1e2", "-1000000000", " -1"] {
        f.set_value(&key(), InputValue::Number(Some(invalid.into())))
            .unwrap();
        assert_eq!(
            f.fields[&key()].value,
            InputValue::Number(Some(invalid.into()))
        );
        assert!(matches!(f.submission(), Err(FormError::Validation(_))));
    }
    f.set_value(&key(), InputValue::Number(Some("-999999999".into())))
        .unwrap();
    f.reconcile_submitted(&view("1")).unwrap();
    assert_eq!(row(&f)["value"], json!("-999999999"));
    assert!(f.fields[&key()].dirty);
    assert_eq!(f.fields[&key()].revision, 0);
    let before = f.clone();
    let mut malformed = view("2");
    malformed["state"] =
        json!({"values":{"adjustment":{"value":{"type":"number_input","value":7}}}});
    assert!(f.reconcile(&malformed).is_err());
    assert_eq!(f, before);
    f.set_value(&key(), InputValue::Number(None)).unwrap();
    assert_eq!(
        f.fields[&key()].error.as_deref(),
        Some("Please complete this required field.")
    );
    assert!(matches!(f.submission(), Err(FormError::Validation(_))));
    f.set_value(&key(), InputValue::Number(Some("0".into())))
        .unwrap();
    assert_eq!(row(&f)["value"], json!("0"));
}
