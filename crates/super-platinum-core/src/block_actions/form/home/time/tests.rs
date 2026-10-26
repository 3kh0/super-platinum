use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "clock".into(),
        action_id: "a".into(),
    }
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[
        {"type":"input","block_id":"clock","optional":optional,"dispatch_action":!quiet,
         "element":{"type":"timepicker","action_id":"a","initial_time":"09:30",
          "timezone":"America/New_York","placeholder":{"type":"plain_text","text":"Pick a time"},"opaque":{"keep":1}}}]})
}
fn selected(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["clock"]["a"]["selected_time"].clone()
}

#[test]
fn time_callback_is_sealed_exact_and_quiet_still_commits() {
    for quiet in [false, true] {
        let view = raw(true, quiet);
        let mut form = HomeForm::parse(&view).unwrap();
        let event = form.time_input(&key(), Some("17:45".into())).unwrap();
        assert_eq!(selected(&form), "17:45");
        assert_eq!(event.is_none(), quiet);
        if let Some(event) = event {
            assert_eq!(
                event.action(),
                &json!({"type":"timepicker","block_id":"clock","action_id":"a",
                "selected_time":"17:45","initial_time":"09:30","timezone":"America/New_York"})
            );
            assert!(event.same_view(&view));
            assert_eq!(
                form.time_input(&key(), Some("17:45".into()))
                    .unwrap()
                    .unwrap(),
                event,
                "Direct callbacks emit; same-visible suppression is the renderer leaf's responsibility"
            );
        }
    }
}

#[test]
fn null_and_empty_commit_before_required_suppression_without_clear_ui_claim() {
    for optional in [false, true] {
        for value in [None, Some(String::new())] {
            let mut form = HomeForm::parse(&raw(optional, false)).unwrap();
            let event = form.time_input(&key(), value).unwrap();
            assert_eq!(selected(&form), Value::Null);
            assert_eq!(form.fields[&key()].error.is_some(), !optional);
            assert_eq!(event.is_some(), optional);
        }
    }
    for timezone in [None, Some("")] {
        let mut view = raw(true, false);
        let element = view["blocks"][0]["element"].as_object_mut().unwrap();
        element.remove("initial_time");
        element.remove("timezone");
        if let Some(timezone) = timezone {
            element.insert("timezone".into(), json!(timezone));
        }
        let mut form = HomeForm::parse(&view).unwrap();
        let event = form
            .time_input(&key(), Some("00:00".into()))
            .unwrap()
            .unwrap();
        assert_eq!(
            event.action(),
            &json!({"type":"timepicker","block_id":"clock","action_id":"a","selected_time":"00:00"})
        );
        assert_eq!(selected(&form), "00:00");
    }
}

#[test]
fn malformed_canonical_candidates_and_forged_public_fields_are_atomic_refusals() {
    let pristine = HomeForm::parse(&raw(true, false)).unwrap();
    for value in ["24:00", "12:60", "9:30", "09:30:00", "5pm", "not-a-time"] {
        let mut form = pristine.clone();
        assert!(form.time_input(&key(), Some(value.into())).is_err());
        assert_eq!(form, pristine);
    }
    for mode in 0..5 {
        let mut form = pristine.clone();
        let field = form.fields.get_mut(&key()).unwrap();
        match mode {
            0 => field.value = InputValue::Time(Some("12:00".into())),
            1 => field.optional = false,
            2 => field.kind = InputKind::Date,
            3 => field.raw["initial_time"] = json!("12:00"),
            _ => field.error = Some("Forged".into()),
        }
        let before = form.clone();
        assert!(form.time_input(&key(), None).is_err());
        assert!(form.time_seed(&key()).is_err());
        assert!(form.state().is_err());
        assert_eq!(form, before);
    }
    let mut form = pristine;
    assert!(
        form.time_input(
            &InputKey {
                block_id: "foreign".into(),
                action_id: "a".into()
            },
            None
        )
        .is_err()
    );
    for malformed in [json!("24:00"), json!(1)] {
        let mut view = raw(true, false);
        view["blocks"][0]["element"]["initial_time"] = malformed;
        if let Ok(mut form) = HomeForm::parse(&view) {
            assert!(form.time_input(&key(), Some("12:34".into())).is_err());
        }
    }
}

#[test]
fn seeded_display_uses_saved_or_source_initial_without_committing_fallback() {
    for saved in [Value::Null, json!(""), json!("12:00")] {
        let mut view = raw(true, false);
        view["state"] =
            json!({"values":{"clock":{"a":{"type":"timepicker","selected_time":saved}}}});
        let form = HomeForm::parse(&view).unwrap();
        let before = form.clone();
        let expected = if saved == json!("12:00") {
            "12:00"
        } else {
            "09:30"
        };
        assert_eq!(form.time_seed(&key()).unwrap().as_deref(), Some(expected));
        assert_eq!(form, before);
        assert_eq!(
            selected(&form),
            if saved == json!("") {
                Value::Null
            } else {
                saved
            }
        );
    }
    let mut view = raw(true, false);
    view["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_time");
    assert_eq!(
        HomeForm::parse(&view).unwrap().time_seed(&key()).unwrap(),
        None
    );
    let mut form = HomeForm::parse(&raw(true, false)).unwrap();
    form.time_input(&key(), Some(String::new())).unwrap();
    assert_eq!(selected(&form), Value::Null);
    assert_eq!(form.time_seed(&key()).unwrap().as_deref(), Some("09:30"));
    assert_eq!(
        selected(&form),
        Value::Null,
        "Fresh display fallback does not rewrite parent null"
    );
    let mut malformed = raw(true, false);
    malformed["blocks"][0]["element"]["initial_time"] = Value::Null;
    assert!(
        HomeForm::parse(&malformed).is_err(),
        "Existing raw initial null admission unchanged"
    );
}

#[test]
fn explicit_parent_token_retains_pristine_and_latest_dirty_parent_without_dirtying() {
    for dirty in [false, true] {
        let mut form = HomeForm::parse(&raw(true, false)).unwrap();
        let parent = form.time_parent(&key()).unwrap();
        if dirty {
            form.time_input(&key(), Some("12:00".into())).unwrap();
        }
        let before = form.clone();
        let mut changed = raw(true, false);
        changed["blocks"][0]["element"]["initial_time"] = json!("11:00");
        changed["state"] =
            json!({"values":{"clock":{"a":{"type":"timepicker","selected_time":"13:00"}}}});
        form.reconcile_with_calendar_parents(&changed, &[], &[parent.clone()])
            .unwrap();
        assert_eq!(selected(&form), selected(&before));
        assert_eq!(form.fields[&key()].dirty, dirty);
        assert_eq!(form.fields[&key()].revision, before.fields[&key()].revision);
        assert_eq!(form.raw_view(), &changed);
        changed["blocks"][0]["optional"] = json!(false);
        form.reconcile_with_calendar_parents(&changed, &[], &[parent])
            .unwrap();
        assert_eq!(selected(&form), selected(&before));
    }
    let mut form = HomeForm::parse(&raw(true, false)).unwrap();
    let mut changed = raw(true, false);
    changed["blocks"][0]["element"]["initial_time"] = json!("11:00");
    form.reconcile(&changed).unwrap();
    assert_eq!(
        selected(&form),
        "11:00",
        "No mounted attestation preserves old reconciliation semantics"
    );
}

#[test]
fn token_mismatch_removal_type_and_native_errors_refuse_reconcile_atomically() {
    let pristine = HomeForm::parse(&raw(true, false)).unwrap();
    let parent = pristine.time_parent(&key()).unwrap();
    for mode in 0..5 {
        let mut form = pristine.clone();
        let mut changed = raw(true, false);
        match mode {
            0 => changed["app_id"] = json!("OTHER"),
            1 => changed["blocks"] = json!([]),
            2 => changed["blocks"][0]["element"] = json!({"type":"datepicker","action_id":"a"}),
            3 => form
                .set_native_error(&key(), "partial".into(), "Invalid native time".into())
                .unwrap(),
            _ => form.fields.get_mut(&key()).unwrap().value = InputValue::Time(None),
        }
        let before = form.clone();
        assert!(
            form.reconcile_with_calendar_parents(&changed, &[], &[parent.clone()])
                .is_err()
        );
        assert_eq!(form, before);
    }
    let mut form = pristine;
    assert!(
        form.reconcile_with_calendar_parents(&raw(true, false), &[], &[parent.clone(), parent])
            .is_err()
    );
}

#[test]
fn valid_native_ack_clears_only_safety_error_and_keeps_parent_and_domain_validation() {
    let mut form = HomeForm::parse(&raw(false, false)).unwrap();
    form.time_input(&key(), None).unwrap();
    let before = form.clone();
    form.set_native_error(&key(), "bad buffer".into(), "Invalid native time".into())
        .unwrap();
    assert!(form.state().is_err());
    assert!(form.time_seed(&key()).is_err());
    let unsafe_before = form.clone();
    assert!(
        form.time_acknowledge_native_buffer(&key(), Some("24:00"))
            .is_err()
    );
    assert_eq!(form, unsafe_before);
    form.time_acknowledge_native_buffer(&key(), Some("13:00"))
        .unwrap();
    assert_eq!(form, before);
    assert_eq!(selected(&form), Value::Null);
    assert!(form.fields[&key()].error.is_some());
    form.set_native_error(&key(), String::new(), "Native incomplete".into())
        .unwrap();
    assert_eq!(form.native_error_text(&key()), Some(""));
    form.time_acknowledge_native_buffer(&key(), Some(""))
        .unwrap();
    assert_eq!(form, before);
    for mode in 0..2 {
        let mut wrong = form.clone();
        if mode == 0 {
            wrong.fields.get_mut(&key()).unwrap().kind = InputKind::Date;
        } else {
            wrong.fields.get_mut(&key()).unwrap().value = InputValue::Time(Some("12:00".into()));
        }
        let before = wrong.clone();
        assert!(wrong.time_acknowledge_native_buffer(&key(), None).is_err());
        assert_eq!(wrong, before);
    }
}

#[test]
fn timezone_props_preserve_trusted_parent_until_current_callback() {
    for dirty in [false, true] {
        let mut form = HomeForm::parse(&raw(true, false)).unwrap();
        let parent = form.time_parent(&key()).unwrap();
        if dirty {
            form.time_input(&key(), Some("10:00".into())).unwrap();
        }
        let before = form.state().unwrap();
        let mut changed = raw(true, false);
        changed["blocks"][0]["element"]["initial_time"] = json!("11:00");
        changed["blocks"][0]["element"]["timezone"] = json!("Asia/Tokyo");
        form.reconcile_with_calendar_parents(&changed, &[], &[parent.clone()])
            .unwrap();
        assert_eq!(form.state().unwrap(), before);
        assert_eq!(form.fields[&key()].dirty, dirty);
        let event = form
            .time_input(&key(), Some("12:00".into()))
            .unwrap()
            .unwrap();
        assert_eq!(
            event.action(),
            &json!({"type":"timepicker","block_id":"clock","action_id":"a","selected_time":"12:00","initial_time":"11:00","timezone":"Asia/Tokyo"})
        );
        assert_eq!(
            form.state().unwrap()["values"]["clock"]["a"],
            json!({"type":"timepicker","selected_time":"12:00","timezone":"Asia/Tokyo"})
        );
        changed["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("timezone");
        form.reconcile_with_calendar_parents(&changed, &[], &[parent])
            .unwrap();
        assert_eq!(
            form.state().unwrap()["values"]["clock"]["a"]["timezone"],
            "Asia/Tokyo"
        );
        let event = form
            .time_input(&key(), Some("23:59".into()))
            .unwrap()
            .unwrap();
        assert!(event.action().get("timezone").is_none());
        assert!(
            form.state().unwrap()["values"]["clock"]["a"]
                .get("timezone")
                .is_none()
        );
    }
}

#[test]
fn mixed_calendar_attestation_retains_both_pristine_parents_and_old_date_api_stays_scoped() {
    let mut view = raw(true, false);
    view["blocks"].as_array_mut().unwrap().push(json!({"type":"input","block_id":"day","optional":true,"dispatch_action":true,"element":{"type":"datepicker","action_id":"a","initial_date":"2026-10-25"}}));
    let day = InputKey {
        block_id: "day".into(),
        action_id: "a".into(),
    };
    let mut form = HomeForm::parse(&view).unwrap();
    let dates = [form.date_parent(&day).unwrap()];
    let times = [form.time_parent(&key()).unwrap()];
    let mut changed = view.clone();
    changed["blocks"][0]["element"]["initial_time"] = json!("11:00");
    changed["blocks"][1]["element"]["initial_date"] = json!("2026-10-26");
    let original = form.state().unwrap();
    let mut date_only = form.clone();
    date_only
        .reconcile_with_date_parents(&changed, &dates)
        .unwrap();
    assert_eq!(selected(&date_only), "11:00");
    assert_eq!(
        date_only.state().unwrap()["values"]["day"]["a"]["selected_date"],
        "2026-10-25"
    );
    form.reconcile_with_calendar_parents(&changed, &dates, &times)
        .unwrap();
    assert_eq!(form.state().unwrap(), original);
    assert!(!form.dirty());
}

#[test]
fn time_admission_stays_input_only_and_preserves_existing_raw_time_grammar() {
    for kind in ["actions", "section"] {
        let mut view = raw(true, false);
        let element = view["blocks"][0]["element"].clone();
        view["blocks"][0] = if kind == "actions" {
            json!({"type":"actions","block_id":"clock","elements":[element]})
        } else {
            json!({"type":"section","block_id":"clock","accessory":element})
        };
        if let Ok(mut form) = HomeForm::parse(&view) {
            assert!(form.time_field(&key()).is_err());
            assert!(form.time_input(&key(), Some("12:00".into())).is_err());
        }
    }
    for property in ["confirm", "visible_to_user_ids"] {
        let mut view = raw(true, false);
        view["blocks"][0]["element"][property] = Value::Null;
        let mut form = HomeForm::parse(&view).unwrap();
        assert!(form.time_input(&key(), Some("12:00".into())).is_err());
    }
    // Existing optional-empty validation precedes HH:mm validation. An empty
    // raw initial is admitted and seeds no time; explicit null still is not
    // an initial string. Neither policy is changed by Time dispatch.
    for optional in [false, true] {
        let mut view = raw(optional, false);
        view["blocks"][0]["element"]["initial_time"] = json!("");
        let mut form = HomeForm::parse(&view).unwrap();
        assert_eq!(form.time_seed(&key()).unwrap(), None);
        assert_eq!(selected(&form), Value::Null);
        assert_eq!(form.fields[&key()].error.is_some(), !optional);
        let event = form
            .time_input(&key(), Some("12:00".into()))
            .unwrap()
            .unwrap();
        assert!(event.action().get("initial_time").is_none());
        assert_eq!(event.action()["selected_time"], "12:00");
    }
    for bad in [json!("24:00"), json!("9:30"), Value::Null] {
        let mut view = raw(true, false);
        view["blocks"][0]["element"]["initial_time"] = bad;
        if let Ok(mut form) = HomeForm::parse(&view) {
            assert!(form.time_seed(&key()).is_err());
            assert!(form.time_input(&key(), Some("12:00".into())).is_err());
        }
    }
}
