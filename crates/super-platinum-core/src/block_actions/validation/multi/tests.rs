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
    json!({"text":{"type":"plain_text","text":format!("Choice {value}")},"value":value,"description":{"type":"plain_text","text":"Details"},"extra":{"keep":true}})
}
fn message(kind: &str) -> Message {
    serde_json::from_value(json!({"channel":"C1","ts":owner().message_ts,"bot_id":"B1","blocks":[{"type":"actions","block_id":"B","elements":[{"type":kind,"action_id":"A","options":[option("a"),option("b"),option("")],"initial_options":[option("a")],"placeholder":{"type":"plain_text","text":"Pick"}}]}]})).unwrap()
}
fn capture(
    message: &Message,
    selected: &[Value],
    retained: &[Value],
) -> Result<CapturedMessageMulti, MessageActionError> {
    CapturedMessageMulti::capture(owner(), message, "B", "A", selected, retained)
}
#[test]
fn multiple_actions_preserve_order_metadata_and_checkbox_omits_select_props() {
    for kind in ["checkboxes", "multi_static_select"] {
        let message = message(kind);
        let selected = vec![option("b"), option("")];
        let captured = capture(&message, &selected, &[]).unwrap();
        assert_eq!(captured.selected_options(), selected);
        let current = captured.revalidate(&owner(), &message).unwrap();
        assert_eq!(current.action()["type"], kind);
        assert_eq!(current.action()["selected_options"], json!(selected));
        assert_eq!(
            current.action().get("initial_options").is_some(),
            kind == "multi_static_select"
        );
        assert_eq!(
            current.action().get("placeholder").is_some(),
            kind == "multi_static_select"
        );
        assert_eq!(current.container()["is_ephemeral"], false);
        let cleared = capture(&message, &[], &selected).unwrap();
        assert_eq!(
            cleared.revalidate(&owner(), &message).unwrap().action()["selected_options"],
            json!([])
        );
    }
}
#[test]
fn retained_raw_options_survive_new_option_lists_but_forged_and_duplicate_values_fail() {
    for kind in ["checkboxes", "multi_static_select"] {
        let mut message = message(kind);
        let old = option("old");
        let selected = vec![old.clone(), option("b")];
        assert!(capture(&message, &selected, &[]).is_err());
        assert!(capture(&message, &selected, std::slice::from_ref(&old)).is_ok());
        let mut forged = old.clone();
        forged["text"]["text"] = json!("Forged");
        assert!(capture(&message, &[forged], std::slice::from_ref(&old)).is_err());
        assert!(
            capture(
                &message,
                &[old.clone(), old.clone()],
                std::slice::from_ref(&old)
            )
            .is_err()
        );
        if kind == "multi_static_select" {
            message.blocks[0]["elements"][0]["max_selected_items"] = json!(1);
            assert!(capture(&message, &selected, std::slice::from_ref(&old)).is_err());
        }
    }
}
#[test]
fn confirmation_capture_is_pure_and_account_message_recipient_changes_reject() {
    let mut message = message("checkboxes");
    let confirm = json!({"title":{"type":"plain_text","text":"Change?"},"text":{"type":"mrkdwn","text":"Change *choices*?"},"confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"No"}});
    message.blocks[0]["elements"][0]["confirm"] = confirm.clone();
    let before = message.clone();
    let captured = capture(&message, &[option("b")], &[]).unwrap();
    assert_eq!(
        serde_json::to_value(&message).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    assert_eq!(captured.confirmation(), Some(&confirm));
    let mut changed_owner = owner();
    changed_owner.account_epoch += 1;
    assert!(matches!(
        captured.revalidate(&changed_owner, &message),
        Err(MessageActionError::WrongMessage)
    ));
    message.blocks[0]["elements"][0]["initial_options"] = json!([]);
    assert!(matches!(
        captured.revalidate(&owner(), &message),
        Err(MessageActionError::StaleMessage)
    ));
    message.blocks[0]["elements"][0]["visible_to_user_ids"] = json!(["OTHER"]);
    assert!(capture(&message, &[], &[]).is_err());
}
#[test]
fn message_multi_parser_accepts_grouped_selects_and_checkbox_mrkdwn_but_rejects_malformed() {
    let mut select = message("multi_static_select").blocks[0]["elements"][0].clone();
    select.as_object_mut().unwrap().remove("options");
    select["option_groups"] =
        json!([{"label":{"type":"plain_text","text":"Group"},"options":[option("a"),option("b")]}]);
    let field = form::parse_message_multi(&select).unwrap();
    let form::InputKind::StaticSelect {
        options, multiple, ..
    } = field.kind
    else {
        panic!()
    };
    assert!(multiple);
    assert_eq!(options[0].group.as_deref(), Some("Group"));
    let mut checks = message("checkboxes").blocks[0]["elements"][0].clone();
    checks["options"][0]["text"]["type"] = json!("mrkdwn");
    checks["initial_options"] = json!([]);
    assert!(form::parse_message_multi(&checks).is_ok());
    checks["initial_options"] = Value::Null;
    assert!(form::parse_message_multi(&checks).is_ok());
    for bad in [
        json!(true),
        json!([option("missing")]),
        json!([option("b"), option("b")]),
    ] {
        checks["initial_options"] = bad;
        assert!(form::parse_message_multi(&checks).is_err());
    }
}

#[test]
fn untouched_uninitialized_multi_capture_omits_selection_but_explicit_clear_is_array() {
    let mut message = message("multi_static_select");
    message.blocks[0]["elements"][0]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    let capture =
        CapturedMessageMulti::capture_optional(owner(), &message, "B", "A", None, &[]).unwrap();
    assert_eq!(capture.selected_options_optional(), None);
    assert!(
        capture
            .revalidate(&owner(), &message)
            .unwrap()
            .action()
            .get("selected_options")
            .is_none()
    );
    let clear = CapturedMessageMulti::capture_optional(owner(), &message, "B", "A", Some(&[]), &[])
        .unwrap();
    assert_eq!(clear.selected_options_optional(), Some(&[][..]));
    assert_eq!(
        clear.revalidate(&owner(), &message).unwrap().action()["selected_options"],
        json!([])
    );
    assert!(
        CapturedMessageMulti::capture_optional(owner(), &message, "B", "A", None, &[option("a")])
            .is_err()
    );
    message.blocks[0]["elements"][0]["initial_options"] = json!([]);
    assert!(
        CapturedMessageMulti::capture_optional(owner(), &message, "B", "A", None, &[]).is_err()
    );
    message.blocks[0]["elements"][0]["type"] = json!("checkboxes");
    message.blocks[0]["elements"][0]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    assert!(
        CapturedMessageMulti::capture_optional(owner(), &message, "B", "A", None, &[]).is_err()
    );
}
