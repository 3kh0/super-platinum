use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "B".into(),
        action_id: "channels".into(),
    }
}
fn message(initial: Option<Value>) -> Message {
    let mut channels =
        json!({"type":"multi_channels_select","action_id":"channels","max_selected_items":2});
    if let Some(initial) = initial {
        channels["initial_channels"] = initial;
    }
    serde_json::from_value(
        json!({"blocks":[{"type":"actions","block_id":"B","elements":[channels,
        {"type":"button","action_id":"go","text":{"type":"plain_text","text":"Go"}},
        {"type":"channels_select","action_id":"single","initial_channel":"C00000003"}]}]}),
    )
    .unwrap()
}

#[test]
fn sibling_button_initialization_includes_present_initial_channels_only() {
    for initial in [None, Some(json!([])), Some(json!(["C00000001"]))] {
        let mut inputs = MessageInputs::parse(&message(initial.clone())).unwrap();
        assert!(inputs.values().unwrap().is_none());
        inputs.initialize();
        let state = inputs.values().unwrap().unwrap();
        assert_eq!(state["B"]["single"]["selected_channel"], "C00000003");
        match initial {
            None => assert!(state["B"].get("channels").is_none()),
            Some(initial) => assert_eq!(state["B"]["channels"]["selected_channels"], initial),
        }
    }
}

#[test]
fn staged_selection_is_pure_and_commits_are_transactional_with_full_siblings() {
    let mut inputs = MessageInputs::parse(&message(Some(json!(["C00000001"])))).unwrap();
    let untouched = inputs.clone();
    assert!(inputs.commit_channels(&key(), &["invalid".into()]).is_err());
    assert_eq!(inputs, untouched);
    let selected = vec!["C00000002".into(), "G00000004".into()];
    inputs.commit_channels(&key(), &selected).unwrap();
    assert_eq!(inputs.saved_channels(&key()), Some(selected.clone()));
    let state = inputs.values().unwrap().unwrap();
    assert_eq!(state["B"]["channels"]["selected_channels"], json!(selected));
    assert_eq!(state["B"]["single"]["selected_channel"], "C00000003");
    inputs.commit_channels(&key(), &[]).unwrap();
    assert_eq!(inputs.saved_channels(&key()), Some(vec![]));
    assert_eq!(inputs.fields[&key()].value, InputValue::Channels(vec![]));
}

#[test]
fn message_refresh_retains_wire_selection_and_resets_changed_initial_display() {
    let original = message(Some(json!(["C00000001"])));
    let mut inputs = MessageInputs::parse(&original).unwrap();
    inputs
        .commit_channels(&key(), &["C00000002".into()])
        .unwrap();
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Channels(vec!["C00000002".into()])
    );
    let updated = message(Some(json!([])));
    inputs.reconcile(&updated).unwrap();
    assert_eq!(inputs.fields[&key()].value, InputValue::Channels(vec![]));
    assert_eq!(
        inputs.saved_channels(&key()),
        Some(vec!["C00000002".into()])
    );
    let mut removed = updated;
    removed.blocks[0]["elements"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    inputs.reconcile(&removed).unwrap();
    assert!(inputs.saved_channels(&key()).is_none());
}

#[test]
fn reduced_maximum_preserves_initial_and_saved_channels_until_corrected() {
    let three = vec!["C00000001".into(), "C00000002".into(), "C00000003".into()];
    for initial in [json!(["C00000001"]), json!(three)] {
        let mut original = message(Some(initial));
        original.blocks[0]["elements"][0]["max_selected_items"] = json!(3);
        let mut inputs = MessageInputs::parse(&original).unwrap();
        inputs.commit_channels(&key(), &three).unwrap();
        let before = inputs.values().unwrap();
        let mut changed = original;
        changed.blocks[0]["elements"][0]["max_selected_items"] = json!(2);
        inputs.reconcile(&changed).unwrap();
        assert_eq!(
            inputs.fields[&key()].value,
            InputValue::Channels(three.clone())
        );
        assert_eq!(inputs.saved_channels(&key()), Some(three.clone()));
        assert_eq!(
            inputs.values().unwrap(),
            before,
            "sibling actions retain untouched wire state"
        );
        assert!(
            inputs.commit_channels(&key(), &three).is_err(),
            "new submissions enforce the reduced max"
        );
        inputs.commit_channels(&key(), &three[..2]).unwrap();
        assert_eq!(inputs.saved_channels(&key()), Some(three[..2].to_vec()));
    }
    // An already over-limit initial array still renders before any interaction.
    let mut inputs = MessageInputs::parse(&message(Some(json!(three)))).unwrap();
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap().unwrap()["B"]["channels"]["selected_channels"],
        json!(three)
    );
    assert!(inputs.commit_channels(&key(), &three).is_err());
    inputs.commit_channels(&key(), &three[..2]).unwrap();
}

#[test]
fn multi_channel_and_single_channel_display_do_not_consume_each_others_saved_property() {
    let original = message(Some(json!(["C00000001"])));
    let mut inputs = MessageInputs::parse(&original).unwrap();
    inputs.commit_channels(&key(), &[]).unwrap();
    let mut single = original.clone();
    single.blocks[0]["elements"][0] =
        json!({"type":"channels_select","action_id":"channels","initial_channel":"C00000002"});
    inputs.reconcile(&single).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Channel(Some("C00000002".into()))
    );
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Channels(vec![]),
        "remount consumes saved empty array"
    );
    inputs.reconcile(&single).unwrap();
    inputs.commit(&key(), Some("C00000003")).unwrap();
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Channels(vec!["C00000001".into()])
    );
}
