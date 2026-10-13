use super::*;

fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Option &lt;{value}&gt;"),"emoji":true},
        "value":value,"description":{"type":"plain_text","text":"Details"},"metadata":{"kept":[1,null]}})
}
fn select(initial: Option<&str>) -> Value {
    let mut control = json!({"type":"static_select","action_id":"choose","placeholder":{"type":"plain_text","text":"Pick &amp; go"},"options":[option("a"),option("b"),option("")]});
    if let Some(value) = initial {
        control["initial_option"] = option(value);
    }
    control
}
fn message(blocks: Value) -> Message {
    serde_json::from_value(json!({"blocks":blocks})).unwrap()
}
fn actions(control: Value) -> Message {
    message(
        json!([{"type":"actions","block_id":"actions","elements":[control,
        {"type":"button","action_id":"go","text":{"type":"plain_text","text":"Go"}}]}]),
    )
}
fn input_key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "choose".into(),
    }
}

#[test]
fn lazy_initial_state_matches_section_type_only_and_actions_omission() {
    let message = message(json!([
        {"type":"section","block_id":"section","accessory":select(None)},
        {"type":"actions","block_id":"empty","elements":[select(None)]},
        {"type":"actions","block_id":"initial","elements":[select(Some("a"))]},
        {"type":"section","block_id":"button","accessory":{"type":"button","action_id":"go"}}
    ]));
    let mut inputs = MessageInputs::parse(&message).unwrap();
    assert_eq!(inputs.fields.len(), 3);
    assert_eq!(inputs.values().unwrap(), None);
    assert_eq!(
        inputs.fields[&input_key("section")].placeholder.as_deref(),
        Some("Pick & go")
    );
    let InputKind::StaticSelect { options, .. } = &inputs.fields[&input_key("section")].kind else {
        panic!("select");
    };
    assert_eq!(options[0].text, "Option <a>");
    assert_eq!(options[0].raw, option("a"));
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap(),
        Some(json!({
            "section":{"choose":{"type":"static_select"}},
            "initial":{"choose":{"type":"static_select","selected_option":option("a")}}
        }))
    );
    let mut only_button =
        MessageInputs::parse(&actions(json!({"type":"button","action_id":"other"}))).unwrap();
    only_button.initialize();
    assert_eq!(only_button.values().unwrap(), Some(json!({})));
}

#[test]
fn select_then_button_keeps_raw_metadata_and_distinguishes_clear_from_empty_value() {
    let mut inputs = MessageInputs::parse(&actions(select(Some("a")))).unwrap();
    let key = input_key("actions");
    let before = inputs.clone();
    assert_eq!(
        inputs.commit(&key, Some("absent")),
        Err(MessageStateError::InvalidSelection)
    );
    assert_eq!(inputs, before);
    assert_eq!(
        inputs.commit(&input_key("missing"), None),
        Err(MessageStateError::UnknownField)
    );
    let wire = inputs.commit(&key, Some("")).unwrap();
    assert_eq!(
        wire,
        json!({"type":"static_select","selected_option":option("")})
    );
    inputs.initialize(); // A subsequent button does not reset the committed choice.
    assert_eq!(inputs.values().unwrap().unwrap()["actions"]["choose"], wire);
    assert!(inputs.fields[&key].dirty);
    let wire = inputs.commit(&key, None).unwrap();
    assert_eq!(wire, json!({"type":"static_select","selected_option":null}));
    assert_eq!(inputs.values().unwrap().unwrap()["actions"]["choose"], wire);
}

#[test]
fn option_removal_resets_display_but_retains_wire_state_until_explicit_repair() {
    let mut inputs = MessageInputs::parse(&actions(select(None))).unwrap();
    let key = input_key("actions");
    let original = inputs.commit(&key, Some("b")).unwrap();
    let mut changed = select(None);
    changed["options"] = json!([option("a")]);
    inputs.reconcile(&actions(changed)).unwrap();
    assert_eq!(inputs.fields[&key].value, InputValue::Selected(vec![]));
    assert!(inputs.fields[&key].dirty);
    assert!(inputs.fields[&key].revision > 0);
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choose"],
        original
    );
    assert_eq!(
        inputs.commit(&key, Some("b")),
        Err(MessageStateError::InvalidSelection)
    );
    inputs.reconcile(&actions(select(None))).unwrap();
    assert_eq!(inputs.fields[&key].value, InputValue::Selected(vec![]));
    let repaired = inputs.commit(&key, Some("a")).unwrap();
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choose"],
        repaired
    );
}

#[test]
fn updates_preserve_selection_and_raw_state_but_initial_prop_resets_display() {
    let mut inputs = MessageInputs::parse(&actions(select(Some("a")))).unwrap();
    let key = input_key("actions");
    let original = inputs.commit(&key, Some("b")).unwrap();
    let mut changed = select(Some("a"));
    changed["options"][1]["metadata"] = json!({"new":true});
    inputs.reconcile(&actions(changed)).unwrap();
    assert_eq!(
        inputs.fields[&key].value,
        InputValue::Selected(vec!["b".into()])
    );
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choose"],
        original
    );
    inputs.reconcile(&actions(select(Some("")))).unwrap();
    assert_eq!(
        inputs.fields[&key].value,
        InputValue::Selected(vec!["".into()])
    );
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choose"],
        original
    );
    inputs
        .reconcile(&actions(json!({"type":"button","action_id":"choose"})))
        .unwrap();
    assert!(inputs.fields.is_empty());
    assert_eq!(inputs.values().unwrap(), Some(json!({})));
}

#[test]
fn grouped_options_and_explicit_null_initial_preserve_canonical_objects() {
    let mut control = select(None);
    control.as_object_mut().unwrap().remove("options");
    control["option_groups"] = json!([{"label":{"type":"plain_text","text":"Group &amp; one"},"options":[option("a"),option("b")]}]);
    control["initial_option"] = Value::Null;
    let mut inputs = MessageInputs::parse(&actions(control)).unwrap();
    let field = &inputs.fields[&input_key("actions")];
    let InputKind::StaticSelect { options, .. } = &field.kind else {
        panic!("select");
    };
    assert_eq!(options[0].group.as_deref(), Some("Group & one"));
    inputs.initialize();
    assert_eq!(inputs.values().unwrap(), Some(json!({})));
    assert_eq!(
        inputs.commit(&input_key("actions"), Some("b")).unwrap()["selected_option"],
        option("b")
    );
}

#[test]
fn duplicate_identity_and_options_are_rejected() {
    for blocks in [
        json!([{"type":"actions","block_id":"same","elements":[select(None),select(None)]}]),
        json!([{"type":"section","block_id":"same","accessory":select(None)}, {"type":"section","block_id":"same","accessory":select(None)}]),
        json!([{"type":"actions","elements":[select(None)]}]),
    ] {
        assert_eq!(
            MessageInputs::parse(&message(blocks)).unwrap_err(),
            MessageStateError::MalformedStructure
        );
    }
    let mut control = select(None);
    control["options"] = json!([option("a"), option("a")]);
    assert_eq!(
        MessageInputs::parse(&actions(control)).unwrap_err(),
        MessageStateError::MalformedStructure
    );
}

#[test]
fn unsupported_siblings_nested_controls_and_bad_updates_cannot_hide_state() {
    let mut inputs = MessageInputs::parse(&actions(select(None))).unwrap();
    inputs.commit(&input_key("actions"), Some("a")).unwrap();
    let before = inputs.clone();
    for block in [
        json!({"type":"actions","block_id":"unsupported","elements":[{"type":"multi_static_select","action_id":"x"}]}),
        json!({"type":"table","rows":[[{"type":"static_select","action_id":"nested"}]]}),
        json!({"type":"rich_text","elements":[{"type":"future_input"}]}),
        json!({"type":"input","block_id":"input","element":select(None)}),
        json!({"type":"condition","cases":[]}),
    ] {
        let mut updated = actions(select(None));
        updated.blocks.push(block);
        assert!(inputs.reconcile(&updated).is_err());
        assert_eq!(inputs, before);
    }
    let mut too_many = actions(select(None));
    too_many.blocks[0]["extra"] = json!(vec![Value::Null; 10_001]);
    assert_eq!(
        MessageInputs::parse(&too_many).unwrap_err(),
        MessageStateError::ExcessiveStructure
    );
}

fn radio(initial: Option<&str>) -> Value {
    let mut value = select(initial);
    value["type"] = json!("radio_buttons");
    value.as_object_mut().unwrap().remove("placeholder");
    value
}

#[test]
fn radio_initial_state_matches_section_omission_and_mixed_button_state() {
    let mut no_initial = radio(None);
    no_initial["initial_option"] = Value::Null;
    let message = message(json!([
        {"type":"section","block_id":"section","accessory":no_initial},
        {"type":"actions","block_id":"absent","elements":[radio(None)]},
        {"type":"actions","block_id":"radio","elements":[radio(Some("a"))]},
        {"type":"actions","block_id":"select","elements":[select(Some("b"))]}
    ]));
    let mut inputs = MessageInputs::parse(&message).unwrap();
    assert_eq!(inputs.values().unwrap(), None);
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap(),
        Some(json!({
            "section":{"choose":{"type":"radio_buttons"}},
            "radio":{"choose":{"type":"radio_buttons","selected_option":option("a")}},
            "select":{"choose":{"type":"static_select","selected_option":option("b")}}
        }))
    );
    let selected = inputs.commit(&input_key("radio"), Some("")).unwrap();
    assert_eq!(
        selected,
        json!({"type":"radio_buttons","selected_option":option("")})
    );
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap().unwrap()["radio"]["choose"],
        selected
    );
    assert_eq!(
        inputs.commit(&input_key("radio"), None).unwrap(),
        json!({"type":"radio_buttons","selected_option":null})
    );
}

#[test]
fn radio_keeps_saved_display_and_wire_through_initial_and_option_updates() {
    let mut inputs = MessageInputs::parse(&actions(radio(Some("a")))).unwrap();
    let key = input_key("actions");
    let committed = inputs.commit(&key, Some("b")).unwrap();
    let mut changed = radio(Some(""));
    changed["options"] = json!([option("a"), option("")]);
    inputs.reconcile(&actions(changed)).unwrap();
    assert_eq!(
        inputs.fields[&key].value,
        InputValue::Selected(vec!["b".into()])
    );
    assert_eq!(inputs.fields[&key].revision, 0);
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choose"],
        committed
    );
    let before = inputs.clone();
    assert_eq!(
        inputs.commit(&key, Some("b")),
        Err(MessageStateError::InvalidSelection)
    );
    assert_eq!(inputs, before);
    inputs.reconcile(&actions(radio(Some("a")))).unwrap();
    assert_eq!(
        inputs.fields[&key].value,
        InputValue::Selected(vec!["b".into()])
    );
    let mut changed = radio(Some("a"));
    changed["options"][1]["description"]["text"] = json!("New description");
    inputs.reconcile(&actions(changed.clone())).unwrap();
    assert_eq!(
        inputs.commit(&key, Some("b")).unwrap()["selected_option"],
        changed["options"][1]
    );
    inputs
        .reconcile(&actions(json!({"type":"button","action_id":"choose"})))
        .unwrap();
    assert!(inputs.fields.is_empty());
    assert_eq!(inputs.values().unwrap(), Some(json!({})));
}

#[test]
fn radio_pristine_display_does_not_follow_changed_initial_option() {
    let mut inputs = MessageInputs::parse(&actions(radio(Some("a")))).unwrap();
    inputs.reconcile(&actions(radio(Some("b")))).unwrap();
    let field = &inputs.fields[&input_key("actions")];
    assert_eq!(field.value, InputValue::Selected(vec!["a".into()]));
    assert!(!field.dirty);
    // The first interaction still initializes from the newest canonical props,
    // independently of the mounted radio's saved display value.
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choose"]["selected_option"],
        option("b")
    );
}

#[test]
fn radio_compositions_preserve_raw_metadata_and_reject_malformed_or_unowned_state() {
    let mut control = radio(None);
    control["options"][0]["text"] = json!({"type":"mrkdwn","text":"*Choice* &amp; one"});
    control["options"][0]["description"] =
        json!({"type":"mrkdwn","text":"<https://example.test|Details>"});
    let mut inputs = MessageInputs::parse(&actions(control.clone())).unwrap();
    let InputKind::Radio { options } = &inputs.fields[&input_key("actions")].kind else {
        panic!("radio");
    };
    assert_eq!(options[0].text, "*Choice* & one");
    assert_eq!(
        inputs.commit(&input_key("actions"), Some("a")).unwrap()["selected_option"],
        control["options"][0]
    );
    for case in 0..5 {
        let mut malformed = radio(None);
        match case {
            0 => malformed["option_groups"] = json!([]),
            1 => malformed["options"] = json!([option("a"), option("a")]),
            2 => malformed["options"][0]["text"]["type"] = json!("html"),
            3 => malformed["initial_option"] = option("missing"),
            4 => malformed["options"][0]["description"] = Value::Null,
            _ => unreachable!(),
        }
        assert!(MessageInputs::parse(&actions(malformed)).is_err());
    }
    for extra in [
        json!({"type":"checkboxes","action_id":"checks"}),
        json!({"type":"datepicker","action_id":"date"}),
    ] {
        let mut mixed = actions(radio(None));
        mixed.blocks[0]["elements"]
            .as_array_mut()
            .unwrap()
            .push(extra);
        assert!(MessageInputs::parse(&mixed).is_err());
    }
    let nested = message(json!([{"type":"table","rows":[[radio(None)]]}]));
    assert!(MessageInputs::parse(&nested).is_err());
}

#[test]
fn control_type_replacement_mounts_from_retained_wire_instead_of_reset_display() {
    let key = input_key("actions");
    let mut inputs = MessageInputs::parse(&actions(select(None))).unwrap();
    let committed = inputs.commit(&key, Some("b")).unwrap();
    let mut reduced = select(None);
    reduced["options"] = json!([option("a")]);
    inputs.reconcile(&actions(reduced)).unwrap();
    assert_eq!(inputs.fields[&key].value, InputValue::Selected(vec![]));
    let revision = inputs.fields[&key].revision;
    inputs.reconcile(&actions(radio(Some("a")))).unwrap();
    assert_eq!(
        inputs.fields[&key].value,
        InputValue::Selected(vec!["b".into()])
    );
    assert!(inputs.fields[&key].revision > revision);
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choose"],
        committed
    );
    // The old static-select wire remains until this radio is actually used.
    assert_eq!(
        inputs.commit(&key, Some("a")).unwrap()["type"],
        "radio_buttons"
    );
}

#[test]
fn replacement_without_saved_option_uses_new_initial_not_old_radio_display() {
    let key = input_key("actions");
    for initialize in [false, true] {
        let mut inputs = MessageInputs::parse(&actions(radio(Some("a")))).unwrap();
        inputs.reconcile(&actions(radio(Some("b")))).unwrap();
        assert_eq!(
            inputs.fields[&key].value,
            InputValue::Selected(vec!["a".into()])
        );
        if initialize {
            inputs.initialize();
        }
        let before = inputs.values().unwrap();
        // No prior interaction uses the new initial C; initialized wire B wins.
        let mut replacement = select(None);
        replacement["options"]
            .as_array_mut()
            .unwrap()
            .push(option("c"));
        replacement["initial_option"] = option("c");
        inputs.reconcile(&actions(replacement)).unwrap();
        assert_eq!(
            inputs.fields[&key].value,
            InputValue::Selected(vec![if initialize { "b" } else { "c" }.into()])
        );
        assert_eq!(inputs.values().unwrap(), before);
    }
}

#[test]
fn replacement_constructor_distinguishes_empty_saved_value_from_null() {
    let key = input_key("actions");
    for from_radio in [false, true] {
        for selected in [None, Some("")] {
            let control = if from_radio {
                radio(Some("a"))
            } else {
                select(Some("a"))
            };
            let mut inputs = MessageInputs::parse(&actions(control)).unwrap();
            let committed = inputs.commit(&key, selected).unwrap();
            let replacement = if from_radio {
                select(Some("b"))
            } else {
                radio(Some("b"))
            };
            inputs.reconcile(&actions(replacement)).unwrap();
            let expected = if from_radio && selected == Some("") {
                ""
            } else {
                "b"
            };
            assert_eq!(
                inputs.fields[&key].value,
                InputValue::Selected(vec![expected.into()])
            );
            assert_eq!(
                inputs.values().unwrap().unwrap()["actions"]["choose"],
                committed
            );
        }
    }
}
