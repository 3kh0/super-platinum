use super::*;

fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Choice {value}")},"value":value})
}
fn input(id: &str, mut element: Value) -> Value {
    element["action_id"] = json!("action");
    json!({"type":"input","block_id":id,"label":{"type":"plain_text","text":id},"element":element})
}
fn view(blocks: Vec<Value>) -> Value {
    json!({"id":"V1","type":"modal","title":{"type":"plain_text","text":"Test"},
    "submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn key(id: &str) -> InputKey {
    InputKey {
        block_id: id.into(),
        action_id: "action".into(),
    }
}
fn text(initial: &str) -> Value {
    input(
        "text",
        json!({"type":"plain_text_input","initial_value":initial}),
    )
}

#[test]
fn ordered_display_and_inputs_preserve_identity_and_raw_options() {
    let display = json!({"type":"section","text":{"type":"mrkdwn","text":"Instructions"}});
    let form = ModalForm::parse(&view(vec![
        display.clone(),
        text("Hello"),
        input(
            "select",
            json!({"type":"static_select",
        "options":[option("one"),option("two")],"initial_option":option("two")}),
        ),
    ]))
    .unwrap();
    assert_eq!(
        form.blocks,
        vec![
            FormBlock::Display(display),
            FormBlock::Input(key("text")),
            FormBlock::Input(key("select"))
        ]
    );
    assert_eq!(
        form.submission().unwrap(),
        json!({"values":{
            "text":{"action":{"type":"plain_text_input","value":"Hello"}},
            "select":{"action":{"type":"static_select","selected_option":option("two")}}
        }})
    );
}

#[test]
fn supported_controls_serialize_exact_state_keys_and_initial_values() {
    let form=ModalForm::parse(&view(vec![
        input("multi",json!({"type":"multi_static_select","options":[option("a"),option("b")],"initial_options":[option("b")]})),
        input("checks",json!({"type":"checkboxes","options":[option("a")],"initial_options":[option("a")]})),
        input("radio",json!({"type":"radio_buttons","options":[option("a")],"initial_option":option("a")})),
        input("date",json!({"type":"datepicker","initial_date":"2024-02-29"})),
        input("time",json!({"type":"timepicker","initial_time":"23:59","timezone":"America/New_York"})),
        input("dt",json!({"type":"datetimepicker","initial_date_time":1791896400})),
    ])).unwrap();
    let state = form.submission().unwrap();
    assert_eq!(
        state["values"]["multi"]["action"],
        json!({"type":"multi_static_select","selected_options":[option("b")]})
    );
    assert_eq!(
        state["values"]["checks"]["action"],
        json!({"type":"checkboxes","selected_options":[option("a")]})
    );
    assert_eq!(
        state["values"]["radio"]["action"],
        json!({"type":"radio_buttons","selected_option":option("a")})
    );
    assert_eq!(
        state["values"]["date"]["action"],
        json!({"type":"datepicker","selected_date":"2024-02-29"})
    );
    assert_eq!(
        state["values"]["time"]["action"],
        json!({"type":"timepicker","selected_time":"23:59","timezone":"America/New_York"})
    );
    assert_eq!(
        state["values"]["dt"]["action"],
        json!({"type":"datetimepicker","selected_date_time":1791896400})
    );
}

#[test]
fn optional_empty_inputs_preserve_explicit_nulls_and_empty_arrays() {
    let mut blocks = vec![
        text(""),
        input(
            "one",
            json!({"type":"static_select","options":[option("a")]}),
        ),
        input(
            "multi",
            json!({"type":"multi_static_select","options":[option("a")]}),
        ),
        input("date", json!({"type":"datepicker"})),
    ];
    for block in &mut blocks {
        block["optional"] = json!(true);
    }
    let form = ModalForm::parse(&view(blocks)).unwrap();
    let state = form.submission().unwrap();
    assert_eq!(state["values"]["text"]["action"]["value"], Value::Null);
    assert_eq!(
        state["values"]["one"]["action"]["selected_option"],
        Value::Null
    );
    assert_eq!(
        state["values"]["multi"]["action"]["selected_options"],
        json!([])
    );
    assert_eq!(
        state["values"]["date"]["action"]["selected_date"],
        Value::Null
    );
}

#[test]
fn required_lengths_utf16_and_invalid_dates_are_checked() {
    let mut form = ModalForm::parse(&view(vec![input(
        "text",
        json!({"type":"plain_text_input","min_length":2,"max_length":3}),
    )]))
    .unwrap();
    assert!(form.submission().is_err());
    form.set_value(&key("text"), InputValue::Text("\u{1f680}".into()))
        .unwrap();
    assert!(form.validate().is_empty());
    form.set_value(&key("text"), InputValue::Text("\u{1f680}\u{1f680}".into()))
        .unwrap();
    assert!(!form.validate().is_empty());
    assert_eq!(form.fields[&key("text")].revision, 0);
    for element in [
        json!({"type":"datepicker","initial_date":"2023-02-29"}),
        json!({"type":"timepicker","initial_time":"24:00"}),
        json!({"type":"timepicker","initial_time":"+1:00"}),
        json!({"type":"timepicker","initial_time":"00:+1"}),
        json!({"type":"datetimepicker","initial_date_time":i64::MAX}),
    ] {
        let form = ModalForm::parse(&view(vec![input("invalid", element)])).unwrap();
        assert!(form.submission().is_err());
    }
}

#[test]
fn choice_validation_rejects_unknown_duplicate_and_excess_selections() {
    let mut form=ModalForm::parse(&view(vec![input("select",json!({"type":"multi_static_select","options":[option("a"),option("b")],"max_selected_items":1}))])).unwrap();
    for values in [vec!["missing"], vec!["a", "a"], vec!["a", "b"]] {
        form.set_value(
            &key("select"),
            InputValue::Selected(values.into_iter().map(str::to_owned).collect()),
        )
        .unwrap();
        assert!(form.submission().is_err());
    }
    form.set_value(&key("select"), InputValue::Selected(vec!["a".into()]))
        .unwrap();
    assert!(form.submission().is_ok());
}

#[test]
fn unsupported_remote_controls_and_display_state_never_disappear_from_submission() {
    // Multi-channel inputs now own typed local state; unsupported remote types
    // below must still block the whole form instead of silently disappearing.
    let supported = ModalForm::parse(&view(vec![input(
        "channels",
        json!({"type":"multi_channels_select","initial_channels":["C12345678"]}),
    )]))
    .unwrap();
    assert!(matches!(
        supported.fields[&key("channels")].kind,
        InputKind::MultiChannel { .. }
    ));
    assert_eq!(
        supported.submission().unwrap()["values"]["channels"]["action"],
        json!({"type":"multi_channels_select","selected_channels":["C12345678"]})
    );
    for kind in ["rich_text_input", "file_input"] {
        let form = ModalForm::parse(&view(vec![input("remote", json!({"type":kind}))])).unwrap();
        assert!(matches!(
            form.fields[&key("remote")].kind,
            InputKind::Unsupported { .. }
        ));
        assert!(matches!(form.submission(), Err(FormError::Unsupported(_))));
    }
    let form = ModalForm::parse(&view(vec![
        json!({"type":"section","accessory":{"type":"static_select","action_id":"a"}}),
    ]))
    .unwrap();
    assert!(matches!(form.submission(), Err(FormError::Unsupported(_))));
    let form = ModalForm::parse(&view(vec![
        json!({"type":"actions","elements":[{"type":"checkboxes"}]}),
    ]))
    .unwrap();
    assert!(!form.unsupported.is_empty());
    // Captured JSK/ and AM6q classify this legacy spelling as stateful even
    // though it lacks the *_input suffix. No view.state is needed to detect it.
    let form = ModalForm::parse(&view(vec![
        json!({"type":"actions","elements":[{"type":"password","action_id":"secret"}]}),
    ]))
    .unwrap();
    assert!(matches!(form.submission(), Err(FormError::Unsupported(_))));
}

#[test]
fn dirty_reconcile_preserves_typing_and_revalidates_new_constraints() {
    let mut form = ModalForm::parse(&view(vec![text("server")])).unwrap();
    form.set_value(&key("text"), InputValue::Text("local typing".into()))
        .unwrap();
    let mut remote = view(vec![text("different server")]);
    remote["blocks"][0]["element"]["max_length"] = json!(5);
    form.reconcile(&remote).unwrap();
    assert_eq!(
        form.fields[&key("text")].value,
        InputValue::Text("local typing".into())
    );
    assert_eq!(form.fields[&key("text")].revision, 0);
    assert!(form.submission().is_err());
    let retained = form.clone();
    assert_eq!(
        form.reconcile(&view(vec![])),
        Err(FormError::ConflictingUpdate)
    );
    assert_eq!(form, retained);
    assert_eq!(
        form.reconcile(&view(vec![input("text", json!({"type":"datepicker"}))])),
        Err(FormError::ConflictingUpdate)
    );
    assert_eq!(form, retained);
}

#[test]
fn dirty_remote_reconcile_preserves_parse_errors_but_recomputes_constraints() {
    let mut form = ModalForm::parse(&view(vec![text("valid")])).unwrap();
    let field = form.fields.get_mut(&key("text")).unwrap();
    field.dirty = true;
    field.error = Some("Native input cannot be parsed".into());
    form.reconcile(&view(vec![text("remote")])).unwrap();
    assert_eq!(
        form.fields[&key("text")].error.as_deref(),
        Some("Native input cannot be parsed")
    );
    assert!(form.submission().is_err());
    let mut tightened = view(vec![text("remote")]);
    tightened["blocks"][0]["element"]["max_length"] = json!(2);
    form.reconcile(&tightened).unwrap();
    form.reconcile(&view(vec![text("remote")])).unwrap();
    assert_eq!(
        form.fields[&key("text")].error.as_deref(),
        Some("Native input cannot be parsed")
    );
    assert!(form.submission().is_err());
    form.set_value(&key("text"), InputValue::Text("fixed".into()))
        .unwrap();
    assert!(form.submission().is_ok());
    let mut remote = view(vec![text("remote")]);
    remote["blocks"][0]["element"]["max_length"] = json!(2);
    form.reconcile(&remote).unwrap();
    assert!(form.submission().is_err());
    form.reconcile(&view(vec![text("remote")])).unwrap();
    assert!(form.submission().is_ok());
}

#[test]
fn accepted_update_retains_current_values_and_allows_the_next_step() {
    let mut form = ModalForm::parse(&view(vec![
        text("initial"),
        input(
            "pristine",
            json!({"type":"plain_text_input","initial_value":"Pristine value"}),
        ),
        input(
            "removed",
            json!({"type":"plain_text_input","initial_value":"Remove after submit"}),
        ),
        input(
            "replaced",
            json!({"type":"plain_text_input","initial_value":"Replace after submit"}),
        ),
    ]))
    .unwrap();
    for name in ["text", "removed", "replaced"] {
        form.set_value(&key(name), InputValue::Text("Submitted value".into()))
            .unwrap();
    }
    form.fields.get_mut(&key("text")).unwrap().revision = 4;
    form.fields.get_mut(&key("replaced")).unwrap().revision = 7;
    let mut updated = view(vec![
        text("New default"),
        input(
            "pristine",
            json!({"type":"plain_text_input","initial_value":"Changed default"}),
        ),
        input(
            "replaced",
            json!({"type":"datepicker","initial_date":"2026-10-15"}),
        ),
        input(
            "added",
            json!({"type":"plain_text_input","initial_value":"New field"}),
        ),
    ]);
    updated["blocks"][0]["element"]["max_length"] = json!(5);
    form.reconcile_submitted(&updated).unwrap();
    assert_eq!(
        form.fields[&key("text")].value,
        InputValue::Text("Submitted value".into())
    );
    assert_eq!(form.fields[&key("text")].revision, 4);
    assert!(form.fields[&key("text")].dirty);
    assert!(form.fields[&key("text")].error.is_some());
    assert_eq!(
        form.fields[&key("pristine")].value,
        InputValue::Text("Pristine value".into())
    );
    assert!(!form.fields[&key("pristine")].dirty);
    assert!(!form.fields.contains_key(&key("removed")));
    assert_eq!(
        form.fields[&key("replaced")].value,
        InputValue::Date(Some("2026-10-15".into()))
    );
    assert_eq!(form.fields[&key("replaced")].revision, 8);
    assert_eq!(
        form.fields[&key("added")].value,
        InputValue::Text("New field".into())
    );
    assert_eq!(form.fields[&key("added")].revision, 0);
    let retained = form.clone();
    updated["id"] = json!("OTHER");
    assert_eq!(
        form.reconcile_submitted(&updated),
        Err(FormError::ConflictingUpdate)
    );
    assert_eq!(form, retained);
}

#[test]
fn pristine_remote_changes_reset_only_affected_field_and_server_errors_clear_on_edit() {
    let mut form = ModalForm::parse(&view(vec![
        text("old"),
        input(
            "other",
            json!({"type":"plain_text_input","initial_value":"same"}),
        ),
    ]))
    .unwrap();
    let mut remote = form.raw_view.clone();
    remote["blocks"][0]["element"]["initial_value"] = json!("new");
    form.reconcile(&remote).unwrap();
    assert_eq!(form.fields[&key("text")].revision, 1);
    assert_eq!(form.fields[&key("other")].revision, 0);
    form.apply_server_errors(&BTreeMap::from([("text".into(), "Server rejected".into())]));
    assert!(form.submission().is_err());
    form.set_value(&key("text"), InputValue::Text("fixed".into()))
        .unwrap();
    assert!(form.fields[&key("text")].error.is_none());
    assert!(form.submission().is_ok());
}

#[test]
fn saved_view_state_overrides_initial_but_malformed_or_unowned_state_is_not_dropped() {
    let mut raw = view(vec![text("initial")]);
    raw["state"] =
        json!({"values":{"text":{"action":{"type":"plain_text_input","value":"saved"}}}});
    let form = ModalForm::parse(&raw).unwrap();
    assert_eq!(
        form.fields[&key("text")].value,
        InputValue::Text("saved".into())
    );
    raw["state"]["values"]["unknown"] =
        json!({"action":{"type":"plain_text_input","value":"lost"}});
    assert!(ModalForm::parse(&raw).unwrap().submission().is_err());
    raw["state"] = json!({"values":[]});
    assert!(ModalForm::parse(&raw).is_err());
}

#[test]
fn ambiguous_identities_malformed_constraints_and_unmatched_initial_options_fail_closed() {
    for blocks in [
        vec![text("a"), text("b")],
        vec![input(
            "text",
            json!({"type":"plain_text_input","max_length":-1}),
        )],
        vec![input(
            "select",
            json!({"type":"static_select","options":[option("a")],"initial_option":option("b")}),
        )],
        vec![input(
            "select",
            json!({"type":"static_select","options":[option("a"),option("a")]}),
        )],
    ] {
        assert!(ModalForm::parse(&view(blocks)).is_err());
    }
    let mut raw = view(vec![text("a")]);
    raw["blocks"][0]["element"]["action_id"] = Value::Null;
    assert!(ModalForm::parse(&raw).is_err());
}

#[test]
fn datetime_html_helpers_roundtrip_and_reject_invalid_wall_clock() {
    let stamp = 1791896400;
    let rendered = datetime_local_value(Some(stamp));
    assert_eq!(parse_datetime_local(&rendered), Ok(Some(stamp)));
    assert_eq!(parse_datetime_local(""), Ok(None));
    assert!(parse_datetime_local("2023-02-29T10:00").is_err());
    assert!(parse_datetime_local("tomorrow").is_err());
    assert_eq!(datetime_local_value(None), "");
}
