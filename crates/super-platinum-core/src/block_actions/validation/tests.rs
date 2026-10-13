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
    serde_json::from_value(json!({
        "ts":"1719800000.000001", "bot_id":"B1", "channel":"C1",
        "blocks":[{"type":"actions","block_id":"review","elements":[{
            "type":"button","action_id":"approve","text":{"type":"plain_text","text":"Approve"},
            "value":"item-1","style":"primary","url":"https://example.test/",
            "accessibility_label":"Approve item one"
        }]}]
    }))
    .unwrap()
}

fn capture(message: &Message) -> Result<CapturedMessageButton, MessageActionError> {
    CapturedMessageButton::capture(owner(), message, "review", "approve")
}

#[test]
fn calendar_and_widget_buttons_require_their_client_specific_flows() {
    for prefix in [
        "ADZ494LHY_CREATE_EVENT",
        "AFV5ECLBZ_CREATE_EVENT",
        "A01NK8UJRGC_CREATE_EVENT",
    ] {
        let mut m = message();
        m.blocks[0]["elements"][0]["value"] = json!(format!("{prefix}:tomorrow"));
        assert_error(capture(&m), MessageActionError::UnsupportedButton);
    }
    let mut m = message();
    m.blocks[0]["block_id"] = json!("widget:review");
    assert_error(
        CapturedMessageButton::capture(owner(), &m, "widget:review", "approve"),
        MessageActionError::UnsupportedButton,
    );
    let mut ordinary = message();
    ordinary.blocks[0]["elements"][0]["value"] = json!("my_CREATE_EVENT:tomorrow");
    assert!(capture(&ordinary).is_ok());
}

#[test]
fn client_owned_buttons_are_not_dispatched_as_developer_actions_for_ordinary_bots() {
    for (block, action) in [
        ("review", "setup_slackbot_joiner_channel_preview"),
        ("review", "setup_slackbot_joiner_channel_join"),
        ("review", "wfb_ai_answer_feedback_positive"),
        ("review", "wfb_ai_answer_feedback_negative"),
        ("review", "set_up_dwa_v3_open_modal"),
        ("review", "mcp_tool_blocked_change_prefs"),
        (
            "ai_agent_action:setup_slackbot_joiner_person_1",
            "setup_slackbot_joiner_person_profile",
        ),
        ("ai_agent_action:review", "salesforce_record_card_edit"),
        (
            "ai_agent_action:review",
            "salesforce_record_card_update_edit",
        ),
        ("sf_files_link:record", "sf_view_all_files"),
        (
            "connect_apps_nudge_actions:google",
            "launch_connect_apps_modal",
        ),
        (
            "connect_apps_nudge_actions:google",
            "launch_connect_apps_modal_google",
        ),
    ] {
        let mut message = message();
        message.blocks[0]["block_id"] = json!(block);
        message.blocks[0]["elements"][0]["action_id"] = json!(action);
        assert_error(
            CapturedMessageButton::capture(owner(), &message, block, action),
            MessageActionError::UnsupportedButton,
        );
    }
}

#[test]
fn scoped_client_ids_do_not_disable_ordinary_app_buttons_in_unrelated_blocks() {
    for action in [
        "setup_slackbot_joiner_person_profile",
        "salesforce_record_card_edit",
        "salesforce_record_card_update_edit",
        "sf_view_all_files",
        "launch_connect_apps_modal",
        "my_wfb_ai_answer_feedback_positive",
        "setup_slackbot_joiner_channel_preview_custom",
    ] {
        let mut message = message();
        message.blocks[0]["elements"][0]["action_id"] = json!(action);
        assert!(
            CapturedMessageButton::capture(owner(), &message, "review", action).is_ok(),
            "{action}"
        );
    }
}

fn assert_error(
    result: Result<CapturedMessageButton, MessageActionError>,
    expected: MessageActionError,
) {
    match result {
        Ok(_) => panic!("unexpected eligible capture"),
        Err(error) => assert_eq!(error, expected),
    }
}

#[test]
fn ordinary_wire_identity_is_scoped_and_excludes_link_presentation() {
    let m = message();
    let saved = capture(&m).unwrap();
    let current = saved.revalidate(&owner(), &m).unwrap();
    assert_eq!(current.service_id(), "B1");
    assert_eq!(
        current.action(),
        &json!({
            "type":"button","block_id":"review","action_id":"approve",
            "text":{"type":"plain_text","text":"Approve"},"value":"item-1","style":"primary"
        })
    );
    assert_eq!(
        current.container(),
        json!({
            "type":"message","message_ts":owner().message_ts,"channel_id":"C1","is_ephemeral":false
        })
    );
    for changed in [
        MessageActionOwner {
            account_epoch: 2,
            ..owner()
        },
        MessageActionOwner {
            self_user_id: "U2".into(),
            ..owner()
        },
        MessageActionOwner {
            team_id: "T2".into(),
            ..owner()
        },
        MessageActionOwner {
            channel_id: "C2".into(),
            ..owner()
        },
        MessageActionOwner {
            message_ts: "1719800001.000001".into(),
            ..owner()
        },
    ] {
        assert!(matches!(
            saved.revalidate(&changed, &m),
            Err(MessageActionError::WrongMessage)
        ));
    }
}

#[test]
fn recipient_restricted_buttons_require_the_current_viewer() {
    let mut m = message();
    m.blocks[0]["elements"][0]["visible_to_user_ids"] = json!(["U1"]);
    assert!(capture(&m).is_ok());
    for recipients in [json!([]), json!(["U2"])] {
        m.blocks[0]["elements"][0]["visible_to_user_ids"] = recipients;
        assert_error(capture(&m), MessageActionError::UnsupportedButton);
    }
    for recipients in [json!(null), json!("U1"), json!(["U1", false])] {
        m.blocks[0]["elements"][0]["visible_to_user_ids"] = recipients;
        assert_error(capture(&m), MessageActionError::MalformedButton);
    }
}

#[test]
fn duplicate_ids_are_resolved_by_block_and_ambiguous_sources_are_rejected() {
    let mut m = message();
    let mut other = m.blocks[0].clone();
    other["block_id"] = json!("other");
    other["elements"][0]["value"] = json!("item-2");
    m.blocks.push(other);
    let saved = CapturedMessageButton::capture(owner(), &m, "other", "approve").unwrap();
    assert_eq!(
        saved.revalidate(&owner(), &m).unwrap().action()["value"],
        "item-2"
    );
    m.blocks.push(m.blocks[0].clone());
    assert_error(capture(&m), MessageActionError::AmbiguousButton);
    m.blocks.pop();
    let duplicate = m.blocks[0]["elements"][0].clone();
    m.blocks[0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert_error(capture(&m), MessageActionError::AmbiguousButton);
}

#[test]
fn edits_replacements_and_app_context_changes_invalidate_pending_confirmation() {
    let m = message();
    let saved = capture(&m).unwrap();
    let changes: Vec<Box<dyn Fn(&mut Message)>> = vec![
        Box::new(|m| m.blocks[0]["elements"][0]["value"] = json!("different item")),
        Box::new(|m| m.blocks[0]["elements"][0]["text"]["text"] = json!("Reject")),
        Box::new(|m| m.blocks[0]["elements"][0]["confirm"] = json!(null)),
        Box::new(|m| {
            m.blocks.clear();
        }),
        Box::new(|m| m.bot_id = Some("B2".into())),
        Box::new(|m| m.edited = Some(json!({"ts":"1719800001.0"}))),
        Box::new(|m| {
            m.extra.insert("function_execution_id".into(), json!("Fx2"));
        }),
        Box::new(|m| m.thread_ts = Some("1719800000.0".into())),
    ];
    for change in changes {
        let mut replaced = m.clone();
        change(&mut replaced);
        assert!(matches!(
            saved.revalidate(&owner(), &replaced),
            Err(MessageActionError::StaleMessage)
        ));
    }
    let mut reaction_only = m.clone();
    reaction_only.reactions =
        serde_json::from_value(json!([{"name":"eyes","users":["U1"],"count":1}])).unwrap();
    assert!(saved.revalidate(&owner(), &reaction_only).is_ok());
}

#[test]
fn ordinary_and_ephemeral_thread_containers_differ_without_subtype_guessing() {
    let mut m = message();
    m.thread_ts = Some("1719799000.000001".into());
    m.subtype = Some("ephemeral".into());
    let saved = capture(&m).unwrap();
    assert_eq!(
        saved
            .revalidate(&owner(), &m)
            .unwrap()
            .container()
            .get("thread_ts"),
        None
    );
    m.extra.insert("is_ephemeral".into(), json!(true));
    let saved = capture(&m).unwrap();
    let container = saved.revalidate(&owner(), &m).unwrap().container();
    assert_eq!(container["is_ephemeral"], true);
    assert_eq!(container["thread_ts"], "1719799000.000001");
    m.extra.insert("is_ephemeral".into(), json!("true"));
    assert_error(capture(&m), MessageActionError::MalformedButton);
}

#[test]
fn confirmation_is_preserved_and_malformed_or_specialized_buttons_cannot_bypass_it() {
    let mut m = message();
    let confirm = json!({
        "title":{"type":"plain_text","text":"Approve?"},
        "text":{"type":"mrkdwn","text":"This approves *item one*."},
        "confirm":{"type":"plain_text","text":"Approve"},
        "deny":{"type":"plain_text","text":"Cancel"}, "style":"danger"
    });
    m.blocks[0]["elements"][0]["confirm"] = confirm.clone();
    let saved = capture(&m).unwrap();
    assert_eq!(
        saved.revalidate(&owner(), &m).unwrap().action()["confirm"],
        confirm
    );
    for invalid in [
        json!(null),
        json!(false),
        json!({}),
        json!({"text":"wrong"}),
    ] {
        m.blocks[0]["elements"][0]["confirm"] = invalid;
        assert_error(capture(&m), MessageActionError::MalformedButton);
    }
    for key in ["agent_prompt", "third_party_auth"] {
        let mut specialized = message();
        specialized.blocks[0]["elements"][0][key] = json!("special");
        assert_error(capture(&specialized), MessageActionError::UnsupportedButton);
    }
}

#[test]
fn missing_identity_pending_messages_and_attachment_substitution_are_rejected() {
    let mut m = message();
    m.bot_id = None;
    assert_error(capture(&m), MessageActionError::MissingServiceIdentity);
    let m = message();
    for ts in ["pending-1", "cached-hello", "", "0.1", "1.x"] {
        let bad_owner = MessageActionOwner {
            message_ts: ts.into(),
            ..owner()
        };
        assert_error(
            CapturedMessageButton::capture(bad_owner, &m, "review", "approve"),
            MessageActionError::InvalidOwner,
        );
    }
    let mut m = message();
    m.channel = Some("C2".into());
    assert_error(capture(&m), MessageActionError::WrongMessage);
    let mut m = message();
    m.attachments = serde_json::from_value(json!([{"blocks":m.blocks}])).unwrap();
    m.blocks.clear();
    assert_error(capture(&m), MessageActionError::MissingButton);
}

#[test]
fn section_accessory_owns_parent_identity_and_absent_optional_fields_stay_absent() {
    let mut m = message();
    m.channel = None; // History responses inherit their canonical bag's channel.
    m.team = Some("T_EXTERNAL_AUTHOR".into());
    m.blocks = vec![json!({"type":"section","block_id":"review","accessory":{
        "type":"button","block_id":"spoof","action_id":"approve",
        "text":{"type":"plain_text","text":"Approve"}
    }})];
    let saved = capture(&m).unwrap();
    let current = saved.revalidate(&owner(), &m).unwrap();
    assert_eq!(current.action()["block_id"], "review");
    assert!(current.action().get("value").is_none());
    assert!(current.action().get("style").is_none());
    assert!(current.action().get("confirm").is_none());
}

#[test]
fn cloned_captures_share_immutable_revision_and_reject_later_edits() {
    let mut message = message();
    let original = capture(&message).unwrap();
    let copied = original.clone();
    assert!(Arc::ptr_eq(&original.revision, &copied.revision));
    assert!(copied.revalidate(&owner(), &message).is_ok());
    message.blocks[0]["elements"][0]["value"] = json!("changed");
    assert!(matches!(
        original.revalidate(&owner(), &message),
        Err(MessageActionError::StaleMessage)
    ));
    assert!(matches!(
        copied.revalidate(&owner(), &message),
        Err(MessageActionError::StaleMessage)
    ));
    assert!(
        capture(&message)
            .unwrap()
            .revalidate(&owner(), &message)
            .is_ok()
    );
}
