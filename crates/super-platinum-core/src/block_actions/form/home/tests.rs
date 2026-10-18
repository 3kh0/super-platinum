use super::*;
fn raw() -> Value {
    json!({"id":"V1","type":"home","blocks":[
 {"type":"input","block_id":"text","label":{"type":"plain_text","text":"Name"},"optional":false,"element":{"type":"plain_text_input","action_id":"a","initial_value":"Équipe 東京"}},
 {"type":"input","block_id":"choice","label":{"type":"plain_text","text":"Choice"},"element":{"type":"static_select","action_id":"a","options":[{"text":{"type":"plain_text","text":"One"},"value":"one"}]}},
 {"type":"input","block_id":"checks","label":{"type":"plain_text","text":"Checks"},"element":{"type":"checkboxes","action_id":"a","options":[{"text":{"type":"plain_text","text":"One"},"value":"one"}]}},
 {"type":"input","block_id":"date","label":{"type":"plain_text","text":"Date"},"element":{"type":"datepicker","action_id":"a"}},
 {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"go","text":{"type":"plain_text","text":"Review"}}]}]})
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "a".into(),
    }
}
#[test]
fn quiet_home_state_has_exact_empty_scalars_arrays_and_option_objects_without_modal_validation() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    assert!(f.unsupported.is_empty());
    assert!(!f.fields[&key("choice")].optional);
    assert_eq!(f.dispatch(&key("text")), Some(Dispatch::Quiet));
    f.set(&key("text"), InputValue::Text("  Équipe 東京  ".into()))
        .unwrap();
    assert_eq!(
        f.fields[&key("text")].value,
        InputValue::Text("  Équipe 東京  ".into())
    );
    let state = f.state().unwrap();
    assert_eq!(state["values"]["text"]["a"]["value"], "Équipe 東京");
    assert_eq!(
        state["values"]["choice"]["a"]["selected_option"],
        Value::Null
    );
    assert_eq!(
        state["values"]["checks"]["a"]["selected_options"],
        json!([])
    );
    assert_eq!(state["values"]["date"]["a"]["selected_date"], Value::Null);
    f.set(&key("text"), InputValue::Text(String::new()))
        .unwrap();
    assert_eq!(
        f.state().unwrap()["values"]["text"]["a"]["value"],
        Value::Null
    );
    f.set(&key("choice"), InputValue::Selected(vec!["one".into()]))
        .unwrap();
    assert_eq!(
        f.state().unwrap()["values"]["choice"]["a"]["selected_option"],
        raw()["blocks"][1]["element"]["options"][0]
    );
}
#[test]
fn quiet_text_emission_matches_javascript_trim_without_changing_native_text() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    for (literal, emitted) in [
        ("\u{feff}A\u{feff}", "A"),
        ("\u{0085}A\u{0085}", "\u{0085}A\u{0085}"),
    ] {
        f.set(&key("text"), InputValue::Text(literal.into()))
            .unwrap();
        assert_eq!(
            f.fields[&key("text")].value,
            InputValue::Text(literal.into())
        );
        assert_eq!(f.state().unwrap()["values"]["text"]["a"]["value"], emitted);
    }
}
#[test]
fn malformed_unknown_and_dispatching_controls_cannot_silently_omit_state() {
    for mode in 0..5 {
        let mut v = raw();
        match mode {
            0 => v["blocks"][0]["dispatch_action"] = json!(true),
            1 => v["blocks"][0]["element"]["confirm"] = json!({}),
            2 => v["blocks"][0]["element"]["type"] = json!("rich_text_input"),
            3 => {
                v["state"] =
                    json!({"values":{"unknown":{"x":{"type":"plain_text_input","value":"kept"}}}})
            }
            _ => v["blocks"][0]["element"] = json!({"type":"users_select","action_id":"a"}),
        }
        let f = HomeForm::parse(&v).unwrap();
        assert!(f.state().is_err());
        assert_eq!(f.raw_view(), &v);
    }
    let mut v = raw();
    v["blocks"][1]["block_id"] = json!("text");
    assert!(HomeForm::parse(&v).is_err());
}
#[test]
fn server_update_preserves_dirty_input_and_refuses_removal_or_type_replacement() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    f.set(&key("text"), InputValue::Text(" retained ".into()))
        .unwrap();
    let mut v = raw();
    v["hash"] = json!("new");
    v["blocks"][0]["element"]["initial_value"] = json!("remote");
    f.reconcile(&v).unwrap();
    assert_eq!(
        f.fields[&key("text")].value,
        InputValue::Text(" retained ".into())
    );
    assert_eq!(
        f.state().unwrap()["values"]["text"]["a"]["value"],
        "retained"
    );
    let before = f.clone();
    v["blocks"].as_array_mut().unwrap().remove(0);
    assert!(f.reconcile(&v).is_err());
    assert_eq!(f, before);
    let mut v = raw();
    v["blocks"][0]["element"] = json!({"type":"datepicker","action_id":"a"});
    assert!(f.reconcile(&v).is_err());
    assert_eq!(f, before);
}
#[test]
fn native_parse_error_does_not_authorize_sending_last_valid_typed_state() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    f.fields.get_mut(&key("date")).unwrap().error = Some("Native input invalid".into());
    assert!(f.state().is_err());
    f.set(&key("date"), InputValue::Date(None)).unwrap();
    assert!(f.state().is_ok());
}

#[test]
fn stale_option_value_and_invalid_calendar_value_never_become_null_wire_state() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    f.set(&key("choice"), InputValue::Selected(vec!["foreign".into()]))
        .unwrap();
    assert!(f.state().is_err());
    f.set(&key("choice"), InputValue::Selected(vec![])).unwrap();
    f.set(&key("date"), InputValue::Date(Some("not-a-date".into())))
        .unwrap();
    assert!(f.state().is_err());
}

#[test]
fn invalid_native_text_survives_refresh_and_remount_until_explicit_edit_or_reset() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    f.set_native_error(
        &key("date"),
        "retained native fragment".into(),
        "Invalid native date".into(),
    )
    .unwrap();
    assert!(f.dirty());
    assert!(f.state().is_err());
    let before = f.clone();
    let mut changed = raw();
    changed["hash"] = json!("changed");
    assert!(f.reconcile(&changed).is_err());
    assert_eq!(f, before);
    assert_eq!(
        f.native_error_text(&key("date")),
        Some("retained native fragment")
    );
    f.set(&key("date"), InputValue::Date(None)).unwrap();
    assert_eq!(f.native_error_text(&key("date")), None);
    assert!(f.state().is_ok());
}

#[test]
fn stale_option_keeps_editing_state_and_blocks_cached_action_until_explicit_correction() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    let choice = key("choice");
    f.set(&choice, InputValue::Selected(vec!["one".into()]))
        .unwrap();
    assert_eq!(
        f.action(&choice).unwrap()["selected_option"]["value"],
        "one"
    );
    f.set(&choice, InputValue::Selected(vec!["foreign".into()]))
        .unwrap();
    assert_eq!(
        f.fields[&choice].value,
        InputValue::Selected(vec!["foreign".into()])
    );
    assert!(f.fields[&choice].dirty);
    assert!(f.fields[&choice].error.is_some());
    assert!(!f.wire.contains_key(&choice));
    assert!(f.state().is_err());
    assert!(f.action(&choice).is_err());
    f.reconcile(&raw()).unwrap();
    assert_eq!(
        f.fields[&choice].value,
        InputValue::Selected(vec!["foreign".into()])
    );
    assert!(f.state().is_err());
    assert!(f.action(&choice).is_err());
    f.set(&choice, InputValue::Selected(vec![])).unwrap();
    assert_eq!(f.action(&choice).unwrap()["selected_option"], Value::Null);
    assert!(f.state().is_ok());
    f.set(&choice, InputValue::Selected(vec!["one".into()]))
        .unwrap();
    assert_eq!(
        f.action(&choice).unwrap()["selected_option"],
        raw()["blocks"][1]["element"]["options"][0]
    );
}

#[test]
fn invalid_date_and_native_error_do_not_expose_previous_action() {
    let mut f = HomeForm::parse(&raw()).unwrap();
    let date = key("date");
    f.set(&date, InputValue::Date(Some("2026-10-17".into())))
        .unwrap();
    assert_eq!(f.action(&date).unwrap()["selected_date"], "2026-10-17");
    f.set(&date, InputValue::Date(Some("not-a-date".into())))
        .unwrap();
    assert!(!f.wire.contains_key(&date));
    assert!(f.action(&date).is_err());
    f.set(&date, InputValue::Date(None)).unwrap();
    f.set_native_error(&date, "unfinished".into(), "Invalid native date".into())
        .unwrap();
    assert!(f.action(&date).is_err());
    assert_eq!(f.native_error_text(&date), Some("unfinished"));
    f.set(&date, InputValue::Date(None)).unwrap();
    assert_eq!(f.action(&date).unwrap()["selected_date"], Value::Null);
}
