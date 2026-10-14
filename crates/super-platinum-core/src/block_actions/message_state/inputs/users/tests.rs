use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "B".into(),
        action_id: "users".into(),
    }
}
fn message(initial: Option<Value>) -> Message {
    let mut users = json!({"type":"multi_users_select","action_id":"users","max_selected_items":2});
    if let Some(initial) = initial {
        users["initial_users"] = initial;
    }
    serde_json::from_value(
        json!({"blocks":[{"type":"actions","block_id":"B","elements":[users,
        {"type":"button","action_id":"go","text":{"type":"plain_text","text":"Go"}},
        {"type":"users_select","action_id":"single","initial_user":"U00000003"}]}]}),
    )
    .unwrap()
}

#[test]
fn sibling_button_initialization_includes_present_initial_users_only() {
    for initial in [None, Some(json!([])), Some(json!(["U00000001"]))] {
        let mut inputs = MessageInputs::parse(&message(initial.clone())).unwrap();
        assert!(inputs.values().unwrap().is_none());
        inputs.initialize();
        let state = inputs.values().unwrap().unwrap();
        assert_eq!(state["B"]["single"]["selected_user"], "U00000003");
        match initial {
            None => assert!(state["B"].get("users").is_none()),
            Some(initial) => assert_eq!(state["B"]["users"]["selected_users"], initial),
        }
    }
}

#[test]
fn staged_selection_is_pure_and_commits_are_transactional_with_full_siblings() {
    let mut inputs = MessageInputs::parse(&message(Some(json!(["U00000001"])))).unwrap();
    let untouched = inputs.clone();
    assert!(inputs.commit_users(&key(), &["invalid".into()]).is_err());
    assert_eq!(inputs, untouched);
    let selected = vec!["U00000002".into(), "W00000004".into()];
    inputs.commit_users(&key(), &selected).unwrap();
    assert_eq!(inputs.saved_users(&key()), Some(selected.clone()));
    let state = inputs.values().unwrap().unwrap();
    assert_eq!(state["B"]["users"]["selected_users"], json!(selected));
    assert_eq!(state["B"]["single"]["selected_user"], "U00000003");
    inputs.commit_users(&key(), &[]).unwrap();
    assert_eq!(inputs.saved_users(&key()), Some(vec![]));
    assert_eq!(inputs.fields[&key()].value, InputValue::Users(vec![]));
}

#[test]
fn message_refresh_retains_wire_selection_and_resets_changed_initial_display() {
    let original = message(Some(json!(["U00000001"])));
    let mut inputs = MessageInputs::parse(&original).unwrap();
    inputs.commit_users(&key(), &["U00000002".into()]).unwrap();
    inputs.reconcile(&original).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        InputValue::Users(vec!["U00000002".into()])
    );
    let updated = message(Some(json!([])));
    inputs.reconcile(&updated).unwrap();
    assert_eq!(inputs.fields[&key()].value, InputValue::Users(vec![]));
    assert_eq!(inputs.saved_users(&key()), Some(vec!["U00000002".into()]));
    let mut removed = updated;
    removed.blocks[0]["elements"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    inputs.reconcile(&removed).unwrap();
    assert!(inputs.saved_users(&key()).is_none());
}

#[test]
fn reduced_maximum_preserves_initial_and_saved_users_until_corrected() {
    let three = vec!["U00000001".into(), "U00000002".into(), "U00000003".into()];
    for initial in [json!(["U00000001"]), json!(three)] {
        let mut original = message(Some(initial));
        original.blocks[0]["elements"][0]["max_selected_items"] = json!(3);
        let mut inputs = MessageInputs::parse(&original).unwrap();
        inputs.commit_users(&key(), &three).unwrap();
        let before = inputs.values().unwrap();
        let mut changed = original;
        changed.blocks[0]["elements"][0]["max_selected_items"] = json!(2);
        inputs.reconcile(&changed).unwrap();
        assert_eq!(
            inputs.fields[&key()].value,
            InputValue::Users(three.clone())
        );
        assert_eq!(inputs.saved_users(&key()), Some(three.clone()));
        assert_eq!(
            inputs.values().unwrap(),
            before,
            "sibling actions retain untouched wire state"
        );
        assert!(
            inputs.commit_users(&key(), &three).is_err(),
            "new submissions enforce the reduced max"
        );
        inputs.commit_users(&key(), &three[..2]).unwrap();
        assert_eq!(inputs.saved_users(&key()), Some(three[..2].to_vec()));
    }
    // An already over-limit initial array still renders before any interaction.
    let mut inputs = MessageInputs::parse(&message(Some(json!(three)))).unwrap();
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap().unwrap()["B"]["users"]["selected_users"],
        json!(three)
    );
    assert!(inputs.commit_users(&key(), &three).is_err());
    inputs.commit_users(&key(), &three[..2]).unwrap();
}
