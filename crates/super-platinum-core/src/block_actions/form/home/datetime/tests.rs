use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "stamp".into(),
        action_id: "a".into(),
    }
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[
        {"type":"input","block_id":"stamp","optional":optional,"dispatch_action":!quiet,
        "element":{"type":"datetimepicker","action_id":"a","initial_date_time":1792946262,
        "timezone":"ignored","placeholder":{"type":"plain_text","text":"Ignored"}}}]})
}
fn row(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["stamp"]["a"].clone()
}
fn without_initial(optional: bool) -> Value {
    let mut v = raw(optional, false);
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_date_time");
    v
}
#[test]
fn datetime_pristine_omission_is_distinct_from_callback_null_and_zero() {
    for optional in [true, false] {
        for initial in [None, Some(0)] {
            let mut view = without_initial(optional);
            if let Some(v) = initial {
                view["blocks"][0]["element"]["initial_date_time"] = json!(v);
            }
            let mut form = HomeForm::parse(&view).unwrap();
            assert_eq!(row(&form), json!({"type":"datetimepicker"}));
            assert_eq!(form.datetime_seed(&key()).unwrap(), 0);
            assert_eq!(form.fields[&key()].error.is_some(), !optional);
            for value in [Some(0), None] {
                let event = form.datetime_input(&key(), value).unwrap();
                assert_eq!(
                    row(&form),
                    json!({"type":"datetimepicker","selected_date_time":null})
                );
                assert_eq!(event.is_some(), optional);
                if let Some(e) = event {
                    assert_eq!(
                        e.action(),
                        &json!({"type":"datetimepicker","block_id":"stamp","action_id":"a","selected_date_time":null})
                    );
                }
            }
        }
    }
}
#[test]
fn datetime_signed_callback_preserves_seconds_and_exact_metadata_quiet_commits() {
    for quiet in [false, true] {
        let view = raw(false, quiet);
        let mut f = HomeForm::parse(&view).unwrap();
        for epoch in [-172800, -1, 1792946262] {
            let event = f.datetime_input(&key(), Some(epoch)).unwrap();
            assert_eq!(
                row(&f),
                json!({"type":"datetimepicker","selected_date_time":epoch})
            );
            assert_eq!(f.datetime_seed(&key()).unwrap(), epoch);
            assert!(f.fields[&key()].error.is_none());
            assert_eq!(event.is_none(), quiet);
            if let Some(e) = event {
                assert!(e.same_view(&view));
                assert_eq!(
                    e.action(),
                    &json!({"type":"datetimepicker","block_id":"stamp","action_id":"a","selected_date_time":epoch,"initial_date_time":1792946262})
                );
            }
        }
    }
    let mut v = raw(true, false);
    v["blocks"][0]["element"]["initial_date_time"] = json!(-1);
    let mut f = HomeForm::parse(&v).unwrap();
    assert_eq!(f.datetime_seed(&key()).unwrap(), -1);
    assert_eq!(
        f.datetime_input(&key(), Some(42))
            .unwrap()
            .unwrap()
            .action()["initial_date_time"],
        -1
    );
}
#[test]
fn datetime_uses_signed_js_date_range_without_chrono_or_modal_normalization() {
    let original = HomeForm::parse(&raw(true, false)).unwrap();
    for epoch in [-8_640_000_000_000, 8_640_000_000_000] {
        let mut f = original.clone();
        assert!(f.datetime_input(&key(), Some(epoch)).unwrap().is_some());
        assert_eq!(row(&f)["selected_date_time"], epoch);
        let mut v = raw(true, false);
        v["blocks"][0]["element"]["initial_date_time"] = json!(epoch);
        assert_eq!(
            HomeForm::parse(&v).unwrap().datetime_seed(&key()).unwrap(),
            epoch
        );
    }
    for epoch in [i64::MIN, -8_640_000_000_001, 8_640_000_000_001, i64::MAX] {
        let mut f = original.clone();
        assert!(f.datetime_input(&key(), Some(epoch)).is_err());
        assert_eq!(f, original);
        let mut v = raw(true, false);
        v["blocks"][0]["element"]["initial_date_time"] = json!(epoch);
        let f = HomeForm::parse(&v).unwrap();
        assert!(f.state().is_err());
        assert!(f.datetime_seed(&key()).is_err());
    }
    let mut generic = original.fields[&key()].clone();
    generic.value = InputValue::DateTime(Some(-1));
    assert!(
        values::error(&generic).is_some(),
        "Generic Modal negative policy remains unchanged"
    );
    assert!(error(&generic).is_none());
}
#[test]
fn datetime_seed_fallback_is_display_only_and_preserves_existing_raw_saved_projection() {
    for saved in [Value::Null, json!(0), json!(-1), json!(1792946263)] {
        let mut v = raw(true, false);
        v["state"] =
            json!({"values":{"stamp":{"a":{"type":"datetimepicker","selected_date_time":saved}}}});
        let f = HomeForm::parse(&v).unwrap();
        let before = f.clone();
        let expected = saved.as_i64().filter(|v| *v != 0).unwrap_or(1792946262);
        assert_eq!(f.datetime_seed(&key()).unwrap(), expected);
        assert_eq!(f, before);
        assert_eq!(row(&f)["selected_date_time"], saved);
    }
    let mut f = HomeForm::parse(&raw(true, false)).unwrap();
    f.datetime_input(&key(), None).unwrap();
    assert_eq!(f.datetime_seed(&key()).unwrap(), 1792946262);
    assert_eq!(row(&f)["selected_date_time"], Value::Null);
    for bad in [Value::Null, json!("42"), json!(1.5)] {
        let mut v = raw(true, false);
        v["blocks"][0]["element"]["initial_date_time"] = bad;
        assert!(
            HomeForm::parse(&v).is_err(),
            "Raw initial grammar is not widened"
        );
    }
}
#[test]
fn datetime_parent_retains_pristine_omission_and_latest_dirty_wire() {
    for initial in [false, true] {
        for dirty in [false, true] {
            let view = if initial {
                raw(true, false)
            } else {
                without_initial(true)
            };
            let mut f = HomeForm::parse(&view).unwrap();
            let token = f.datetime_parent(&key()).unwrap();
            if dirty {
                f.datetime_input(&key(), Some(-1)).unwrap();
            }
            let before = f.clone();
            let mut changed = view.clone();
            changed["blocks"][0]["element"]["initial_date_time"] = json!(1792999999);
            changed["blocks"][0]["element"]["timezone"] = json!("new ignored zone");
            f.reconcile_with_all_calendar_parents(&changed, &[], &[], &[token.clone()])
                .unwrap();
            assert_eq!(row(&f), row(&before));
            assert_eq!(f.fields[&key()].dirty, dirty);
            assert_eq!(f.fields[&key()].revision, before.fields[&key()].revision);
            let e = f.datetime_input(&key(), Some(123456789)).unwrap().unwrap();
            assert_eq!(e.action()["initial_date_time"], 1792999999);
            assert!(e.action().get("timezone").is_none());
            f.datetime_input(&key(), None).unwrap();
            f.reconcile_with_all_calendar_parents(&changed, &[], &[], &[token])
                .unwrap();
            assert_eq!(
                row(&f),
                json!({"type":"datetimepicker","selected_date_time":null})
            );
        }
    }
    let mut f = HomeForm::parse(&without_initial(true)).unwrap();
    f.reconcile(&raw(true, false)).unwrap();
    assert_eq!(row(&f)["selected_date_time"], 1792946262);
}
#[test]
fn datetime_parent_refuses_foreign_removed_changed_or_duplicate_identity_atomically() {
    let original = HomeForm::parse(&raw(true, false)).unwrap();
    let token = original.datetime_parent(&key()).unwrap();
    for mode in 0..7 {
        let mut f = original.clone();
        let mut v = raw(true, false);
        match mode {
            0 => v["id"] = json!("V2"),
            1 => v["app_id"] = json!("A2"),
            2 => v["app_installed_team_id"] = json!("T2"),
            3 => v["bot_id"] = json!("B2"),
            4 => v["blocks"] = json!([]),
            5 => v["blocks"][0]["element"] = json!({"type":"datepicker","action_id":"a"}),
            _ => {
                f.set_native_error(&key(), "draft".into(), "Unsafe".into())
                    .unwrap();
            }
        }
        let before = f.clone();
        assert!(
            f.reconcile_with_all_calendar_parents(&v, &[], &[], &[token.clone()])
                .is_err()
        );
        assert_eq!(f, before);
    }
    let mut f = original.clone();
    assert!(
        f.reconcile_with_all_calendar_parents(&raw(true, false), &[], &[], &[token.clone(), token])
            .is_err()
    );
    assert_eq!(f, original);
}
#[test]
fn datetime_native_ack_clears_only_safety_and_preserves_missing_or_null_parent() {
    for explicit in [false, true] {
        let mut f = HomeForm::parse(&without_initial(false)).unwrap();
        if explicit {
            f.datetime_input(&key(), None).unwrap();
        }
        let wire = row(&f);
        let value = f.fields[&key()].value.clone();
        let revision = f.fields[&key()].revision;
        f.set_native_error(&key(), String::new(), "Native incomplete".into())
            .unwrap();
        assert!(f.state().is_err());
        let before = f.clone();
        assert!(
            f.datetime_acknowledge_native_buffer(&key(), Some(i64::MAX))
                .is_err()
        );
        assert_eq!(f, before);
        f.datetime_acknowledge_native_buffer(&key(), Some(-1))
            .unwrap();
        assert_eq!(row(&f), wire);
        assert_eq!(f.fields[&key()].value, value);
        assert_eq!(f.fields[&key()].revision, revision);
        assert!(f.fields[&key()].error.is_some());
        assert_eq!(f.native_error_text(&key()), None);
    }
}
#[test]
fn datetime_public_corruption_and_noninput_admission_cannot_forge_callback() {
    let original = HomeForm::parse(&raw(true, false)).unwrap();
    for mode in 0..5 {
        let mut f = original.clone();
        let field = f.fields.get_mut(&key()).unwrap();
        match mode {
            0 => field.value = InputValue::DateTime(Some(42)),
            1 => field.kind = InputKind::Date,
            2 => field.optional = false,
            3 => field.raw["initial_date_time"] = json!(42),
            _ => field.error = Some("Forged".into()),
        }
        let before = f.clone();
        assert!(f.datetime_input(&key(), Some(123)).is_err());
        assert!(f.datetime_seed(&key()).is_err());
        assert!(f.state().is_err());
        assert_eq!(f, before);
    }
    for kind in ["actions", "section"] {
        let mut v = raw(true, false);
        let e = v["blocks"][0]["element"].clone();
        v["blocks"][0] = if kind == "actions" {
            json!({"type":kind,"block_id":"stamp","elements":[e]})
        } else {
            json!({"type":kind,"block_id":"stamp","accessory":e})
        };
        let mut f = HomeForm::parse(&v).unwrap();
        assert!(f.datetime_field(&key()).is_err());
        assert!(f.datetime_input(&key(), Some(1)).is_err());
    }
    for property in ["confirm", "visible_to_user_ids"] {
        let mut v = raw(true, false);
        v["blocks"][0]["element"][property] = Value::Null;
        assert!(HomeForm::parse(&v).unwrap().datetime_field(&key()).is_err());
    }
}
#[test]
fn datetime_all_calendar_parents_are_additive_and_old_signatures_remain_scoped() {
    let mut v = raw(true, false);
    v["blocks"].as_array_mut().unwrap().extend([
        json!({"type":"input","block_id":"day","optional":true,"element":{"type":"datepicker","action_id":"a","initial_date":"2026-10-25"}}),
        json!({"type":"input","block_id":"clock","optional":true,"element":{"type":"timepicker","action_id":"a","initial_time":"09:30"}})]);
    let day = InputKey {
        block_id: "day".into(),
        action_id: "a".into(),
    };
    let clock = InputKey {
        block_id: "clock".into(),
        action_id: "a".into(),
    };
    let mut f = HomeForm::parse(&v).unwrap();
    let dates = [f.date_parent(&day).unwrap()];
    let times = [f.time_parent(&clock).unwrap()];
    let datetimes = [f.datetime_parent(&key()).unwrap()];
    let old = f.state().unwrap();
    let mut changed = v.clone();
    changed["blocks"][0]["element"]["initial_date_time"] = json!(42);
    changed["blocks"][1]["element"]["initial_date"] = json!("2026-10-26");
    changed["blocks"][2]["element"]["initial_time"] = json!("11:00");
    let mut legacy = f.clone();
    legacy
        .reconcile_with_calendar_parents(&changed, &dates, &times)
        .unwrap();
    assert_eq!(row(&legacy)["selected_date_time"], 42);
    f.reconcile_with_all_calendar_parents(&changed, &dates, &times, &datetimes)
        .unwrap();
    assert_eq!(f.state().unwrap(), old);
    assert!(!f.dirty());
}
