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
fn message() -> Message {
    serde_json::from_value(json!({"channel":"C1","ts":"1719800000.000001","bot_id":"B1","blocks":[{"type":"actions","block_id":"conversations","elements":[{"type":"conversations_select","action_id":"due","initial_conversation":"U00000001","placeholder":{"type":"plain_text","text":"Pick a day"},"confirm":{"title":{"type":"plain_text","text":"Change user?"},"text":{"type":"mrkdwn","text":"Apply *user*?"},"confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"Cancel"}}}]}]})).unwrap()
}
fn capture(
    message: &Message,
    value: Option<&str>,
) -> Result<CapturedMessageConversation, MessageActionError> {
    CapturedMessageConversation::capture(owner(), message, "conversations", "due", value)
}
#[test]
fn exact_conversation_action_has_confirmation_and_truthy_initial_but_no_placeholder() {
    let message = message();
    let before = serde_json::to_value(&message).unwrap();
    let captured = capture(&message, Some("U00000002")).unwrap();
    assert_eq!(captured.selected_conversation(), Some("U00000002"));
    assert_eq!(
        captured.revalidate(&owner(), &message).unwrap().action(),
        &json!({"type":"conversations_select","block_id":"conversations","action_id":"due","selected_conversation":"U00000002","initial_conversation":"U00000001","confirm":message.blocks[0]["elements"][0]["confirm"]})
    );
    drop(captured);
    assert_eq!(serde_json::to_value(&message).unwrap(), before);
    for invalid in [
        "",
        "2026-02-29",
        "2026-2-03",
        "U00000001T00:00:00",
        "not-a-user",
    ] {
        assert!(capture(&message, Some(invalid)).is_err(), "{invalid}");
    }
}
#[test]
fn explicit_null_and_missing_empty_initial_preserve_captured_wire() {
    for initial in [Value::Null, json!("")] {
        let mut message = message();
        message.blocks[0]["elements"][0]["initial_conversation"] = initial;
        let captured = capture(&message, None).unwrap();
        let current = captured.revalidate(&owner(), &message).unwrap();
        assert_eq!(current.action()["selected_conversation"], Value::Null);
        assert!(current.action().get("initial_conversation").is_none());
    }
    let mut message = message();
    message.blocks[0]["elements"][0]["initial_conversation"] = json!("2026-02-30");
    assert!(capture(&message, Some("W00000003")).is_err());
}
#[test]
fn stale_conversations_confirmation_owner_and_service_are_rejected() {
    let message = message();
    let captured = capture(&message, Some("W00000003")).unwrap();
    for case in 0..4 {
        let mut changed = message.clone();
        match case {
            0 => changed.blocks[0]["elements"][0]["initial_conversation"] = json!("U00000004"),
            1 => changed.blocks[0]["elements"][0]["confirm"]["text"]["text"] = json!("Different"),
            2 => changed.bot_id = Some("B2".into()),
            _ => changed.blocks.clear(),
        };
        assert!(captured.revalidate(&owner(), &changed).is_err());
    }
    let mut other = owner();
    other.account_epoch += 1;
    assert!(captured.revalidate(&other, &message).is_err());
    for bad in [None, Some("BSLACKBOT")] {
        let mut changed = message.clone();
        changed.bot_id = bad.map(str::to_owned);
        assert!(capture(&changed, None).is_err());
    }
    let mut changed = message;
    changed.blocks[0]["elements"][0]["visible_to_user_ids"] = json!(["U2"]);
    assert!(capture(&changed, None).is_err());
}

#[test]
fn member_channel_ids_and_response_flag_are_forwarded_without_opening_dm() {
    let mut message = message();
    message.blocks[0]["elements"][0]["response_url_enabled"] = json!(true);
    message.blocks[0]["elements"][0]["default_to_current_conversation"] = json!(true);
    for id in [
        "U00000001",
        "W00000001",
        "USLACKBOT",
        "C00000001",
        "G00000001",
        "D00000001",
        "Z00000001",
    ] {
        let captured = capture(&message, Some(id)).unwrap();
        let current = captured.revalidate(&owner(), &message).unwrap();
        assert_eq!(current.action()["selected_conversation"], id);
        assert_eq!(current.action()["response_url_enabled"], true);
        assert!(
            current
                .action()
                .get("default_to_current_conversation")
                .is_none()
        );
    }
}
#[test]
fn malformed_filter_rejects_capture_and_default_current_does_not_invent_initial() {
    let mut message = message();
    let element = &mut message.blocks[0]["elements"][0];
    element
        .as_object_mut()
        .unwrap()
        .remove("initial_conversation");
    element["default_to_current_conversation"] = json!(true);
    let field = form::parse_message_conversation(element).unwrap();
    assert_eq!(field.value, form::InputValue::Conversation(None));
    for filter in [
        json!({"include":["unknown"]}),
        json!({"exclude_bot_users":"true"}),
        json!({"invented":true}),
    ] {
        message.blocks[0]["elements"][0]["filter"] = filter;
        assert!(capture(&message, Some("C00000001")).is_err());
    }
}
