use super::*;
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Choice {value}")},"value":value,"custom":{"retain":true}})
}
fn control(kind: &str, initial: Option<Value>) -> Value {
    let mut value =
        json!({"type":kind,"action_id":"choose","options":[option("a"),option("b"),option("")]});
    if let Some(initial) = initial {
        value["initial_options"] = initial;
    }
    value
}
fn message(blocks: Value) -> Message {
    serde_json::from_value(json!({"blocks":blocks})).unwrap()
}
fn actions(control: Value) -> Message {
    message(json!([{"type":"actions","block_id":"B","elements":[control]}]))
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "choose".into(),
    }
}
#[test]
fn multi_initial_absent_empty_and_populated_keep_section_actions_difference() {
    for kind in ["checkboxes", "multi_static_select"] {
        let message = message(json!([
            {"type":"section","block_id":"section","accessory":control(kind,None)},
            {"type":"actions","block_id":"absent","elements":[control(kind,None)]},
            {"type":"actions","block_id":"empty","elements":[control(kind,Some(json!([])))]},
            {"type":"actions","block_id":"populated","elements":[control(kind,Some(json!([option("")])))]}
        ]));
        let mut model = MessageInputs::parse(&message).unwrap();
        assert_eq!(model.values().unwrap(), None);
        model.initialize();
        let values = model.values().unwrap().unwrap();
        assert_eq!(values["section"]["choose"], json!({"type":kind}));
        assert!(values.get("absent").is_none());
        assert_eq!(
            values["empty"]["choose"],
            json!({"type":kind,"selected_options":[]})
        );
        assert_eq!(model.saved_options(&key("empty")), Some(&[][..]));
        assert_eq!(
            values["populated"]["choose"]["selected_options"],
            json!([option("")])
        );
    }
}
#[test]
fn multi_commit_is_transactional_preserves_siblings_raw_order_and_clear() {
    let mut model=MessageInputs::parse(&message(json!([
        {"type":"actions","block_id":"B","elements":[control("checkboxes",Some(json!([option("a")])))]},
        {"type":"section","block_id":"sibling","accessory":control("multi_static_select",Some(json!([option("b")])))}
    ]))).unwrap();
    let before = model.clone();
    assert!(
        model
            .commit_multi(&key("B"), &[option("wrong")], &[])
            .is_err()
    );
    assert_eq!(model, before);
    let raw = vec![option("b"), option("")];
    model.commit_multi(&key("B"), &raw, &[]).unwrap();
    let values = model.values().unwrap().unwrap();
    assert_eq!(values["B"]["choose"]["selected_options"], json!(raw));
    assert_eq!(
        values["sibling"]["choose"]["selected_options"],
        json!([option("b")])
    );
    model.commit_multi(&key("B"), &[], &raw).unwrap();
    assert_eq!(model.saved_options(&key("B")), Some(&[][..]));
}
#[test]
fn multi_display_reconciliation_does_not_overwrite_retained_wire_state() {
    for kind in ["checkboxes", "multi_static_select"] {
        let mut model =
            MessageInputs::parse(&actions(control(kind, Some(json!([option("a")]))))).unwrap();
        model.commit_multi(&key("B"), &[option("b")], &[]).unwrap();
        let mut updated = control(kind, Some(json!([])));
        updated["options"] = json!([option("a")]);
        model.reconcile(&actions(updated)).unwrap();
        assert_eq!(model.saved_options(&key("B")), Some(&[option("b")][..]));
        assert_eq!(
            model.fields[&key("B")].value,
            InputValue::Selected(if kind == "checkboxes" {
                vec!["b".into()]
            } else {
                vec![]
            })
        );
        // Removing initial_options or changing options alone never resets etBz.
        let mut updated = control(kind, None);
        updated["options"] = json!([option("a")]);
        let before = model.fields[&key("B")].value.clone();
        model.reconcile(&actions(updated)).unwrap();
        assert_eq!(model.fields[&key("B")].value, before);
        model
            .commit_multi(&key("B"), &[option("b"), option("a")], &[option("b")])
            .unwrap();
        assert_eq!(
            model.saved_options(&key("B")).unwrap(),
            &[option("b"), option("a")]
        );
    }
}
#[test]
fn removed_multi_identity_prunes_state_and_unsupported_siblings_still_block() {
    let mut model = MessageInputs::parse(&actions(control("checkboxes", None))).unwrap();
    model.commit_multi(&key("B"), &[option("a")], &[]).unwrap();
    let before = model.clone();
    let bad = actions(json!({"type":"external_select","action_id":"choose"}));
    assert!(model.reconcile(&bad).is_err());
    assert_eq!(model, before);
    model
        .reconcile(&actions(json!({"type":"button","action_id":"choose"})))
        .unwrap();
    assert_eq!(model.values().unwrap(), Some(json!({})));
}

#[test]
fn untouched_multi_apply_commits_type_only_and_preserves_siblings_then_clear_array() {
    let message = message(json!([
        {"type":"actions","block_id":"B","elements":[control("multi_static_select",None)]},
        {"type":"section","block_id":"other","accessory":control("checkboxes",Some(json!([option("a")]))) }
    ]));
    let mut model = MessageInputs::parse(&message).unwrap();
    let committed = model.commit_multi_optional(&key("B"), None, &[]).unwrap();
    assert_eq!(committed, json!({"type":"multi_static_select"}));
    let values = model.values().unwrap().unwrap();
    assert_eq!(values["B"]["choose"], committed);
    assert_eq!(
        values["other"]["choose"]["selected_options"],
        json!([option("a")])
    );
    model
        .commit_multi_optional(&key("B"), Some(&[]), &[])
        .unwrap();
    assert_eq!(
        model.values().unwrap().unwrap()["B"]["choose"]["selected_options"],
        json!([])
    );
}
