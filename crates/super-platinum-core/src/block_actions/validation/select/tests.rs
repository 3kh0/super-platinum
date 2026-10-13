use super::*;

fn owner() -> MessageActionOwner {
    MessageActionOwner {
        account_epoch: 1,
        self_user_id: "U1".into(),
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1719800000.000001".into(),
    }
}
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Choice {value}"),"emoji":true},
        "value":value,"description":{"type":"plain_text","text":"Keep metadata"},"custom":{"a":[1,null]}})
}
fn message() -> Message {
    serde_json::from_value(
        json!({"channel":"C1","ts":"1719800000.000001","bot_id":"B1",
        "blocks":[{"type":"actions","block_id":"choose","elements":[{
            "type":"static_select","action_id":"priority","options":[option("a"),option("b")],
            "initial_option":option("a"),"placeholder":{"type":"plain_text","text":"Pick"}
        }]}]}),
    )
    .unwrap()
}
fn element(message: &mut Message) -> &mut Value {
    &mut message.blocks[0]["elements"][0]
}
fn capture(
    message: &Message,
    selected: Option<&Value>,
) -> Result<CapturedMessageSelect, MessageActionError> {
    CapturedMessageSelect::capture(owner(), message, "choose", "priority", selected)
}
fn assert_error(
    result: Result<CapturedMessageSelect, MessageActionError>,
    expected: MessageActionError,
) {
    assert!(matches!(result,Err(error) if error==expected));
}

#[test]
fn selection_preserves_exact_raw_options_initial_and_placeholder() {
    let message = message();
    let selected = option("b");
    let captured = capture(&message, Some(&selected)).unwrap();
    assert_eq!(captured.selected_option(), Some(&selected));
    let current = captured.revalidate(&owner(), &message).unwrap();
    assert_eq!(
        current.action(),
        &json!({"type":"static_select","block_id":"choose","action_id":"priority",
        "selected_option":selected,"initial_option":option("a"),"placeholder":{"type":"plain_text","text":"Pick"}})
    );
    assert_eq!(current.service_id(), "B1");
    assert_eq!(
        current.container(),
        json!({"type":"message","channel_id":"C1","message_ts":"1719800000.000001","is_ephemeral":false})
    );
}

#[test]
fn option_groups_and_null_selection_keep_canonical_identity() {
    let mut message = message();
    let select = element(&mut message);
    select.as_object_mut().unwrap().remove("options");
    select["option_groups"] =
        json!([{"label":{"type":"plain_text","text":"Group"},"options":[option("a"),option("b")]}]);
    let captured = capture(&message, Some(&option("b"))).unwrap();
    assert!(captured.revalidate(&owner(), &message).is_ok());
    element(&mut message)["initial_option"] = Value::Null;
    let cleared = capture(&message, None).unwrap();
    let current = cleared.revalidate(&owner(), &message).unwrap();
    assert_eq!(current.action()["selected_option"], Value::Null);
    assert!(current.action().get("initial_option").is_none());
    assert!(cleared.selected_option().is_none());
    assert_error(
        capture(&message, Some(&Value::Null)),
        MessageActionError::MalformedButton,
    );
}

#[test]
fn spoofed_option_metadata_duplicates_and_wrong_initial_selection_are_rejected() {
    let mut changed = option("b");
    changed["text"]["text"] = json!("Forged");
    assert_error(
        capture(&message(), Some(&changed)),
        MessageActionError::MalformedButton,
    );
    for case in 0..4 {
        let mut message = message();
        match case {
            0 => element(&mut message)["options"] = json!([option("a"), option("a")]),
            1 => element(&mut message)["initial_option"] = option("missing"),
            2 => element(&mut message)["option_groups"] = json!([]),
            _ => element(&mut message)["placeholder"] = json!({"type":"mrkdwn","text":"bad"}),
        }
        assert_error(capture(&message, None), MessageActionError::MalformedButton);
    }
}

#[test]
fn confirmation_is_frozen_and_capture_cancel_does_not_mutate_source() {
    let mut message = message();
    let confirm = json!({"title":{"type":"plain_text","text":"Change?"},"text":{"type":"mrkdwn","text":"Change *priority*?"},
        "confirm":{"type":"plain_text","text":"Change"},"deny":{"type":"plain_text","text":"Cancel"},"style":"danger"});
    element(&mut message)["confirm"] = confirm.clone();
    let before = serde_json::to_value(&message).unwrap();
    let candidate = capture(&message, Some(&option("b"))).unwrap();
    assert_eq!(candidate.confirmation(), Some(&confirm));
    assert_eq!(
        candidate.revalidate(&owner(), &message).unwrap().action()["confirm"],
        confirm
    );
    drop(candidate); // Cancel discards the candidate; it commits no local/wire state.
    assert_eq!(serde_json::to_value(&message).unwrap(), before);
    element(&mut message)["confirm"]["deny"] = Value::Null;
    assert_error(capture(&message, None), MessageActionError::MalformedButton);
}

#[test]
fn canonical_option_or_owner_changes_retire_confirmed_selection() {
    let message = message();
    let captured = capture(&message, Some(&option("b"))).unwrap();
    let mut other = owner();
    other.self_user_id = "U2".into();
    assert!(matches!(
        captured.revalidate(&other, &message),
        Err(MessageActionError::WrongMessage)
    ));
    for case in 0..5 {
        let mut changed = message.clone();
        match case {
            0 => element(&mut changed)["options"] = json!([option("a")]),
            1 => element(&mut changed)["options"][1]["description"]["text"] = json!("New details"),
            2 => element(&mut changed)["initial_option"] = option("b"),
            3 => changed.bot_id = Some("B2".into()),
            _ => changed.blocks[0]["block_id"] = json!("different"),
        }
        assert!(
            matches!(
                captured.revalidate(&owner(), &changed),
                Err(MessageActionError::StaleMessage)
            ),
            "case {case}"
        );
    }
}

#[test]
fn recipients_special_routes_and_missing_service_remain_gated() {
    let mut message = message();
    element(&mut message)["visible_to_user_ids"] = json!(["U1"]);
    assert!(capture(&message, None).is_ok());
    element(&mut message)["visible_to_user_ids"] = json!(["U2"]);
    assert_error(
        capture(&message, None),
        MessageActionError::UnsupportedButton,
    );
    element(&mut message)
        .as_object_mut()
        .unwrap()
        .remove("visible_to_user_ids");
    message.bot_id = None;
    assert_error(
        capture(&message, None),
        MessageActionError::MissingServiceIdentity,
    );
    message.bot_id = Some("BSLACKBOT".into());
    assert_error(
        capture(&message, None),
        MessageActionError::UnsupportedButton,
    );
    message.bot_id = Some("B1".into());
    element(&mut message)["action_id"] = json!("mcp_tool_blocked_change_prefs");
    assert_error(
        CapturedMessageSelect::capture(
            owner(),
            &message,
            "choose",
            "mcp_tool_blocked_change_prefs",
            None,
        ),
        MessageActionError::UnsupportedButton,
    );
}

#[test]
fn section_accessory_supported_but_duplicate_ids_and_input_container_are_not() {
    let mut message = message();
    let select = element(&mut message).clone();
    message.blocks = vec![json!({"type":"section","block_id":"choose","accessory":select})];
    assert!(capture(&message, Some(&option("b"))).is_ok());
    message.blocks.push(message.blocks[0].clone());
    assert_error(capture(&message, None), MessageActionError::AmbiguousButton);
    message.blocks = vec![json!({"type":"input","block_id":"choose","element":select})];
    assert_error(
        capture(&message, None),
        MessageActionError::UnsupportedButton,
    );
}
