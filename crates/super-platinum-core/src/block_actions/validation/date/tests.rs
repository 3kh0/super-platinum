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
    serde_json::from_value(json!({"channel":"C1","ts":"1719800000.000001","bot_id":"B1","blocks":[{"type":"actions","block_id":"dates","elements":[{"type":"datepicker","action_id":"due","initial_date":"2026-10-13","placeholder":{"type":"plain_text","text":"Pick a day"},"confirm":{"title":{"type":"plain_text","text":"Change date?"},"text":{"type":"mrkdwn","text":"Apply *date*?"},"confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"Cancel"}}}]}]})).unwrap()
}
fn capture(
    message: &Message,
    value: Option<&str>,
) -> Result<CapturedMessageDate, MessageActionError> {
    CapturedMessageDate::capture(owner(), message, "dates", "due", value)
}
#[test]
fn exact_date_action_has_confirmation_and_truthy_initial_but_no_placeholder() {
    let message = message();
    let before = serde_json::to_value(&message).unwrap();
    let captured = capture(&message, Some("2028-02-29")).unwrap();
    assert_eq!(captured.selected_date(), Some("2028-02-29"));
    assert_eq!(
        captured.revalidate(&owner(), &message).unwrap().action(),
        &json!({"type":"datepicker","block_id":"dates","action_id":"due","selected_date":"2028-02-29","initial_date":"2026-10-13","confirm":message.blocks[0]["elements"][0]["confirm"]})
    );
    drop(captured);
    assert_eq!(serde_json::to_value(&message).unwrap(), before);
    for invalid in [
        "",
        "2026-02-29",
        "2026-2-03",
        "2026-10-13T00:00:00",
        "not-a-date",
    ] {
        assert!(capture(&message, Some(invalid)).is_err(), "{invalid}");
    }
}
#[test]
fn explicit_null_and_missing_empty_initial_preserve_captured_wire() {
    for initial in [Value::Null, json!("")] {
        let mut message = message();
        message.blocks[0]["elements"][0]["initial_date"] = initial;
        let captured = capture(&message, None).unwrap();
        let current = captured.revalidate(&owner(), &message).unwrap();
        assert_eq!(current.action()["selected_date"], Value::Null);
        assert!(current.action().get("initial_date").is_none());
    }
    let mut message = message();
    message.blocks[0]["elements"][0]["initial_date"] = json!("2026-02-30");
    assert!(capture(&message, Some("2026-10-14")).is_err());
}
#[test]
fn stale_dates_confirmation_owner_and_service_are_rejected() {
    let message = message();
    let captured = capture(&message, Some("2026-10-14")).unwrap();
    for case in 0..4 {
        let mut changed = message.clone();
        match case {
            0 => changed.blocks[0]["elements"][0]["initial_date"] = json!("2026-10-15"),
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
