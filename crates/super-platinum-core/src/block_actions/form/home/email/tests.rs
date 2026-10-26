use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "email".into(),
        action_id: "value".into(),
    }
}
fn raw() -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[{
        "type":"input","block_id":"email","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Email"},
        "element":{"type":"email_text_input","action_id":"value","initial_value":"Initial@Example.com","placeholder":{"type":"plain_text","text":"Contact"},"dispatch_action_config":{"trigger_actions_on":["on_character_entered","on_enter_pressed"]}}
    }]})
}
fn row(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["email"]["value"].clone()
}
fn input(form: &mut HomeForm, literal: &str, trigger: EmailTrigger) -> Option<TextEvent> {
    form.email_input(&key(), literal.into(), trigger).unwrap()
}
fn reconcile(form: &mut HomeForm, view: &Value, parents: &[EmailParent]) -> Result<(), FormError> {
    form.reconcile_with_email_parents(view, &[], &[], &[], &[], &[], &[], &[], &[], parents)
}
#[test]
fn exact_original_validator_23_boundaries_and_parent_first_state() {
    let cases = [
        ("", false),
        ("a@example.com", true),
        ("UPPER+tag@EXAMPLE.COM", true),
        ("a@localhost", false),
        ("a@b.c", true),
        (".a..b.@example.com", true),
        ("a!#$%&'*+/=?^_`{|}~-@example.com", true),
        ("a@-domain.com", false),
        ("a@domain-.com", false),
        ("a@do--main.com", true),
        ("a@a_b.com", false),
        ("\"quoted\"@example.com", false),
        ("a@[127.0.0.1]", false),
        ("\u{00e9}@example.com", false),
        ("a@\u{00e9}xample.com", false),
        ("a@xn--xample-9ua.com", true),
        (" a@example.com ", false),
        ("a@example.com\n", false),
        ("a@example.com\r\n", false),
        ("a,b@example.com", false),
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@example.com",
            true,
        ),
        (
            "a@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.com",
            true,
        ),
        (
            "a@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.com",
            false,
        ),
    ];
    assert_eq!(cases.len(), 23);
    for (value, accepted) in cases {
        assert_eq!(valid_email(value), accepted, "{value:?}");
        let mut form = HomeForm::parse(&raw()).unwrap();
        assert_eq!(
            input(&mut form, value, EmailTrigger::Change).is_some(),
            value.is_empty() || accepted,
            "{value:?}"
        );
        assert_eq!(row(&form), json!({"type":"email_text_input","value":value}));
        assert_eq!(
            form.fields[&key()].error.is_some(),
            !value.is_empty() && !accepted
        );
    }
}
#[test]
fn first_new_parent_is_omitted_even_with_valid_or_invalid_initial() {
    for initial in [None, Some(""), Some("Initial@Example.com"), Some("invalid")] {
        let mut view = raw();
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
        if let Some(v) = initial {
            view["blocks"][0]["element"]["initial_value"] = json!(v);
        }
        let mut form = HomeForm::parse(&view).unwrap();
        let before = form.clone();
        assert_eq!(
            form.email_mount(&key(), EmailMount::NewParent).unwrap(),
            initial.unwrap_or_default()
        );
        assert_eq!(form, before);
        assert_eq!(row(&form), json!({"type":"email_text_input"}));
        assert!(!form.fields[&key()].dirty);
    }
}
#[test]
fn literal_empty_is_not_null_and_ordinary_actions_omit_initial_metadata() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    for trigger in [EmailTrigger::Change, EmailTrigger::Enter] {
        let event = input(&mut form, "UPPER+tag@EXAMPLE.COM", trigger).unwrap();
        assert_eq!(
            event.action(),
            &json!({"type":"email_text_input","block_id":"email","action_id":"value","value":"UPPER+tag@EXAMPLE.COM"})
        );
        let empty = input(&mut form, "", trigger).unwrap();
        assert_eq!(empty.action()["value"], "");
        assert_eq!(row(&form), json!({"type":"email_text_input","value":""}));
    }
}
#[test]
fn no_implicit_trigger_and_quiet_still_commits() {
    for (config, change, enter) in [
        (None, false, false),
        (Some(json!({})), false, false),
        (Some(json!({"trigger_actions_on":[]})), false, false),
        (
            Some(json!({"trigger_actions_on":["on_character_entered"]})),
            true,
            false,
        ),
        (
            Some(json!({"trigger_actions_on":["on_enter_pressed"]})),
            false,
            true,
        ),
    ] {
        let mut view = raw();
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("dispatch_action_config");
        if let Some(v) = config {
            view["blocks"][0]["element"]["dispatch_action_config"] = v;
        }
        let mut form = HomeForm::parse(&view).unwrap();
        assert_eq!(
            input(&mut form, "a@example.com", EmailTrigger::Change).is_some(),
            change
        );
        assert_eq!(
            input(&mut form, "b@example.com", EmailTrigger::Enter).is_some(),
            enter
        );
        assert_eq!(row(&form)["value"], "b@example.com");
        view["blocks"][0]["dispatch_action"] = json!(false);
        let mut quiet = HomeForm::parse(&view).unwrap();
        assert!(input(&mut quiet, "a@example.com", EmailTrigger::Change).is_none());
        assert_eq!(row(&quiet)["value"], "a@example.com");
    }
}
#[test]
fn invalid_and_required_empty_commit_without_replacing_prior_event() {
    let mut view = raw();
    view["blocks"][0]["optional"] = json!(false);
    let mut form = HomeForm::parse(&view).unwrap();
    let queued = input(&mut form, "queued@example.com", EmailTrigger::Change).unwrap();
    for value in ["invalid", " a@example.com ", "a@example.com\n", ""] {
        assert!(input(&mut form, value, EmailTrigger::Change).is_none());
        assert_eq!(row(&form)["value"], value);
        assert_eq!(
            form.fields[&key()].error.as_deref(),
            Some(if value.is_empty() {
                "An email address is required"
            } else {
                "You must enter a valid email address"
            })
        );
    }
    assert_eq!(queued.action()["value"], "queued@example.com");
}
#[test]
fn equal_callbacks_are_not_deduped_and_no_text_trim_is_inherited() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    for trigger in [EmailTrigger::Change, EmailTrigger::Enter] {
        let first = input(&mut form, "a@example.com", trigger).unwrap();
        let second = input(&mut form, "a@example.com", trigger).unwrap();
        assert_eq!(first, second);
    }
    assert!(input(&mut form, " a@example.com ", EmailTrigger::Change).is_none());
    assert_eq!(row(&form)["value"], " a@example.com ");
}
#[test]
fn retained_parent_fresh_mount_overwrites_only_with_truthy_initial() {
    for initial in [None, Some(""), Some("Initial@Example.com"), Some("invalid")] {
        let mut view = raw();
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
        if let Some(v) = initial {
            view["blocks"][0]["element"]["initial_value"] = json!(v);
        }
        let mut form = HomeForm::parse(&view).unwrap();
        input(&mut form, "stored@example.com", EmailTrigger::Change);
        assert_eq!(
            form.email_seed(&key()).unwrap(),
            initial.unwrap_or_default()
        );
        assert_eq!(row(&form)["value"], "stored@example.com");
        assert_eq!(
            form.email_mount(&key(), EmailMount::RetainedParent)
                .unwrap(),
            initial.unwrap_or_default()
        );
        assert_eq!(
            row(&form)["value"],
            initial
                .filter(|v| !v.is_empty())
                .unwrap_or("stored@example.com")
        );
        assert!(form.email_mount(&key(), EmailMount::NewParent).is_err());
    }
}
#[test]
fn initialized_props_retain_omitted_and_dirty_parent_without_implicit_mount() {
    for dirty in [false, true] {
        let view = raw();
        let mut form = HomeForm::parse(&view).unwrap();
        form.email_mount(&key(), EmailMount::NewParent).unwrap();
        if dirty {
            input(&mut form, "typed@example.com", EmailTrigger::Change);
        }
        let token = form.email_parent(&key()).unwrap();
        let before = row(&form);
        let mut next = view.clone();
        next["blocks"][0]["element"]["initial_value"] = json!("new@example.com");
        reconcile(&mut form, &next, std::slice::from_ref(&token)).unwrap();
        assert_eq!(row(&form), before);
        next["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
        reconcile(&mut form, &next, &[token]).unwrap();
        assert_eq!(row(&form), before);
        assert_eq!(form.email_seed(&key()).unwrap(), "");
    }
}
#[test]
fn saved_parent_is_not_a_local_seed_and_saved_null_is_not_new_raw_authority() {
    for saved in [
        json!({"type":"email_text_input"}),
        json!({"type":"email_text_input","value":""}),
        json!({"type":"email_text_input","value":"stored@example.com"}),
    ] {
        let mut view = raw();
        view["state"] = json!({"values":{"email":{"value":saved}}});
        let mut form = HomeForm::parse(&view).unwrap();
        assert_eq!(row(&form), saved);
        assert_eq!(form.email_seed(&key()).unwrap(), "Initial@Example.com");
        let mut initial_form = form.clone();
        initial_form
            .email_mount(&key(), EmailMount::NewParent)
            .unwrap();
        assert_eq!(row(&initial_form)["value"], "Initial@Example.com");
        form.email_mount(&key(), EmailMount::RetainedParent)
            .unwrap();
        assert_eq!(row(&form)["value"], "Initial@Example.com");
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
        let mut no_initial = HomeForm::parse(&view).unwrap();
        assert_eq!(
            no_initial
                .email_mount(&key(), EmailMount::NewParent)
                .unwrap(),
            ""
        );
        assert_eq!(row(&no_initial), saved);
    }
    for value in [Value::Null, json!(2)] {
        let mut view = raw();
        view["state"] =
            json!({"values":{"email":{"value":{"type":"email_text_input","value":value}}}});
        assert!(HomeForm::parse(&view).is_err());
        let mut view = raw();
        view["blocks"][0]["element"]["initial_value"] = value;
        assert!(HomeForm::parse(&view).is_err());
    }
}
#[test]
fn sealed_parent_and_current_source_reject_foreign_or_corrupt_inventory() {
    let view = raw();
    let form = HomeForm::parse(&view).unwrap();
    let token = form.email_parent(&key()).unwrap();
    for property in ["id", "app_id", "app_installed_team_id", "bot_id"] {
        let mut next = view.clone();
        next[property] = json!("OTHER");
        let mut f = form.clone();
        assert!(reconcile(&mut f, &next, std::slice::from_ref(&token)).is_err());
        assert_eq!(f, form);
    }
    let mut f = form.clone();
    assert!(reconcile(&mut f, &view, &[token.clone(), token]).is_err());
    let mut next = view.clone();
    next["blocks"][0]["element"] = json!({"type":"plain_text_input","action_id":"value"});
    assert!(reconcile(&mut f, &next, &[form.email_parent(&key()).unwrap()]).is_err());
    for mode in 0..4 {
        let mut f = form.clone();
        match mode {
            0 => {
                f.fields.clear();
            }
            1 => {
                f.fields.get_mut(&key()).unwrap().value =
                    InputValue::Email(Some("forged@example.com".into()));
            }
            2 => {
                f.fields.get_mut(&key()).unwrap().raw["initial_value"] =
                    json!("forged@example.com");
            }
            _ => {
                f.fields.get_mut(&key()).unwrap().optional = false;
            }
        }
        assert!(f.state().is_err());
        assert!(
            f.email_input(&key(), "fresh@example.com".into(), EmailTrigger::Change)
                .is_err()
        );
    }
}
#[test]
fn native_ack_keeps_invalid_domain_parent_and_does_not_commit_buffer() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    input(&mut form, "invalid", EmailTrigger::Change);
    let before = row(&form);
    form.set_native_error(&key(), "unseen".into(), "Native custody".into())
        .unwrap();
    assert!(form.state().is_err());
    form.email_acknowledge_native_buffer(&key(), "good@example.com")
        .unwrap();
    assert_eq!(row(&form), before);
    assert_eq!(
        form.fields[&key()].error.as_deref(),
        Some("You must enter a valid email address")
    );
    assert!(form.set(&key(), InputValue::Email(None)).is_err());
}
#[test]
fn home_only_admission_preserves_modal_noninput_and_config_guards() {
    let mut modal = raw();
    modal["type"] = json!("modal");
    modal["title"] = json!({"type":"plain_text","text":"Modal"});
    let parsed = ModalForm::parse(&modal).unwrap();
    assert!(matches!(
        parsed.fields[&key()].kind,
        InputKind::Unsupported { .. }
    ));
    let mut view = raw();
    view["blocks"][0]["type"] = json!("section");
    view["blocks"][0]["accessory"] = view["blocks"][0]["element"].take();
    assert!(HomeForm::parse(&view).unwrap().state().is_err());
    for config in [
        Value::Null,
        json!({"trigger_actions_on":null}),
        json!({"trigger_actions_on":["unknown"]}),
    ] {
        let mut view = raw();
        view["blocks"][0]["element"]["dispatch_action_config"] = config;
        assert!(HomeForm::parse(&view).is_err());
    }
    for property in ["confirm", "visible_to_user_ids"] {
        let mut view = raw();
        view["blocks"][0]["element"][property] = json!({});
        assert!(HomeForm::parse(&view).unwrap().state().is_err());
    }
}
