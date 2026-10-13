use super::*;
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "user".into(),
    }
}
fn message() -> Message {
    serde_json::from_value(json!({"blocks":[
 {"type":"section","block_id":"section","accessory":{"type":"conversations_select","action_id":"user"}},
 {"type":"actions","block_id":"actions","elements":[{"type":"conversations_select","action_id":"user"},{"type":"static_select","action_id":"choice","initial_option":{"value":"a","text":{"type":"plain_text","text":"A"}},"options":[{"value":"a","text":{"type":"plain_text","text":"A"}}]}]}
]})).unwrap()
}
#[test]
fn lazy_user_seed_distinguishes_section_actions_and_retains_sibling_raw_state() {
    let message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    assert_eq!(inputs.values().unwrap(), None);
    inputs.initialize();
    let values = inputs.values().unwrap().unwrap();
    assert_eq!(
        values["section"]["user"],
        json!({"type":"conversations_select"})
    );
    assert!(values["actions"].get("user").is_none());
    let sibling = values["actions"]["choice"].clone();
    inputs.commit(&key("actions"), Some("C00000002")).unwrap();
    assert_eq!(
        inputs.values().unwrap().unwrap()["actions"]["user"],
        json!({"type":"conversations_select","selected_conversation":"C00000002"})
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
        inputs.values().unwrap().unwrap()["actions"]["user"]["selected_conversation"],
        Value::Null
    );
}
#[test]
fn changed_initial_resets_display_but_retains_shared_user_until_removed_identity() {
    let mut message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    inputs.commit(&key("section"), Some("U00000001")).unwrap();
    let wire = inputs.values().unwrap();
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("section")].value,
        InputValue::Conversation(Some("U00000001".into()))
    );
    message.blocks[0]["accessory"]["initial_conversation"] = json!("G00000003");
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("section")].value,
        InputValue::Conversation(Some("G00000003".into()))
    );
    assert_eq!(inputs.values().unwrap(), wire);
    message.blocks[0]["accessory"]
        .as_object_mut()
        .unwrap()
        .remove("initial_conversation");
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("section")].value,
        InputValue::Conversation(None)
    );
    assert_eq!(inputs.values().unwrap(), wire);
    message.blocks.remove(0);
    inputs.reconcile(&message).unwrap();
    assert!(inputs.values().unwrap().unwrap().get("section").is_none());
}
#[test]
fn replacing_user_with_choice_does_not_cross_contaminate_value_types() {
    let mut message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    inputs.commit(&key("actions"), Some("U00000001")).unwrap();
    let old = inputs.values().unwrap().unwrap()["actions"]["user"].clone();
    let mut choice = message.blocks[1]["elements"][1].clone();
    choice["action_id"] = json!("user");
    message.blocks[1]["elements"][0] = choice;
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("actions")].value,
        InputValue::Selected(vec!["a".into()])
    );
    assert_eq!(inputs.values().unwrap().unwrap()["actions"]["user"], old);
    message.blocks[1]["elements"][0] = json!({"type":"conversations_select","action_id":"user","initial_conversation":"U00000004"});
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("actions")].value,
        InputValue::Conversation(Some("U00000001".into()))
    );
}
