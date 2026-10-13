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
    serde_json::from_value(json!({"channel":"C1","ts":"1719800000.000001","bot_id":"B1","blocks":[{"type":"actions","block_id":"times","elements":[{"type":"timepicker","action_id":"pick","initial_time":"09:30","timezone":"America/New_York","placeholder":{"type":"plain_text","text":"Choose"},"confirm":{"title":{"type":"plain_text","text":"Change?"},"text":{"type":"mrkdwn","text":"Change *time*?"},"confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"Cancel"}}}]}]})).unwrap()
}
fn capture(
    message: &Message,
    selected: Option<&str>,
) -> Result<CapturedMessageTime, MessageActionError> {
    CapturedMessageTime::capture(owner(), message, "times", "pick", selected)
}
#[test]
fn time_action_preserves_wall_time_timezone_and_confirmation_without_placeholder() {
    let message = message();
    let before = serde_json::to_value(&message).unwrap();
    for selected in [None, Some("00:00"), Some("23:59")] {
        let captured = capture(&message, selected).unwrap();
        assert_eq!(captured.selected_time(), selected);
        assert_eq!(
            captured.revalidate(&owner(), &message).unwrap().action(),
            &json!({"type":"timepicker","block_id":"times","action_id":"pick","selected_time":selected,"initial_time":"09:30","timezone":"America/New_York","confirm":message.blocks[0]["elements"][0]["confirm"]})
        );
        drop(captured);
        assert_eq!(serde_json::to_value(&message).unwrap(), before);
    }
    for invalid in [
        "",
        "9:30",
        "24:00",
        "12:60",
        "09:30:00",
        "09:30Z",
        "-1:00",
        "aa:bb",
        "１２:３０",
    ] {
        assert!(capture(&message, Some(invalid)).is_err(), "{invalid}");
    }
}
#[test]
fn truthy_initial_and_timezone_only_are_exact_and_invalid_initial_rejected() {
    for empty in [Value::Null, json!("")] {
        let mut message = message();
        message.blocks[0]["elements"][0]["initial_time"] = empty.clone();
        message.blocks[0]["elements"][0]["timezone"] = empty;
        let captured = capture(&message, Some("10:15")).unwrap();
        let current = captured.revalidate(&owner(), &message).unwrap();
        assert!(current.action().get("initial_time").is_none());
        assert!(current.action().get("timezone").is_none());
    }
    for (key, bad) in [("initial_time", json!("25:00")), ("timezone", json!(7))] {
        let mut message = message();
        message.blocks[0]["elements"][0][key] = bad;
        assert!(capture(&message, None).is_err());
    }
}
#[test]
fn timezone_or_initial_changes_and_wrong_owner_retire_captured_time() {
    let message = message();
    let captured = capture(&message, Some("10:15")).unwrap();
    for (key, value) in [
        ("timezone", json!("Europe/Berlin")),
        ("initial_time", json!("11:00")),
    ] {
        let mut changed = message.clone();
        changed.blocks[0]["elements"][0][key] = value;
        assert!(matches!(
            captured.revalidate(&owner(), &changed),
            Err(MessageActionError::StaleMessage)
        ));
    }
    let mut other = owner();
    other.account_epoch += 1;
    assert!(captured.revalidate(&other, &message).is_err());
    for bot in [None, Some("BSLACKBOT")] {
        let mut changed = message.clone();
        changed.bot_id = bot.map(str::to_owned);
        assert!(capture(&changed, None).is_err());
    }
    let mut changed = message;
    changed.blocks[0]["elements"][0]["visible_to_user_ids"] = json!(["U2"]);
    assert!(capture(&changed, None).is_err());
}
