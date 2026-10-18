use super::*;

fn owner() -> MessageActionOwner {
    MessageActionOwner {
        account_epoch: 7,
        self_user_id: "U1".into(),
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1.000001".into(),
    }
}
fn message() -> Message {
    serde_json::from_value(json!({
        "ts":"1.000001","channel":"C1","bot_id":"BSLACKBOT",
        "blocks":[{"type":"actions","block_id":"logs","elements":[{
            "type":"button","action_id":WORKFLOW_LOGS_ACTION,
            "text":{"type":"plain_text","text":"View logs"},
            "value":json!({"type":"wfb","wf_id":"WF1","execution_id":"E1","opaque":[2,1]}).to_string(),
            "opaque":{"keep":true}
        }]}]
    })).unwrap()
}
fn button(m: &mut Message) -> &mut Value {
    &mut m.blocks[0]["elements"][0]
}
fn capture(m: &Message) -> Result<CapturedWorkflowLogs, MessageActionError> {
    CapturedWorkflowLogs::capture(owner(), m, "logs", WORKFLOW_LOGS_ACTION)
}

#[test]
fn slackbot_and_missing_service_identity_capture_exact_client_navigation_without_wire_action() {
    for bot in [Some("BSLACKBOT"), Some("BAPP"), None] {
        let mut m = message();
        m.bot_id = bot.map(str::to_owned);
        let captured = capture(&m).unwrap();
        assert_eq!(captured.owner(), &owner());
        assert_eq!(captured.workflow_id(), "WF1");
        assert_eq!(captured.execution_id(), Some("E1"));
        assert_eq!(captured.button(), &m.blocks[0]["elements"][0]);
        let current = captured.revalidate(&owner(), &m).unwrap();
        assert_eq!(current.owner(), captured.owner());
        assert_eq!(current.workflow_id(), "WF1");
        assert_eq!(current.execution_id(), Some("E1"));
        assert_eq!(current.button(), captured.button());
    }
}

#[test]
fn optional_execution_normalization_and_nonempty_strings_do_not_coerce_unknown_types() {
    for execution in [None, Some(Value::Null), Some(json!("")), Some(json!("E1"))] {
        let mut m = message();
        let mut raw = json!({"type":"wfb","wf_id":" WF1 "});
        if let Some(value) = execution {
            raw["execution_id"] = value;
        }
        button(&mut m)["value"] = json!(raw.to_string());
        let captured = capture(&m).unwrap();
        assert_eq!(captured.workflow_id(), " WF1 ");
        assert_eq!(
            captured.execution_id(),
            raw["execution_id"].as_str().filter(|id| !id.is_empty())
        );
    }
    for raw in [
        json!({"type":"wfb"}),
        json!({"type":"wfb","wf_id":""}),
        json!({"type":"wfb","wf_id":42}),
        json!({"type":"wfb","wf_id":null}),
        json!({"type":"wfb","wf_id":"WF1","execution_id":true}),
        json!({"type":"wfb","wf_id":"WF1","execution_id":1}),
        json!({"type":"wfb","wf_id":"WF1","execution_id":[]}),
        json!({"type":"wfb","wf_id":"WF1","execution_id":{}}),
    ] {
        let mut m = message();
        button(&mut m)["value"] = json!(raw.to_string());
        assert!(matches!(
            capture(&m),
            Err(MessageActionError::MalformedButton)
        ));
    }
}

#[test]
fn exact_action_is_reserved_from_generic_dispatch_even_when_payload_is_malformed() {
    for raw in [
        Value::Null,
        json!(17),
        json!(""),
        json!("not json"),
        json!("{}"),
        json!("[]"),
        json!(r#"{"type":"other","wf_id":"WF1"}"#),
    ] {
        let mut m = message();
        button(&mut m)["value"] = raw;
        assert!(capture(&m).is_err());
        assert!(matches!(
            CapturedMessageButton::capture(owner(), &m, "logs", WORKFLOW_LOGS_ACTION),
            Err(MessageActionError::UnsupportedButton)
        ));
    }
    let m = message();
    assert!(matches!(
        CapturedMessageButton::capture(owner(), &m, "logs", WORKFLOW_LOGS_ACTION),
        Err(MessageActionError::UnsupportedButton)
    ));
}

#[test]
fn action_prefix_and_details_neighbor_are_not_captured_or_reserved_by_this_leaf() {
    for action in [
        "workflow_error_view_logs_extra",
        "workflow_error_view_log",
        "workflow_error_view_details",
    ] {
        let mut m = message();
        m.bot_id = Some("BAPP".into());
        button(&mut m)["action_id"] = json!(action);
        assert!(matches!(
            CapturedWorkflowLogs::capture(owner(), &m, "logs", action),
            Err(MessageActionError::UnsupportedButton)
        ));
        assert!(!client_owned_action("logs", action));
    }
}

#[test]
fn duplicate_blocks_or_buttons_refuse_and_section_accessory_remains_supported() {
    let mut duplicate_block = message();
    duplicate_block
        .blocks
        .push(duplicate_block.blocks[0].clone());
    assert!(matches!(
        capture(&duplicate_block),
        Err(MessageActionError::AmbiguousButton)
    ));
    let mut duplicate_button = message();
    let item = button(&mut duplicate_button).clone();
    duplicate_button.blocks[0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(item);
    assert!(matches!(
        capture(&duplicate_button),
        Err(MessageActionError::AmbiguousButton)
    ));
    let mut section = message();
    let item = button(&mut section).clone();
    section.blocks[0] = json!({"type":"section","block_id":"logs","text":{"type":"mrkdwn","text":"Workflow failed"},"accessory":item});
    assert_eq!(capture(&section).unwrap().execution_id(), Some("E1"));
    assert!(matches!(
        CapturedWorkflowLogs::capture(owner(), &section, "missing", WORKFLOW_LOGS_ACTION),
        Err(MessageActionError::MissingButton)
    ));
}

#[test]
fn recipient_basic_button_and_confirmation_validation_are_reused() {
    let mut m = message();
    button(&mut m)["visible_to_user_ids"] = json!(["U1"]);
    button(&mut m)["confirm"] = json!({
        "title":{"type":"plain_text","text":"Open logs?"},
        "text":{"type":"mrkdwn","text":"Open this workflow"},
        "confirm":{"type":"plain_text","text":"Open"},
        "deny":{"type":"plain_text","text":"Cancel"}
    });
    let captured = capture(&m).unwrap();
    assert_eq!(
        captured.confirmation(),
        m.blocks[0]["elements"][0].get("confirm")
    );
    for change in 0..7 {
        let mut bad = m.clone();
        match change {
            0 => button(&mut bad)["visible_to_user_ids"] = json!(["UOTHER"]),
            1 => button(&mut bad)["visible_to_user_ids"] = json!("U1"),
            2 => button(&mut bad)["type"] = json!("static_select"),
            3 => button(&mut bad)["text"]["text"] = json!(""),
            4 => button(&mut bad)["third_party_auth"] = json!({}),
            5 => button(&mut bad)["confirm"]["deny"] = Value::Null,
            6 => bad.blocks[0]["block_id"] = json!("widget:logs"),
            _ => unreachable!(),
        }
        let block = if change == 6 { "widget:logs" } else { "logs" };
        assert!(
            CapturedWorkflowLogs::capture(owner(), &bad, block, WORKFLOW_LOGS_ACTION).is_err(),
            "change {change}"
        );
    }
}

#[test]
fn exact_owner_and_message_location_are_required_but_author_team_is_not_inferred() {
    let m = message();
    let captured = capture(&m).unwrap();
    for change in 0..5 {
        let mut other = owner();
        match change {
            0 => other.account_epoch += 1,
            1 => other.team_id = "T2".into(),
            2 => other.self_user_id = "U2".into(),
            3 => other.channel_id = "C2".into(),
            4 => other.message_ts = "2.000001".into(),
            _ => unreachable!(),
        }
        assert!(matches!(
            captured.revalidate(&other, &m),
            Err(MessageActionError::WrongMessage)
        ));
    }
    for change in 0..3 {
        let mut bad = m.clone();
        match change {
            0 => bad.ts = Some("2.000001".into()),
            1 => bad.channel = Some("C2".into()),
            2 => bad.ts = None,
            _ => unreachable!(),
        }
        assert!(matches!(
            capture(&bad),
            Err(MessageActionError::WrongMessage)
        ));
    }
    let mut bad_owner = owner();
    bad_owner.message_ts = "not-a-timestamp".into();
    assert!(matches!(
        CapturedWorkflowLogs::capture(bad_owner, &m, "logs", WORKFLOW_LOGS_ACTION),
        Err(MessageActionError::InvalidOwner)
    ));
    let mut external_author = m.clone();
    external_author.team = Some("TEXTERNAL".into());
    assert!(captured.revalidate(&owner(), &external_author).is_ok());
}

#[test]
fn opaque_button_and_whole_action_revision_changes_retire_navigation_capture() {
    let m = message();
    let captured = capture(&m).unwrap();
    for change in 0..7 {
        let mut changed = m.clone();
        match change {
            0 => button(&mut changed)["opaque"]["keep"] = json!(false),
            1 => {
                button(&mut changed)["value"] =
                    json!(r#"{"type":"wfb","wf_id":"WF1","execution_id":"E2"}"#)
            }
            2 => changed
                .blocks
                .push(json!({"type":"divider","block_id":"new"})),
            3 => changed.edited = Some(json!({"ts":"2.000001"})),
            4 => changed.bot_id = None,
            5 => changed.thread_ts = Some("1.000000".into()),
            6 => {
                changed.extra.insert("is_ephemeral".into(), json!(true));
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                captured.revalidate(&owner(), &changed),
                Err(MessageActionError::StaleMessage)
            ),
            "change {change}"
        );
    }
    assert_eq!(captured.button(), &m.blocks[0]["elements"][0]);
}
