use super::*;
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "date".into(),
    }
}
fn message() -> Message {
    serde_json::from_value(json!({"blocks":[
 {"type":"section","block_id":"section","accessory":{"type":"datepicker","action_id":"date"}},
 {"type":"actions","block_id":"actions","elements":[{"type":"datepicker","action_id":"date"},{"type":"static_select","action_id":"choice","initial_option":{"value":"a","text":{"type":"plain_text","text":"A"}},"options":[{"value":"a","text":{"type":"plain_text","text":"A"}}]}]}
]})).unwrap()
}
#[test]
fn lazy_date_seed_distinguishes_section_actions_and_retains_sibling_raw_state() {
    let message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    assert_eq!(inputs.values().unwrap(), None);
    inputs.initialize();
    let values = inputs.values().unwrap().unwrap();
    assert_eq!(values["section"]["date"], json!({"type":"datepicker"}));
    assert!(values["actions"].get("date").is_none());
    let sibling = values["actions"]["choice"].clone();
    inputs.commit(&key("actions"), Some("2028-02-29")).unwrap();
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["date"],
        json!({"type":"datepicker","selected_date":"2028-02-29"})
    );
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["choice"],
        sibling
    );
    let before = inputs.clone();
    assert!(inputs.commit(&key("actions"), Some("2027-02-29")).is_err());
    assert_eq!(inputs, before);
    inputs.commit(&key("actions"), None).unwrap();
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["date"]["selected_date"],
        Value::Null
    );
}
#[test]
fn changed_initial_resets_display_but_retains_shared_date_until_removed_identity() {
    let mut message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    inputs.commit(&key("section"), Some("2026-10-13")).unwrap();
    let wire = inputs.values().unwrap();
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("section")].value,
        InputValue::Date(Some("2026-10-13".into()))
    );
    message.blocks[0]["accessory"]["initial_date"] = json!("2026-11-01");
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("section")].value,
        InputValue::Date(Some("2026-11-01".into()))
    );
    assert_eq!(inputs.values().unwrap(), wire);
    message.blocks[0]["accessory"]
        .as_object_mut()
        .unwrap()
        .remove("initial_date");
    inputs.reconcile(&message).unwrap();
    assert_eq!(inputs.fields[&key("section")].value, InputValue::Date(None));
    assert_eq!(inputs.values().unwrap(), wire);
    message.blocks.remove(0);
    inputs.reconcile(&message).unwrap();
    assert!(inputs.values().unwrap().unwrap().get("section").is_none());
}
#[test]
fn replacing_date_with_choice_does_not_cross_contaminate_value_types() {
    let mut message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    inputs.commit(&key("actions"), Some("2026-10-13")).unwrap();
    let old = inputs.values().unwrap().unwrap()["actions"]["date"].clone();
    let mut choice = message.blocks[1]["elements"][1].clone();
    choice["action_id"] = json!("date");
    message.blocks[1]["elements"][0] = choice;
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("actions")].value,
        InputValue::Selected(vec!["a".into()])
    );
    assert_eq!(inputs.values().unwrap().unwrap()["actions"]["date"], old);
    message.blocks[1]["elements"][0] =
        json!({"type":"datepicker","action_id":"date","initial_date":"2026-12-01"});
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("actions")].value,
        InputValue::Date(Some("2026-10-13".into()))
    );
}
