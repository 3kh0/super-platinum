use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "day".into(),
        action_id: "a".into(),
    }
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[
        {"type":"input","block_id":"day","optional":optional,"dispatch_action":!quiet,
         "element":{"type":"datepicker","action_id":"a","initial_date":"2026-10-13",
          "placeholder":{"type":"plain_text","text":"Pick a day"},"opaque":{"keep":1}}}]})
}
fn selected(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["day"]["a"]["selected_date"].clone()
}

#[test]
fn date_callback_is_sealed_exact_and_quiet_still_commits() {
    for quiet in [false, true] {
        let view = raw(true, quiet);
        let mut form = HomeForm::parse(&view).unwrap();
        let event = form.date_input(&key(), Some("2028-02-29".into())).unwrap();
        assert_eq!(selected(&form), "2028-02-29");
        assert_eq!(event.is_none(), quiet);
        if let Some(event) = event {
            assert_eq!(
                event.action(),
                &json!({"type":"datepicker","block_id":"day","action_id":"a",
                "selected_date":"2028-02-29","initial_date":"2026-10-13"})
            );
            assert!(event.same_view(&view));
            assert_eq!(
                form.date_input(&key(), Some("2028-02-29".into()))
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
            let event = form.date_input(&key(), value).unwrap();
            assert_eq!(selected(&form), Value::Null);
            assert_eq!(form.fields[&key()].error.is_some(), !optional);
            assert_eq!(event.is_some(), optional);
        }
    }
    for initial in [None, Some("")] {
        let mut view = raw(true, false);
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_date");
        if let Some(initial) = initial {
            view["blocks"][0]["element"]["initial_date"] = json!(initial);
        }
        let mut form = HomeForm::parse(&view).unwrap();
        let event = form
            .date_input(&key(), Some("2026-11-01".into()))
            .unwrap()
            .unwrap();
        assert!(event.action().get("initial_date").is_none());
    }
}

#[test]
fn malformed_native_candidates_and_forged_public_fields_are_atomic_refusals() {
    let pristine = HomeForm::parse(&raw(true, false)).unwrap();
    for value in [
        "2027-02-29",
        "2026-2-01",
        "2026-10-13T00:00:00",
        "not-a-date",
    ] {
        let mut form = pristine.clone();
        assert!(form.date_input(&key(), Some(value.into())).is_err());
        assert_eq!(form, pristine);
    }
    for mode in 0..5 {
        let mut form = pristine.clone();
        let field = form.fields.get_mut(&key()).unwrap();
        match mode {
            0 => field.value = InputValue::Date(Some("2026-12-01".into())),
            1 => field.optional = false,
            2 => field.kind = InputKind::Time { timezone: None },
            3 => field.raw["initial_date"] = json!("2026-12-01"),
            _ => field.error = Some("Forged".into()),
        }
        let before = form.clone();
        assert!(form.date_input(&key(), None).is_err());
        assert!(form.date_seed(&key()).is_err());
        assert!(form.state().is_err());
        assert_eq!(form, before);
    }
    let mut form = pristine;
    assert!(
        form.date_input(
            &InputKey {
                block_id: "foreign".into(),
                action_id: "a".into()
            },
            None
        )
        .is_err()
    );
    for malformed in [json!("2027-02-29"), json!(1)] {
        let mut view = raw(true, false);
        view["blocks"][0]["element"]["initial_date"] = malformed;
        if let Ok(mut form) = HomeForm::parse(&view) {
            assert!(form.date_input(&key(), Some("2027-03-01".into())).is_err());
        }
    }
}

#[test]
fn seeded_display_uses_saved_or_source_initial_without_committing_fallback() {
    for saved in [Value::Null, json!(""), json!("2026-12-01")] {
        let mut view = raw(true, false);
        view["state"] = json!({"values":{"day":{"a":{"type":"datepicker","selected_date":saved}}}});
        let form = HomeForm::parse(&view).unwrap();
        let before = form.clone();
        let expected = if saved == json!("2026-12-01") {
            "2026-12-01"
        } else {
            "2026-10-13"
        };
        assert_eq!(form.date_seed(&key()).unwrap().as_deref(), Some(expected));
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
    for initial in [None, Some("")] {
        let mut view = raw(true, false);
        view["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_date");
        if let Some(v) = initial {
            view["blocks"][0]["element"]["initial_date"] = json!(v);
        }
        assert_eq!(
            HomeForm::parse(&view).unwrap().date_seed(&key()).unwrap(),
            None
        );
    }
    let mut malformed = raw(true, false);
    malformed["blocks"][0]["element"]["initial_date"] = Value::Null;
    assert!(
        HomeForm::parse(&malformed).is_err(),
        "Existing raw initial null admission unchanged"
    );
}

#[test]
fn explicit_parent_token_retains_pristine_and_latest_dirty_parent_without_dirtying() {
    for dirty in [false, true] {
        let mut form = HomeForm::parse(&raw(true, false)).unwrap();
        let parent = form.date_parent(&key()).unwrap();
        if dirty {
            form.date_input(&key(), Some("2026-12-01".into())).unwrap();
        }
        let before = form.clone();
        let mut changed = raw(true, false);
        changed["blocks"][0]["element"]["initial_date"] = json!("2027-01-02");
        changed["state"] =
            json!({"values":{"day":{"a":{"type":"datepicker","selected_date":"2027-03-04"}}}});
        form.reconcile_with_date_parents(&changed, &[parent.clone()])
            .unwrap();
        assert_eq!(selected(&form), selected(&before));
        assert_eq!(form.fields[&key()].dirty, dirty);
        assert_eq!(form.fields[&key()].revision, before.fields[&key()].revision);
        assert_eq!(form.raw_view(), &changed);
        changed["blocks"][0]["optional"] = json!(false);
        form.reconcile_with_date_parents(&changed, &[parent])
            .unwrap();
        assert_eq!(selected(&form), selected(&before));
    }
    let mut form = HomeForm::parse(&raw(true, false)).unwrap();
    let mut changed = raw(true, false);
    changed["blocks"][0]["element"]["initial_date"] = json!("2027-01-02");
    form.reconcile(&changed).unwrap();
    assert_eq!(
        selected(&form),
        "2027-01-02",
        "No mounted attestation preserves old reconciliation semantics"
    );
}

#[test]
fn token_mismatch_removal_type_and_native_errors_refuse_reconcile_atomically() {
    let pristine = HomeForm::parse(&raw(true, false)).unwrap();
    let parent = pristine.date_parent(&key()).unwrap();
    for mode in 0..5 {
        let mut form = pristine.clone();
        let mut changed = raw(true, false);
        match mode {
            0 => changed["app_id"] = json!("OTHER"),
            1 => changed["blocks"] = json!([]),
            2 => changed["blocks"][0]["element"] = json!({"type":"timepicker","action_id":"a"}),
            3 => form
                .set_native_error(&key(), "partial".into(), "Invalid native date".into())
                .unwrap(),
            _ => form.fields.get_mut(&key()).unwrap().value = InputValue::Date(None),
        }
        let before = form.clone();
        assert!(
            form.reconcile_with_date_parents(&changed, &[parent.clone()])
                .is_err()
        );
        assert_eq!(form, before);
    }
    let mut form = pristine;
    assert!(
        form.reconcile_with_date_parents(&raw(true, false), &[parent.clone(), parent])
            .is_err()
    );
}

#[test]
fn valid_native_ack_clears_only_safety_error_and_keeps_parent_and_domain_validation() {
    let mut form = HomeForm::parse(&raw(false, false)).unwrap();
    form.date_input(&key(), None).unwrap();
    let before = form.clone();
    form.set_native_error(&key(), "bad buffer".into(), "Invalid native date".into())
        .unwrap();
    assert!(form.state().is_err());
    assert!(form.date_seed(&key()).is_err());
    let unsafe_before = form.clone();
    assert!(
        form.date_acknowledge_native_buffer(&key(), Some("2027-02-29"))
            .is_err()
    );
    assert_eq!(form, unsafe_before);
    form.date_acknowledge_native_buffer(&key(), Some("2027-03-04"))
        .unwrap();
    assert_eq!(form, before);
    assert_eq!(selected(&form), Value::Null);
    assert!(form.fields[&key()].error.is_some());
    form.set_native_error(&key(), String::new(), "Native incomplete".into())
        .unwrap();
    assert_eq!(form.native_error_text(&key()), Some(""));
    form.date_acknowledge_native_buffer(&key(), Some(""))
        .unwrap();
    assert_eq!(form, before);
    for mode in 0..2 {
        let mut wrong = form.clone();
        if mode == 0 {
            wrong.fields.get_mut(&key()).unwrap().kind = InputKind::Time { timezone: None };
        } else {
            wrong.fields.get_mut(&key()).unwrap().value =
                InputValue::Date(Some("2026-12-01".into()));
        }
        let before = wrong.clone();
        assert!(wrong.date_acknowledge_native_buffer(&key(), None).is_err());
        assert_eq!(wrong, before);
    }
}
