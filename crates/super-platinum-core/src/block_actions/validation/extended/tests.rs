use super::*;

fn owner() -> MessageActionOwner {
    MessageActionOwner {
        account_epoch: 1,
        self_user_id: "U00000001".into(),
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1719800000.000001".into(),
    }
}
fn message(element: Value) -> Message {
    serde_json::from_value(json!({"channel":"C1","ts":owner().message_ts,"bot_id":"B1",
        "blocks":[{"type":"actions","block_id":"B","elements":[element]}]}))
    .unwrap()
}
fn confirmation() -> Value {
    json!({"title":{"type":"plain_text","text":"Apply?"},"text":{"type":"mrkdwn","text":"Apply *selection*?"},
        "confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"Cancel"}})
}
fn users() -> Value {
    json!({"type":"multi_users_select","action_id":"a","initial_users":[],"max_selected_items":2,"confirm":confirmation()})
}
fn overflow() -> Value {
    json!({"type":"overflow","action_id":"a","confirm":confirmation(),"options":[
        {"text":{"type":"plain_text","text":"Open","emoji":true},"value":"open","url":"https://example.com","description":{"type":"plain_text","text":"Details"}},
        {"text":{"type":"plain_text","text":"Run"},"value":"run"}
    ]})
}

#[test]
fn user_capture_keeps_exact_arrays_confirmation_and_optional_initial() {
    let message = message(users());
    let original = serde_json::to_value(&message).unwrap();
    let selected = vec!["U00000002".into(), "W00000003".into()];
    let captured = CapturedMessageUsers::capture(owner(), &message, "B", "a", &selected).unwrap();
    assert_eq!(captured.selected_users(), selected);
    assert_eq!(captured.confirmation(), Some(&confirmation()));
    assert_eq!(
        captured.revalidate(&owner(), &message).unwrap().action(),
        &json!({
            "type":"multi_users_select","block_id":"B","action_id":"a","selected_users":selected,
            "initial_users":[],"confirm":confirmation()
        })
    );
    drop(captured);
    assert_eq!(
        serde_json::to_value(&message).unwrap(),
        original,
        "cancelling capture is pure"
    );
    let mut element = users();
    element.as_object_mut().unwrap().remove("initial_users");
    let fresh = super::tests::message(element);
    let captured = CapturedMessageUsers::capture(owner(), &fresh, "B", "a", &[]).unwrap();
    let current = captured.revalidate(&owner(), &fresh).unwrap();
    assert!(current.action().get("initial_users").is_none());
    assert_eq!(current.action()["selected_users"], json!([]));
}

#[test]
fn user_required_limit_duplicates_and_malformed_initial_are_rejected() {
    let mut element = users();
    element["required"] = json!(true);
    let message = message(element);
    for selected in [
        vec![],
        vec!["U00000002".into(), "U00000002".into()],
        vec!["bad".into()],
        vec!["U00000002".into(), "U00000003".into(), "U00000004".into()],
    ] {
        assert!(CapturedMessageUsers::capture(owner(), &message, "B", "a", &selected).is_err());
    }
    assert!(
        CapturedMessageUsers::capture(owner(), &message, "B", "a", &["USLACKBOT".into()]).is_ok()
    );
    for invalid in [
        json!("U00000001"),
        json!([null]),
        json!(["U00000001", "U00000001"]),
    ] {
        let mut element = users();
        element["initial_users"] = invalid;
        assert!(form::parse_message_users(&element).is_err());
    }
    for invalid in [json!(0), json!(-1), json!(1.5), json!("2")] {
        let mut element = users();
        element["max_selected_items"] = invalid;
        assert!(form::parse_message_users(&element).is_err());
    }
}

#[test]
fn overflow_action_keeps_only_text_value_and_routes_url_separately() {
    let message = message(overflow());
    let captured = CapturedMessageOverflow::capture(owner(), &message, "B", "a", "open").unwrap();
    assert_eq!(captured.url(), Some("https://example.com"));
    assert_eq!(captured.confirmation(), Some(&confirmation()));
    let current = captured.revalidate(&owner(), &message).unwrap();
    assert_eq!(current.url(), Some("https://example.com"));
    assert_eq!(
        current.action(),
        &json!({"type":"overflow","block_id":"B","action_id":"a",
        "selected_option":{"text":{"type":"plain_text","text":"Open","emoji":true},"value":"open"},"confirm":confirmation()})
    );
    assert!(CapturedMessageOverflow::capture(owner(), &message, "B", "a", "missing").is_err());
    assert!(
        CapturedMessageOverflow::capture(owner(), &message, "B", "a", "run")
            .unwrap()
            .url()
            .is_none()
    );
}

#[test]
fn overflow_bounds_and_exact_option_metadata_are_enforced() {
    let option = overflow()["options"][0].clone();
    for count in [0, 6] {
        let mut element = overflow();
        element["options"] = json!(vec![option.clone(); count]);
        assert!(form::parse_message_overflow(&element).is_err());
    }
    let mut element = overflow();
    element["options"] = json!([option]);
    assert_eq!(form::parse_message_overflow(&element).unwrap().len(), 1);
    for (key, value) in [
        ("url", json!(1)),
        ("value", json!(null)),
        ("text", json!({"type":"mrkdwn","text":"Bad"})),
    ] {
        let mut bad = element.clone();
        bad["options"][0][key] = value;
        assert!(form::parse_message_overflow(&bad).is_err());
    }
}

#[test]
fn both_controls_reject_stale_owner_revision_visibility_and_special_routing() {
    for element in [users(), overflow()] {
        let message = message(element.clone());
        let captured = if element["type"] == "overflow" {
            CapturedMessageOverflow::capture(owner(), &message, "B", "a", "run")
                .unwrap()
                .0
        } else {
            CapturedMessageUsers::capture(owner(), &message, "B", "a", &[])
                .unwrap()
                .0
        };
        let kind = element["type"].as_str().unwrap();
        for case in 0..5 {
            let mut other = owner();
            match case {
                0 => other.account_epoch += 1,
                1 => other.team_id = "T2".into(),
                2 => other.channel_id = "C2".into(),
                3 => other.self_user_id = "U00000002".into(),
                _ => other.message_ts = "1719800000.000002".into(),
            }
            assert_eq!(
                captured.revalidate(&other, &message, kind),
                Err(MessageActionError::WrongMessage)
            );
        }
        let mut changed = message.clone();
        changed.blocks[0]["elements"][0]["confirm"]["text"]["text"] = json!("Changed");
        assert_eq!(
            captured.revalidate(&owner(), &changed, kind),
            Err(MessageActionError::StaleMessage)
        );
        for (key, value) in [
            ("visible_to_user_ids", json!(["U00000002"])),
            ("third_party_auth", json!({})),
        ] {
            let mut bad = element.clone();
            bad[key] = value;
            assert!(
                CapturedElement::validate(&owner(), &super::tests::message(bad), "B", "a", kind)
                    .is_err()
            );
        }
        let mut bad = message.clone();
        bad.bot_id = Some("BSLACKBOT".into());
        assert!(CapturedElement::validate(&owner(), &bad, "B", "a", kind).is_err());
    }
}

#[test]
fn channels_capture_preserves_order_clear_initial_confirmation_and_immutable_owner() {
    for initial in [None, Some(json!([])), Some(json!(["C00000002"]))] {
        let mut element = json!({"type":"multi_channels_select","action_id":"a","confirm":confirmation(),"response_url_enabled":true});
        if let Some(value) = initial.clone() {
            element["initial_channels"] = value;
        }
        let message = message(element);
        let original = serde_json::to_value(&message).unwrap();
        for selected in [vec![], vec!["G00000003".into(), "C00000002".into()]] {
            let captured =
                CapturedMessageChannels::capture(owner(), &message, "B", "a", &selected).unwrap();
            assert_eq!(captured.selected_channels(), selected);
            assert_eq!(captured.confirmation(), Some(&confirmation()));
            let current = captured.revalidate(&owner(), &message).unwrap();
            assert_eq!(current.action()["selected_channels"], json!(selected));
            assert_eq!(current.action().get("initial_channels"), initial.as_ref());
            assert!(current.action().get("response_url_enabled").is_none());
            let mut changed = message.clone();
            changed.blocks[0]["elements"][0]["max_selected_items"] = json!(1);
            assert!(matches!(
                captured.revalidate(&owner(), &changed),
                Err(MessageActionError::StaleMessage)
            ));
            let mut other = owner();
            other.account_epoch += 1;
            assert!(matches!(
                captured.revalidate(&other, &message),
                Err(MessageActionError::WrongMessage)
            ));
        }
        assert_eq!(
            serde_json::to_value(&message).unwrap(),
            original,
            "cancel capture is pure"
        );
    }
}
#[test]
fn channels_capture_checks_required_max_ids_and_channel_specific_shape() {
    let message = message(
        json!({"type":"multi_channels_select","action_id":"a","required":true,"max_selected_items":1}),
    );
    for ids in [
        vec![],
        vec!["C00000002".into(), "C00000002".into()],
        vec!["U00000002".into()],
        vec!["C00000002".into(), "G00000003".into()],
        vec!["".into()],
    ] {
        assert!(CapturedMessageChannels::capture(owner(), &message, "B", "a", &ids).is_err());
    }
    assert!(
        CapturedMessageChannels::capture(owner(), &message, "B", "a", &["C00000002".into()])
            .is_ok()
    );
}

#[test]
fn conversations_capture_preserves_order_clear_initial_confirmation_and_immutable_owner() {
    for initial in [None, Some(json!([])), Some(json!(["C00000002"]))] {
        let mut element = json!({"type":"multi_conversations_select","action_id":"a","confirm":confirmation(),"response_url_enabled":true,"default_to_current_conversation":true});
        if let Some(value) = initial.clone() {
            element["initial_conversations"] = value;
        }
        let message = message(element);
        let original = serde_json::to_value(&message).unwrap();
        for selected in [vec![], vec!["G00000003".into(), "C00000002".into()]] {
            let captured =
                CapturedMessageConversations::capture(owner(), &message, "B", "a", &selected)
                    .unwrap();
            assert_eq!(captured.selected_conversations(), selected);
            assert_eq!(captured.confirmation(), Some(&confirmation()));
            let current = captured.revalidate(&owner(), &message).unwrap();
            assert_eq!(current.action()["selected_conversations"], json!(selected));
            assert_eq!(
                current.action().get("initial_conversations"),
                initial.as_ref()
            );
            assert!(current.action().get("response_url_enabled").is_none());
            assert!(
                current
                    .action()
                    .get("default_to_current_conversation")
                    .is_none()
            );
            let mut changed = message.clone();
            changed.blocks[0]["elements"][0]["default_to_current_conversation"] = json!(false);
            assert!(matches!(
                captured.revalidate(&owner(), &changed),
                Err(MessageActionError::StaleMessage)
            ));
            let mut changed = message.clone();
            changed.blocks[0]["elements"][0]["max_selected_items"] = json!(1);
            assert!(matches!(
                captured.revalidate(&owner(), &changed),
                Err(MessageActionError::StaleMessage)
            ));
            let mut other = owner();
            other.account_epoch += 1;
            assert!(matches!(
                captured.revalidate(&other, &message),
                Err(MessageActionError::WrongMessage)
            ));
        }
        assert_eq!(
            serde_json::to_value(&message).unwrap(),
            original,
            "cancel capture is pure"
        );
    }
}
#[test]
fn conversations_capture_checks_required_max_ids_and_conversation_specific_shape() {
    let message = message(
        json!({"type":"multi_conversations_select","action_id":"a","required":true,"max_selected_items":1}),
    );
    for ids in [
        vec![],
        vec!["C00000002".into(), "C00000002".into()],
        vec!["X00000002".into()],
        vec!["C00000002".into(), "G00000003".into()],
        vec!["".into()],
    ] {
        assert!(CapturedMessageConversations::capture(owner(), &message, "B", "a", &ids).is_err());
    }
    assert!(
        CapturedMessageConversations::capture(owner(), &message, "B", "a", &["C00000002".into()])
            .is_ok()
    );
}
