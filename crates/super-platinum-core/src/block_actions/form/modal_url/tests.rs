use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "url".into(),
        action_id: "value".into(),
    }
}
fn view() -> Value {
    json!({"id":"V_URL","type":"modal","title":{"type":"plain_text","text":"Project"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":[{
        "type":"input","block_id":"url","optional":true,"label":{"type":"plain_text","text":"Project URL"},
        "element":{"type":"url_text_input","action_id":"value"}}]})
}
fn parse(v: &Value) -> ModalForm {
    ModalForm::parse_with_scalar_context(v, None, None).unwrap()
}
fn row(f: &ModalForm) -> Value {
    f.submission().unwrap()["values"]["url"]["value"].clone()
}

#[test]
fn url_validator_matches_bounded_independent_node_predicate_without_rewriting() {
    // Own Node-host transcription of exact sxgo/uKWJ; no downloaded factory,
    // native field or Slack runtime was executed. Receipt is retained in
    // tmp/parity-search-recovery-20261027/modal-url-validation-oracle-01.json.
    let cases: Value = serde_json::from_str(include_str!("validation_cases.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 59);
    for case in cases.as_array().unwrap() {
        let literal = case["literal"].as_str().unwrap();
        assert_eq!(
            valid(literal),
            case["accepts"].as_bool().unwrap(),
            "{literal:?}"
        );
    }
    let mut f = parse(&view());
    for literal in [
        "example.com/project",
        "www.example.com/a b",
        "mailto:a@example.com",
        "https://EXAMPLE.com/%zz",
    ] {
        f.set_value(&key(), InputValue::Url(Some(literal.into())))
            .unwrap();
        assert_eq!(row(&f), json!({"type":"url_text_input","value":literal}));
    }
    let rejected = "  example.com  ";
    f.set_value(&key(), InputValue::Url(Some(rejected.into())))
        .unwrap();
    assert_eq!(
        f.fields[&key()].value,
        InputValue::Url(Some(rejected.into()))
    );
    assert!(matches!(f.submission(), Err(FormError::Validation(_))));
}

#[test]
fn url_admission_is_explicit_normal_modal_only_and_never_granted_by_saved_state() {
    for f in [
        ModalForm::parse(&view()).unwrap(),
        ModalForm::parse_with_origin(&view(), None).unwrap(),
        ModalForm::parse_with_context(&view(), None, None).unwrap(),
        ModalForm::parse_with_email_context(&view(), None, None).unwrap(),
    ] {
        assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    }
    let mut special = view();
    special["type"] = json!("app_notification_subscription_configuration");
    assert!(matches!(
        parse(&special).submission(),
        Err(FormError::Unsupported(_))
    ));
    let mut home = view();
    home["type"] = json!("home");
    assert!(ModalForm::parse_with_scalar_context(&home, None, None).is_err());
    home["app_id"] = json!("A1");
    home["app_installed_team_id"] = json!("T1");
    home["bot_id"] = json!("B1");
    let home_form = home::HomeForm::parse(&home).unwrap();
    assert!(matches!(
        home_form.fields[&key()].kind,
        InputKind::Unsupported { .. }
    ));
    assert!(home_form.state().is_err());
    let mut f = parse(&view());
    assert_eq!(row(&f), json!({"type":"url_text_input"}));
    f.set_value(&key(), InputValue::Url(Some("".into())))
        .unwrap();
    assert_eq!(row(&f), json!({"type":"url_text_input","value":""}));
    assert_eq!(
        f.set_value(&key(), InputValue::Email(Some("a@example.com".into()))),
        Err(FormError::ValueTypeMismatch)
    );
}

#[test]
fn url_required_invalid_and_server_errors_validate_callback_literal_before_submission() {
    let mut v = view();
    v["blocks"][0]["optional"] = json!(false);
    let mut f = parse(&v);
    assert_eq!(f.validate()[0].1, "A URL is required");
    for literal in ["", "not a url", "bad host name"] {
        f.set_value(&key(), InputValue::Url(Some(literal.into())))
            .unwrap();
        assert_eq!(
            f.fields[&key()].value,
            InputValue::Url(Some(literal.into()))
        );
        assert_eq!(
            f.validate()[0].1,
            if literal.is_empty() {
                "A URL is required"
            } else {
                "You must enter a valid URL"
            }
        );
        assert!(matches!(f.submission(), Err(FormError::Validation(_))));
    }
    f.set_value(&key(), InputValue::Url(Some("example.com".into())))
        .unwrap();
    f.apply_server_errors(&[("url".into(), "Use the project link".into())].into());
    f.reconcile(&v).unwrap();
    assert_eq!(f.validate()[0].1, "Use the project link");
    f.set_value(&key(), InputValue::Url(Some("example.com/project".into())))
        .unwrap();
    assert_eq!(row(&f)["value"], json!("example.com/project"));
}

#[test]
fn url_initial_and_interaction_boundaries_cannot_be_edited_away() {
    let mut cases = vec![];
    let mut v = view();
    v["blocks"][0]["element"]["initial_value"] = json!("example.com");
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
            f.set_value(&key(), InputValue::Url(Some("example.com".into())))
                .unwrap();
        }
        assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    }
    for initial in [Value::Null, json!(false), json!(3)] {
        let mut v = view();
        v["blocks"][0]["element"]["initial_value"] = initial;
        assert!(ModalForm::parse_with_scalar_context(&v, None, None).is_err());
    }
    let mut empty = view();
    empty["blocks"][0]["element"]["initial_value"] = json!("");
    assert_eq!(row(&parse(&empty)), json!({"type":"url_text_input"}));
}

#[test]
fn url_fresh_restoration_and_retained_drafts_have_distinct_admission_and_pruning() {
    for (saved, allowed) in [
        (json!({"type":"url_text_input"}), true),
        (json!({"type":"url_text_input","value":""}), true),
        (
            json!({"type":"url_text_input","value":"example.com"}),
            false,
        ),
    ] {
        let mut v = view();
        v["state"] = json!({"values":{"url":{"value":saved.clone()}}});
        let f = parse(&v);
        assert_eq!(f.submission().is_ok(), allowed);
        if allowed {
            assert_eq!(row(&f), saved);
        }
        assert!(
            ModalForm::parse_with_email_context(&v, None, None)
                .unwrap()
                .submission()
                .is_err()
        );
    }
    for value in [Value::Null, json!(1), json!({})] {
        let mut v = view();
        v["state"] = json!({"values":{"url":{"value":{"type":"url_text_input","value":value}}}});
        assert!(ModalForm::parse_with_scalar_context(&v, None, None).is_err());
    }
    let mut f = parse(&view());
    f.set_value(
        &key(),
        InputValue::Url(Some("local.example/project".into())),
    )
    .unwrap();
    let mut v = view();
    v["state"] =
        json!({"values":{"url":{"value":{"type":"url_text_input","value":"remote.example"}}}});
    f.reconcile(&v).unwrap();
    assert_eq!(row(&f)["value"], json!("local.example/project"));
    assert_eq!(f.fields[&key()].revision, 0);
    v["blocks"][0]["element"]["initial_value"] = json!("unsupported.example");
    f.reconcile(&v).unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    assert_eq!(
        f.fields[&key()].value,
        InputValue::Url(Some("local.example/project".into()))
    );
    v["blocks"][0]["element"]["initial_value"] = json!("");
    f.reconcile(&v).unwrap();
    let before = f.clone();
    let mut removed = view();
    removed["blocks"] = json!([]);
    assert_eq!(f.reconcile(&removed), Err(FormError::ConflictingUpdate));
    assert_eq!(f, before);
    let mut replaced = view();
    replaced["blocks"][0]["element"]["type"] = json!("email_text_input");
    assert_eq!(f.reconcile(&replaced), Err(FormError::ConflictingUpdate));
    assert_eq!(f, before);
    f.reconcile_submitted(&v).unwrap();
    assert_eq!(row(&f)["value"], json!("local.example/project"));
    f.reconcile_submitted(&removed).unwrap();
    f.reconcile_submitted(&view()).unwrap();
    assert_eq!(row(&f), json!({"type":"url_text_input"}));
}
