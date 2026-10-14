use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "B".into(),
        action_id: "conversations".into(),
    }
}
fn message(initial: Option<Value>) -> Message {
    let mut conversations = json!({"type":"multi_conversations_select","action_id":"conversations","max_selected_items":2});
    if let Some(initial) = initial {
        conversations["initial_conversations"] = initial;
    }
    serde_json::from_value(
        json!({"blocks":[{"type":"actions","block_id":"B","elements":[conversations,
        {"type":"button","action_id":"go","text":{"type":"plain_text","text":"Go"}},
        {"type":"conversations_select","action_id":"single","initial_conversation":"C00000003"}]}]}),
    )
    .unwrap()
}

#[test]
fn sibling_button_initialization_includes_present_initial_conversations_only() {
    for initial in [None, Some(json!([])), Some(json!(["C00000001"]))] {
        let mut inputs = MessageInputs::parse(&message(initial.clone())).unwrap();
        assert!(inputs.values().unwrap().is_none());
        inputs.initialize();
        let state = inputs.values().unwrap().unwrap();
        assert_eq!(state["B"]["single"]["selected_conversation"], "C00000003");
        match initial {
            None => assert!(state["B"].get("conversations").is_none()),
            Some(initial) => assert_eq!(
                state["B"]["conversations"]["selected_conversations"],
                initial
            ),
        }
    }
}

#[test]
fn staged_selection_is_pure_and_commits_are_transactional_with_full_siblings() {
    let mut inputs = MessageInputs::parse(&message(Some(json!(["C00000001"])))).unwrap();
    let untouched = inputs.clone();
    assert!(
        inputs
            .commit_conversations(&key(), &["invalid".into()])
            .is_err()
    );
    assert_eq!(inputs, untouched);
    let selected = vec!["C00000002".into(), "G00000004".into()];
    inputs.commit_conversations(&key(), &selected).unwrap();
    assert_eq!(inputs.saved_conversations(&key()), Some(selected.clone()));
    let state = inputs.values().unwrap().unwrap();
    assert_eq!(
        state["B"]["conversations"]["selected_conversations"],
        json!(selected)
    );
    assert_eq!(state["B"]["single"]["selected_conversation"], "C00000003");
    inputs.commit_conversations(&key(), &[]).unwrap();
    assert_eq!(inputs.saved_conversations(&key()), Some(vec![]));
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(vec![])
    );
}

#[test]
fn message_refresh_retains_wire_selection_and_resets_changed_initial_display() {
    let original = message(Some(json!(["C00000001"])));
    let mut inputs = MessageInputs::parse(&original).unwrap();
    inputs
        .commit_conversations(&key(), &["C00000002".into()])
        .unwrap();
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(vec!["C00000002".into()])
    );
    let updated = message(Some(json!([])));
    inputs.reconcile(&updated).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(vec![])
    );
    assert_eq!(
        inputs.saved_conversations(&key()),
        Some(vec!["C00000002".into()])
    );
    let mut removed = updated;
    removed.blocks[0]["elements"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    inputs.reconcile(&removed).unwrap();
    assert!(inputs.saved_conversations(&key()).is_none());
}

#[test]
fn reduced_maximum_preserves_initial_and_saved_conversations_until_corrected() {
    let three = vec!["C00000001".into(), "C00000002".into(), "C00000003".into()];
    for initial in [json!(["C00000001"]), json!(three)] {
        let mut original = message(Some(initial));
        original.blocks[0]["elements"][0]["max_selected_items"] = json!(3);
        let mut inputs = MessageInputs::parse(&original).unwrap();
        inputs.commit_conversations(&key(), &three).unwrap();
        let before = inputs.values().unwrap();
        let mut changed = original;
        changed.blocks[0]["elements"][0]["max_selected_items"] = json!(2);
        inputs.reconcile(&changed).unwrap();
        assert_eq!(
            inputs.fields[&key()].value,
            InputValue::Conversations(three.clone())
        );
        assert_eq!(inputs.saved_conversations(&key()), Some(three.clone()));
        assert_eq!(
            inputs.values().unwrap(),
            before,
            "sibling actions retain untouched wire state"
        );
        assert!(
            inputs.commit_conversations(&key(), &three).is_err(),
            "new submissions enforce the reduced max"
        );
        inputs.commit_conversations(&key(), &three[..2]).unwrap();
        assert_eq!(
            inputs.saved_conversations(&key()),
            Some(three[..2].to_vec())
        );
    }
    // An already over-limit initial array still renders before any interaction.
    let mut inputs = MessageInputs::parse(&message(Some(json!(three)))).unwrap();
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap().unwrap()["B"]["conversations"]["selected_conversations"],
        json!(three)
    );
    assert!(inputs.commit_conversations(&key(), &three).is_err());
    inputs.commit_conversations(&key(), &three[..2]).unwrap();
}

#[test]
fn multi_conversation_and_single_conversation_display_do_not_consume_each_others_saved_property() {
    let original = message(Some(json!(["C00000001"])));
    let mut inputs = MessageInputs::parse(&original).unwrap();
    inputs.commit_conversations(&key(), &[]).unwrap();
    let mut single = original.clone();
    single.blocks[0]["elements"][0] = json!({"type":"conversations_select","action_id":"conversations","initial_conversation":"C00000002"});
    inputs.reconcile(&single).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversation(Some("C00000002".into()))
    );
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(vec![]),
        "remount consumes saved empty array"
    );
    inputs.reconcile(&single).unwrap();
    inputs.commit(&key(), Some("C00000003")).unwrap();
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(vec!["C00000001".into()])
    );
}

#[test]
fn removing_initial_array_preserves_mounted_value_but_changed_empty_array_resets() {
    let mut original = message(Some(json!(["C00000001"])));
    let mut inputs = MessageInputs::parse(&original).unwrap();
    let selected = vec!["U00000002".into(), "D00000003".into()];
    inputs.commit_conversations(&key(), &selected).unwrap();
    let wire = inputs.values().unwrap();
    original.blocks[0]["elements"][0]
        .as_object_mut()
        .unwrap()
        .remove("initial_conversations");
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(selected.clone())
    );
    original.blocks[0]["elements"][0]["initial_conversations"] = Value::Null;
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(selected.clone())
    );
    assert_eq!(inputs.values().unwrap(), wire);
    original.blocks[0]["elements"][0]["initial_conversations"] = json!([]);
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Conversations(vec![])
    );
    assert_eq!(inputs.saved_conversations(&key()), Some(selected));
    assert_eq!(inputs.values().unwrap(), wire);
}
