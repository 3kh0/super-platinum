use super::*;

fn multi(optional: bool, maximum: usize) -> Value {
    let mut raw = raw(optional);
    raw["blocks"][0]["element"] = json!({
        "type":"multi_users_select", "action_id":"pick",
        "initial_users":["UINITIAL1"], "max_selected_items":maximum,
        "placeholder":{"type":"plain_text","text":"Choose reviewers"}
    });
    raw
}
fn ids(values: &[&str]) -> InputValue {
    InputValue::Users(values.iter().map(|value| (*value).into()).collect())
}

#[test]
fn inline_over_limit_and_required_empty_are_committed_before_home_validation() {
    for optional in [false, true] {
        let raw = multi(optional, 2);
        let view = view(raw.clone());
        let mut form = HomeForm::parse(&raw).unwrap();
        assert_eq!(form.dispatch(&key()), Some(Dispatch::Quiet));
        assert!(matches!(
            form.user_field(&key()).unwrap().kind,
            InputKind::MultiUser { .. }
        ));
        assert!(form.external_field(&key()).is_err());
        for selected_ids in [vec!["USELECT03", "USELECT01", "USELECT02"], vec![]] {
            form.set(&key(), ids(&selected_ids)).unwrap();
            assert_eq!(
                form.fields[&key()].error.is_some(),
                selected_ids.len() > 2 || (!optional && selected_ids.is_empty())
            );
            assert_eq!(
                selected(&form),
                json!({
                    "type":"multi_users_select", "selected_users":selected_ids
                })
            );
            for (context, expected_team) in [(None, "T2"), (Some("TCONTEXT1"), "TCONTEXT1")] {
                let wire = action(&view, &form, context);
                let state: Value = serde_json::from_str(&wire["state"]).unwrap();
                assert_eq!(state["values"]["member"]["pick"], selected(&form));
                assert_eq!(wire["service_team_id"], expected_team);
                let actions: Value = serde_json::from_str(&wire["actions"]).unwrap();
                assert_eq!(actions.as_array().unwrap().len(), 1);
                assert_eq!(actions[0]["type"], "button");
            }
        }
        assert_eq!(form.raw_view(), &raw);
    }
}

#[test]
fn saved_arrays_and_initial_are_distinct_from_unproven_null_raw_state() {
    let mut over = multi(false, 1);
    over["blocks"][0]["element"]["initial_users"] = json!(["UINITIAL1", "UINITIAL2"]);
    let initial = HomeForm::parse(&over).unwrap();
    assert!(initial.fields[&key()].error.is_some());
    assert_eq!(
        selected(&initial)["selected_users"],
        json!(["UINITIAL1", "UINITIAL2"])
    );
    for saved in [json!([]), json!(["USAVED001", "USAVED002"])] {
        let mut raw = multi(false, 1);
        raw["state"] = json!({"values":{"member":{"pick":{
            "type":"multi_users_select", "selected_users":saved
        }}}});
        let form = HomeForm::parse(&raw).unwrap();
        assert_eq!(selected(&form)["selected_users"], saved);
    }
    let mut raw = multi(false, 1);
    raw["state"] = json!({"values":{"member":{"pick":{"type":"multi_users_select"}}}});
    assert!(HomeForm::parse(&raw).is_err());
    raw["state"]["values"]["member"]["pick"]["selected_users"] = Value::Null;
    assert!(HomeForm::parse(&raw).is_err());
    // Constructor savedUsers null/missing fallback is independently observed
    // at the widget prop boundary, not proof of raw view-state admission.
    raw.as_object_mut().unwrap().remove("state");
    raw["blocks"][0]["element"]["initial_users"] = Value::Null;
    assert!(
        selected(&HomeForm::parse(&raw).unwrap())
            .get("selected_users")
            .is_none()
    );
}

#[test]
fn compatible_max_and_initial_refresh_do_not_invent_a_widget_callback() {
    let mut raw = multi(false, 2);
    let mut form = HomeForm::parse(&raw).unwrap();
    form.set(&key(), ids(&["USELECT03", "USELECT01", "USELECT02"]))
        .unwrap();
    let committed = selected(&form);
    for maximum in [4, 1] {
        raw["hash"] = json!(format!("max-{maximum}"));
        raw["blocks"][0]["element"]["max_selected_items"] = json!(maximum);
        raw["blocks"][0]["element"]["initial_users"] = json!(["UNEWINIT1"]);
        raw["state"] = json!({"values":{"member":{"pick":{
            "type":"multi_users_select", "selected_users":["USAVED001"]
        }}}});
        form.reconcile(&raw).unwrap();
        assert_eq!(selected(&form), committed);
        assert_eq!(form.fields[&key()].error.is_some(), maximum == 1);
        assert_eq!(
            form.user_field(&key()).unwrap().raw["initial_users"],
            json!(["UNEWINIT1"])
        );
    }
    // A pristine parent follows its incoming state. Widget saved-prop no-op
    // does not restore old committed state or override core reconciliation.
    let mut pristine = HomeForm::parse(&raw).unwrap();
    raw["state"]["values"]["member"]["pick"]["selected_users"] = json!([]);
    pristine.reconcile(&raw).unwrap();
    assert_eq!(selected(&pristine)["selected_users"], json!([]));
}

#[test]
fn home_exception_preserves_identity_duplicate_type_and_array_bounds() {
    let raw = multi(true, 1);
    for bad in [
        ids(&["USELECT01", "USELECT01"]),
        ids(&["CCHANNEL1"]),
        ids(&["Ubad"]),
        ids(&[""]),
    ] {
        let mut form = HomeForm::parse(&raw).unwrap();
        form.set(&key(), ids(&["USELECT01"])).unwrap();
        form.set(&key(), bad).unwrap();
        assert!(form.state().is_err());
        assert!(form.action(&key()).is_err());
    }
    let mut form = HomeForm::parse(&raw).unwrap();
    let thousand: Vec<String> = (0..1000).map(|n| format!("U{n:08}")).collect();
    form.set(&key(), InputValue::Users(thousand.clone()))
        .unwrap();
    assert_eq!(
        selected(&form)["selected_users"].as_array().unwrap().len(),
        1000
    );
    let mut too_many = thousand;
    too_many.push("U00001000".into());
    form.set(&key(), InputValue::Users(too_many)).unwrap();
    assert!(form.state().is_err());
    assert!(form.set(&key(), InputValue::User(None)).is_err());
    assert!(
        form.user_field(&InputKey {
            block_id: "foreign".into(),
            action_id: "pick".into()
        })
        .is_err()
    );
    form.fields.get_mut(&key()).unwrap().raw["max_selected_items"] = json!(3);
    assert!(form.user_field(&key()).is_err());
}

#[test]
fn multi_dispatch_admission_rejects_malformed_source() {
    for flag in ["confirm", "visible_to_user_ids", "dispatch_action_config"] {
        let mut raw = multi(true, 2);
        raw["blocks"][0]["element"][flag] = json!({});
        assert!(HomeForm::parse(&raw).unwrap().user_field(&key()).is_err());
    }
    let mut raw = multi(true, 2);
    raw["blocks"][0]["dispatch_action"] = json!(true);
    let form = HomeForm::parse(&raw).unwrap();
    assert!(form.user_field(&key()).is_ok());
    assert!(form.state().is_ok());
    assert_eq!(form.dispatch(&key()), Some(Dispatch::Change));
    for malformed in [
        json!(true),
        json!("USELECT01"),
        json!([null]),
        json!(["USELECT01", "USELECT01"]),
    ] {
        let mut raw = multi(true, 2);
        raw["blocks"][0]["element"]["initial_users"] = malformed;
        assert!(HomeForm::parse(&raw).is_err());
    }
    let mut raw = multi(true, 2);
    raw["blocks"][0]["element"]["max_selected_items"] = json!(0);
    assert!(HomeForm::parse(&raw).is_err());
}

#[test]
fn message_apply_and_modal_validation_keep_their_required_and_max_policy() {
    let raw = multi(false, 1);
    let mut field = HomeForm::parse(&raw)
        .unwrap()
        .fields
        .remove(&key())
        .unwrap();
    assert!(validate_message_users(&field, &[]).is_err());
    let over = vec!["USELECT01".into(), "USELECT02".into()];
    assert!(validate_message_users(&field, &over).is_err());
    field.value = InputValue::Users(over);
    assert!(values::error(&field).is_some());
    field.value = InputValue::Users(vec![]);
    assert!(values::error(&field).is_some());
}
