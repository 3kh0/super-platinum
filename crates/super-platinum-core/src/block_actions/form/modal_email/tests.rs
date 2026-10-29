use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "email".into(),
        action_id: "contact".into(),
    }
}
fn view() -> Value {
    json!({"type":"modal","id":"V_EMAIL","title":{"type":"plain_text","text":"Contact"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":[{
        "type":"input","block_id":"email","optional":true,
        "label":{"type":"plain_text","text":"Email address"},
        "element":{"type":"email_text_input","action_id":"contact"}}]})
}
fn parse(v: &Value) -> ModalForm {
    ModalForm::parse_with_email_context(v, None, None).unwrap()
}
fn row(f: &ModalForm) -> Value {
    f.submission().unwrap()["values"]["email"]["contact"].clone()
}

#[test]
fn explicit_email_admission_preserves_generic_and_special_view_rejection() {
    for f in [
        ModalForm::parse(&view()).unwrap(),
        ModalForm::parse_with_origin(&view(), None).unwrap(),
        ModalForm::parse_with_context(&view(), None, None).unwrap(),
    ] {
        assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    }
    assert_eq!(row(&parse(&view())), json!({"type":"email_text_input"}));
    let mut special = view();
    special["type"] = json!("app_notification_subscription_configuration");
    assert!(matches!(
        parse(&special).submission(),
        Err(FormError::Unsupported(_))
    ));
    let mut number = view();
    number["blocks"][0]["element"]["type"] = json!("number_input");
    assert!(matches!(
        parse(&number).submission(),
        Err(FormError::Unsupported(_))
    ));
}

#[test]
fn pristine_omission_and_literal_clear_are_distinct_without_mount_edits() {
    for initial in [None, Some("")] {
        let mut v = view();
        if let Some(initial) = initial {
            v["blocks"][0]["element"]["initial_value"] = json!(initial);
        }
        let mut f = parse(&v);
        assert_eq!(f.fields[&key()].value, InputValue::Email(None));
        assert!(!f.fields[&key()].dirty);
        assert_eq!(row(&f), json!({"type":"email_text_input"}));
        f.set_value(&key(), InputValue::Email(Some("a@example.com".into())))
            .unwrap();
        assert_eq!(
            row(&f),
            json!({"type":"email_text_input","value":"a@example.com"})
        );
        f.set_value(&key(), InputValue::Email(Some(String::new())))
            .unwrap();
        assert_eq!(row(&f), json!({"type":"email_text_input","value":""}));
        assert!(f.fields[&key()].dirty);
        assert_eq!(
            f.set_value(&key(), InputValue::Text("wrong".into())),
            Err(FormError::ValueTypeMismatch)
        );
    }
}

#[test]
fn required_email_uses_shared_exact_validator_and_preserves_invalid_literals() {
    let mut v = view();
    v["blocks"][0]["optional"] = json!(false);
    let mut f = parse(&v);
    assert_eq!(f.validate()[0].1, "An email address is required");
    for (literal, valid) in [
        ("UPPER+tag@EXAMPLE.COM", true),
        (".a..b.@example.com", true),
        ("a@b.c", true),
        ("a@localhost", false),
        ("a@-domain.com", false),
        ("a@domain-.com", false),
        ("é@example.com", false),
        (" a@example.com ", false),
        ("a@example.com\n", false),
    ] {
        f.set_value(&key(), InputValue::Email(Some(literal.into())))
            .unwrap();
        assert_eq!(
            f.fields[&key()].value,
            InputValue::Email(Some(literal.into()))
        );
        assert_eq!(f.submission().is_ok(), valid, "{literal:?}");
        if !valid {
            assert_eq!(f.validate()[0].1, "You must enter a valid email address");
        }
    }
}

#[test]
fn unsupported_initial_dispatch_visibility_and_confirmation_cannot_be_edited_away() {
    let mut cases = vec![];
    let mut v = view();
    v["blocks"][0]["element"]["initial_value"] = json!("prefilled@example.com");
    cases.push(v);
    let mut v = view();
    v["blocks"][0]["dispatch_action"] = json!(true);
    cases.push(v);
    for setting in ["dispatch_action_config", "confirm", "visible_to_user_ids"] {
        let mut v = view();
        v["blocks"][0]["element"][setting] = Value::Null;
        cases.push(v);
    }
    let mut v = view();
    v["blocks"][0]["type"] = json!("actions");
    v["blocks"][0]["elements"] = json!([v["blocks"][0]["element"].clone()]);
    cases.push(v);
    for v in cases {
        let mut f = parse(&v);
        if f.fields.contains_key(&key()) {
            f.set_value(&key(), InputValue::Email(Some("valid@example.com".into())))
                .unwrap();
        }
        assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    }
    for initial in [Value::Null, json!(1), json!({})] {
        let mut v = view();
        v["blocks"][0]["element"]["initial_value"] = initial;
        assert!(ModalForm::parse_with_email_context(&v, None, None).is_err());
    }
}

#[test]
fn fresh_saved_rows_do_not_grant_widget_state_or_email_capability() {
    for (saved, allowed) in [
        (json!({"type":"email_text_input"}), true),
        (json!({"type":"email_text_input","value":""}), true),
        (
            json!({"type":"email_text_input","value":"restored@example.com"}),
            false,
        ),
    ] {
        let mut v = view();
        v["state"] = json!({"values":{"email":{"contact":saved.clone()}}});
        let f = parse(&v);
        assert_eq!(f.submission().is_ok(), allowed);
        if allowed {
            assert_eq!(row(&f), saved);
        }
        assert!(ModalForm::parse(&v).unwrap().submission().is_err());
    }
    for value in [Value::Null, json!(false), json!([])] {
        let mut v = view();
        v["state"] =
            json!({"values":{"email":{"contact":{"type":"email_text_input","value":value}}}});
        assert!(ModalForm::parse_with_email_context(&v, None, None).is_err());
    }
}

#[test]
fn matching_remote_updates_preserve_rows_and_recheck_admission_transactionally() {
    let mut f = parse(&view());
    f.set_value(&key(), InputValue::Email(Some("local@example.com".into())))
        .unwrap();
    let mut v = view();
    v["blocks"][0]["label"]["text"] = json!("Updated label");
    v["state"] = json!({"values":{"email":{"contact":{"type":"email_text_input","value":"remote@example.com"}}}});
    f.reconcile(&v).unwrap();
    assert_eq!(row(&f)["value"], json!("local@example.com"));
    assert_eq!(f.fields[&key()].revision, 0);
    v["blocks"][0]["element"]["initial_value"] = json!("unsupported@example.com");
    f.reconcile(&v).unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    assert_eq!(
        f.fields[&key()].value,
        InputValue::Email(Some("local@example.com".into()))
    );
    v["blocks"][0]["element"]["initial_value"] = json!("");
    f.reconcile(&v).unwrap();
    assert!(f.submission().is_ok());
    let before = f.clone();
    let mut removed = v.clone();
    removed["blocks"] = json!([]);
    assert_eq!(f.reconcile(&removed), Err(FormError::ConflictingUpdate));
    assert_eq!(f, before);
    // Capability and matching state survive an acknowledged step update.
    f.reconcile_submitted(&v).unwrap();
    assert_eq!(row(&f)["value"], json!("local@example.com"));
    f.reconcile_submitted(&removed).unwrap();
    assert!(f.fields.is_empty());
    f.reconcile_submitted(&view()).unwrap();
    assert_eq!(row(&f), json!({"type":"email_text_input"}));
}

#[test]
fn pristine_saved_empty_and_server_errors_survive_retained_updates_until_edit() {
    let mut v = view();
    v["state"] = json!({"values":{"email":{"contact":{"type":"email_text_input","value":""}}}});
    let mut f = parse(&v);
    f.reconcile(&view()).unwrap();
    assert_eq!(row(&f)["value"], json!(""));
    assert_eq!(f.fields[&key()].revision, 0);
    f.apply_server_errors(&[("email".into(), "Review this address".into())].into());
    f.reconcile(&view()).unwrap();
    assert_eq!(f.validate()[0].1, "Review this address");
    f.set_value(&key(), InputValue::Email(Some("local@example.com".into())))
        .unwrap();
    f.apply_server_errors(&[("email".into(), "Use your work address".into())].into());
    f.reconcile(&view()).unwrap();
    assert_eq!(f.validate()[0].1, "Use your work address");
    f.set_value(&key(), InputValue::Email(Some("work@example.com".into())))
        .unwrap();
    assert!(f.submission().is_ok());
}
