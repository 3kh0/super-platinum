use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "url".into(),
        action_id: "value".into(),
    }
}
fn view(initial: Option<&str>) -> Value {
    let mut v = json!({"id":"V_PREFILL","type":"modal","title":{"type":"plain_text","text":"Project link"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":[{
        "type":"input","block_id":"url","optional":true,"label":{"type":"plain_text","text":"URL"},
        "element":{"type":"url_text_input","action_id":"value"}}]});
    if let Some(initial) = initial {
        v["blocks"][0]["element"]["initial_value"] = json!(initial);
    }
    v
}
fn parse(v: &Value) -> ModalForm {
    ModalForm::parse_with_url_prefill_context(v, None, None).unwrap()
}
fn row(f: &ModalForm) -> Value {
    f.submission().unwrap()["values"]["url"]["value"].clone()
}

#[test]
fn fresh_url_prefill_initializes_literal_typed_submission_without_callback() {
    let raw = view(Some("https://EXAMPLE.com/Project?q=one%20two"));
    for old in [
        ModalForm::parse(&raw).unwrap(),
        ModalForm::parse_with_email_context(&raw, None, None).unwrap(),
        ModalForm::parse_with_scalar_context(&raw, None, None).unwrap(),
    ] {
        assert!(matches!(old.submission(), Err(FormError::Unsupported(_))));
    }
    let f = parse(&raw);
    assert_eq!(
        row(&f),
        json!({"type":"url_text_input","value":"https://EXAMPLE.com/Project?q=one%20two"})
    );
    assert!(!f.fields[&key()].dirty);
    assert_eq!(f.fields[&key()].revision, 0);
    for initial in [None, Some("")] {
        assert_eq!(
            row(&parse(&view(initial))),
            json!({"type":"url_text_input"})
        );
    }
    let mut required = raw.clone();
    required["blocks"][0]["optional"] = json!(false);
    assert!(parse(&required).submission().is_ok());
    let mut invalid = parse(&view(Some("not a url")));
    assert!(invalid.unsupported.is_empty());
    assert_eq!(
        invalid.fields[&key()].value,
        InputValue::Url(Some("not a url".into()))
    );
    assert_eq!(invalid.validate()[0].1, "You must enter a valid URL");
    invalid
        .set_value(
            &key(),
            InputValue::Url(Some("typed.example/project".into())),
        )
        .unwrap();
    assert_eq!(row(&invalid)["value"], json!("typed.example/project"));
}

#[test]
fn fresh_url_prefill_excludes_every_matching_raw_saved_row_and_other_guards() {
    for saved in [
        json!({"type":"url_text_input"}),
        json!({"type":"url_text_input","value":""}),
        json!({"type":"url_text_input","value":"saved.example"}),
        Value::Null,
        json!({"type":"url_text_input","value":null}),
    ] {
        let mut v = view(Some("initial.example"));
        v["state"] = json!({"values":{"url":{"value":saved}}});
        match ModalForm::parse_with_url_prefill_context(&v, None, None) {
            Err(_) => {}
            Ok(mut f) => {
                f.set_value(&key(), InputValue::Url(Some("typed.example".into())))
                    .unwrap();
                assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
            }
        }
    }
    // A container without this action is not a matching saved row.
    let mut empty = view(Some("initial.example"));
    empty["state"] = json!({"values":{"url":{}}});
    assert_eq!(row(&parse(&empty))["value"], json!("initial.example"));
    for name in ["dispatch_action_config", "confirm", "visible_to_user_ids"] {
        let mut v = view(Some("initial.example"));
        v["blocks"][0]["element"][name] = Value::Null;
        assert!(matches!(
            parse(&v).submission(),
            Err(FormError::Unsupported(_))
        ));
    }
    let mut v = view(Some("initial.example"));
    v["blocks"][0]["dispatch_action"] = json!(true);
    assert!(matches!(
        parse(&v).submission(),
        Err(FormError::Unsupported(_))
    ));
    let mut v = view(Some("initial.example"));
    v["type"] = json!("app_notification_subscription_configuration");
    assert!(matches!(
        parse(&v).submission(),
        Err(FormError::Unsupported(_))
    ));
    for initial in [Value::Null, json!(1), json!(false)] {
        let mut v = view(None);
        v["blocks"][0]["element"]["initial_value"] = initial;
        assert!(ModalForm::parse_with_url_prefill_context(&v, None, None).is_err());
    }
}

#[test]
fn retained_prefill_preserves_pristine_and_dirty_values_without_blank_origin_escalation() {
    let mut f = parse(&view(Some("original.example")));
    f.reconcile(&view(Some("remote.example"))).unwrap();
    assert_eq!(row(&f)["value"], json!("original.example"));
    assert!(!f.fields[&key()].dirty);
    assert_eq!(f.fields[&key()].revision, 0);
    f.reconcile(&view(None)).unwrap();
    f.reconcile(&view(Some("another.example"))).unwrap();
    assert_eq!(row(&f)["value"], json!("original.example"));
    f.set_value(&key(), InputValue::Url(Some("owned.example".into())))
        .unwrap();
    f.reconcile(&view(Some("remote.example"))).unwrap();
    assert_eq!(row(&f)["value"], json!("owned.example"));
    let mut blank = parse(&view(None));
    for initial in ["remote.example", "another.example"] {
        blank.reconcile(&view(Some(initial))).unwrap();
        assert!(matches!(blank.submission(), Err(FormError::Unsupported(_))));
        assert_eq!(blank.fields[&key()].value, InputValue::Url(None));
    }
    blank.reconcile(&view(None)).unwrap();
    assert_eq!(row(&blank), json!({"type":"url_text_input"}));
    blank
        .reconcile(&view(Some("still-remote.example")))
        .unwrap();
    assert!(matches!(blank.submission(), Err(FormError::Unsupported(_))));
}

#[test]
fn prefill_provenance_prunes_and_cannot_cross_foreign_parent_or_bad_saved_wire() {
    let mut f = parse(&view(Some("original.example")));
    let before = f.clone();
    let mut foreign_view = view(Some("foreign.example"));
    foreign_view["id"] = json!("V_OTHER");
    assert_eq!(
        f.reconcile(&foreign_view),
        Err(FormError::ConflictingUpdate)
    );
    assert_eq!(f, before);
    let mut malformed = view(Some("remote.example"));
    malformed["state"] = json!({"values":{"url":{"value":{"type":"url_text_input","value":null}}}});
    assert!(f.reconcile(&malformed).is_err());
    assert_eq!(f, before);
    f.set_value(&key(), InputValue::Url(Some("accepted.example".into())))
        .unwrap();
    f.reconcile_submitted(&view(Some("remote.example")))
        .unwrap();
    assert_eq!(row(&f)["value"], json!("accepted.example"));
    let mut removed = view(None);
    removed["blocks"] = json!([]);
    f.reconcile_submitted(&removed).unwrap();
    assert!(f.fields.is_empty());
    assert!(f.url_prefill_keys.is_empty());
    // Model removal need not mean the old widget was disposed before a batched
    // readdition. Even a blank optional recreation must remain unsupported.
    for initial in [None, None, Some("recreated.example")] {
        f.reconcile_submitted(&view(initial)).unwrap();
        assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    }
    f.reconcile_submitted(&removed).unwrap();
    let mut introduced = view(Some("new.example"));
    introduced["blocks"][0]["block_id"] = json!("new_url");
    f.reconcile_submitted(&introduced).unwrap();
    assert!(matches!(f.submission(), Err(FormError::Unsupported(_))));
    // A new owned form parent is the explicit admission boundary.
    assert_eq!(
        row(&parse(&view(Some("new.example"))))["value"],
        json!("new.example")
    );
    // Blank-origin legacy recreations remain supported and never gain prefill.
    let mut blank = parse(&view(None));
    blank.reconcile(&removed).unwrap();
    blank.reconcile(&view(None)).unwrap();
    assert_eq!(row(&blank), json!({"type":"url_text_input"}));
    let mut text = view(Some("Text"));
    text["blocks"][0]["element"]["type"] = json!("plain_text_input");
    let mut foreign = parse(&text);
    foreign
        .reconcile_submitted(&view(Some("foreign.example")))
        .unwrap();
    assert!(matches!(
        foreign.submission(),
        Err(FormError::Unsupported(_))
    ));
}
