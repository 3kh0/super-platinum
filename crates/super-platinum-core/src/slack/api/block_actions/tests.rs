use std::collections::BTreeMap;

use super::*;
use crate::block_actions::validation::{
    CapturedMessageButton, CapturedMessageRadio, CapturedMessageSelect, MessageActionOwner,
};
use crate::slack::client::RequestBody;
use crate::slack::models::Message;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    }
}

fn select_message(ephemeral: bool) -> (Message, Value) {
    let selected = json!({"text":{"type":"plain_text","text":"Chosen & exact","emoji":true},
        "value":"value&scope=unchanged","description":{"type":"plain_text","text":"Raw metadata"}});
    let mut message = message(ephemeral);
    message
        .extra
        .insert("function_execution_id".into(), json!("Fx1"));
    message.blocks = vec![json!({"type":"actions","block_id":"review","elements":[{
        "type":"static_select","action_id":"choose","options":[selected],"initial_option":selected,
        "placeholder":{"type":"plain_text","text":"Choose"},
        "confirm":{"title":{"type":"plain_text","text":"Confirm"},"text":{"type":"mrkdwn","text":"Change?"},
            "confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"No"}}
    }]})];
    (message, selected)
}

#[test]
fn static_select_request_preserves_raw_action_full_state_and_message_container() {
    for ephemeral in [false, true] {
        let (message, selected) = select_message(ephemeral);
        let owner = owner();
        let captured = CapturedMessageSelect::capture(
            owner.clone(),
            &message,
            "review",
            "choose",
            Some(&selected),
        )
        .unwrap();
        let current = captured.revalidate(&owner, &message).unwrap();
        let values = json!({"review":{"choose":{"type":"static_select","selected_option":selected}},
            "other":{"untouched":{"type":"static_select","selected_option":{"text":{"type":"plain_text","text":"Retained"},"value":"old"}}}});
        let fields = fields(
            block_action_select(
                &SlackClient::default(),
                &workspace(),
                &current,
                "A1",
                "T2",
                "web-select-1",
                Some(&values),
            )
            .unwrap(),
        );
        let action = &message.blocks[0]["elements"][0];
        assert_eq!(
            serde_json::from_str::<Value>(&fields["actions"]).unwrap(),
            json!([{
                "type":"static_select","block_id":"review","action_id":"choose","selected_option":selected,
                "initial_option":action["initial_option"],"placeholder":action["placeholder"],"confirm":action["confirm"]
            }])
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["state"]).unwrap(),
            json!({"values":values})
        );
        assert_eq!(fields["service_id"], "B1");
        assert_eq!(fields["service_team_id"], "T2");
        assert_eq!(fields["function_execution_id"], "Fx1");
        assert_eq!(fields["client_token"], "web-select-1");
        let container: Value = serde_json::from_str(&fields["container"]).unwrap();
        assert_eq!(container["is_ephemeral"], ephemeral);
        assert_eq!(
            container.get("thread_ts"),
            ephemeral.then_some(&json!("1719700000.000001"))
        );
    }
}

#[test]
fn static_select_clear_requires_the_same_explicit_null_in_full_state() {
    let (message, _) = select_message(false);
    let owner = owner();
    let captured =
        CapturedMessageSelect::capture(owner.clone(), &message, "review", "choose", None).unwrap();
    let current = captured.revalidate(&owner, &message).unwrap();
    let values = json!({"review":{"choose":{"type":"static_select","selected_option":null}}});
    let fields = fields(
        block_action_select(
            &SlackClient::default(),
            &workspace(),
            &current,
            "A1",
            "T2",
            "web-clear",
            Some(&values),
        )
        .unwrap(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["actions"]).unwrap()[0]["selected_option"],
        Value::Null
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["state"]).unwrap(),
        json!({"values":values})
    );
}

#[test]
fn select_request_rejects_uncommitted_or_mismatched_state_and_wrong_workspace() {
    let (message, selected) = select_message(false);
    let owner = owner();
    let captured = CapturedMessageSelect::capture(
        owner.clone(),
        &message,
        "review",
        "choose",
        Some(&selected),
    )
    .unwrap();
    let current = captured.revalidate(&owner, &message).unwrap();
    for values in [
        None,
        Some(json!({})),
        Some(json!([])),
        Some(json!({"review":{"choose":{"type":"static_select","selected_option":null}}})),
        Some(json!({"review":{"choose":{"type":"radio_buttons","selected_option":selected}}})),
    ] {
        assert!(
            matches!(block_action_select(&SlackClient::default(),&workspace(),&current,"A1","T2","web-select",values.as_ref()),
            Err(Error::Api(code)) if code=="block_action_state_invalid")
        );
    }
    let values = json!({"review":{"choose":{"type":"static_select","selected_option":selected}}});
    let mut other = workspace();
    other.user_id = "U2".into();
    assert!(
        matches!(block_action_select(&SlackClient::default(),&other,&current,"A1","T2","web-select",Some(&values)),
        Err(Error::Api(code)) if code=="block_action_workspace_mismatch")
    );
}

fn radio_message(ephemeral: bool) -> (Message, Value) {
    let selected = json!({"text":{"type":"plain_text","text":"Chosen & exact","emoji":true},
        "value":"value&scope=unchanged","description":{"type":"plain_text","text":"Raw metadata"}});
    let mut message = message(ephemeral);
    message
        .extra
        .insert("function_execution_id".into(), json!("Fx1"));
    message.blocks = vec![json!({"type":"actions","block_id":"review","elements":[{
        "type":"radio_buttons","action_id":"choose","options":[selected],"initial_option":selected,
        "placeholder":{"type":"plain_text","text":"Choose"},
        "confirm":{"title":{"type":"plain_text","text":"Confirm"},"text":{"type":"mrkdwn","text":"Change?"},
            "confirm":{"type":"plain_text","text":"Yes"},"deny":{"type":"plain_text","text":"No"}}
    }]})];
    (message, selected)
}

#[test]
fn radio_request_preserves_raw_action_full_state_and_message_container() {
    for ephemeral in [false, true] {
        let (message, selected) = radio_message(ephemeral);
        let owner = owner();
        let captured = CapturedMessageRadio::capture(
            owner.clone(),
            &message,
            "review",
            "choose",
            Some(&selected),
        )
        .unwrap();
        let current = captured.revalidate(&owner, &message).unwrap();
        let values = json!({"review":{"choose":{"type":"radio_buttons","selected_option":selected}},
            "other":{"untouched":{"type":"radio_buttons","selected_option":{"text":{"type":"plain_text","text":"Retained"},"value":"old"}}}});
        let fields = fields(
            block_action_radio(
                &SlackClient::default(),
                &workspace(),
                &current,
                "A1",
                "T2",
                "web-select-1",
                Some(&values),
            )
            .unwrap(),
        );
        let action = &message.blocks[0]["elements"][0];
        assert_eq!(
            serde_json::from_str::<Value>(&fields["actions"]).unwrap(),
            json!([{
                "type":"radio_buttons","block_id":"review","action_id":"choose","selected_option":selected,
                "initial_option":action["initial_option"],"confirm":action["confirm"]
            }])
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["state"]).unwrap(),
            json!({"values":values})
        );
        assert_eq!(fields["service_id"], "B1");
        assert_eq!(fields["service_team_id"], "T2");
        assert_eq!(fields["function_execution_id"], "Fx1");
        assert_eq!(fields["client_token"], "web-select-1");
        let container: Value = serde_json::from_str(&fields["container"]).unwrap();
        assert_eq!(container["is_ephemeral"], ephemeral);
        assert_eq!(
            container.get("thread_ts"),
            ephemeral.then_some(&json!("1719700000.000001"))
        );
    }
}

#[test]
fn radio_clear_requires_the_same_explicit_null_in_full_state() {
    let (message, _) = radio_message(false);
    let owner = owner();
    let captured =
        CapturedMessageRadio::capture(owner.clone(), &message, "review", "choose", None).unwrap();
    let current = captured.revalidate(&owner, &message).unwrap();
    let values = json!({"review":{"choose":{"type":"radio_buttons","selected_option":null}}});
    let fields = fields(
        block_action_radio(
            &SlackClient::default(),
            &workspace(),
            &current,
            "A1",
            "T2",
            "web-clear",
            Some(&values),
        )
        .unwrap(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["actions"]).unwrap()[0]["selected_option"],
        Value::Null
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["state"]).unwrap(),
        json!({"values":values})
    );
    let missing_selection = json!({"review":{"choose":{"type":"radio_buttons"}}});
    assert!(matches!(
        block_action_radio(&SlackClient::default(), &workspace(), &current, "A1", "T2", "web-clear", Some(&missing_selection)),
        Err(Error::Api(code)) if code == "block_action_state_invalid"
    ));
}

#[test]
fn radio_request_rejects_uncommitted_or_mismatched_state_and_wrong_workspace() {
    let (message, selected) = radio_message(false);
    let owner = owner();
    let captured =
        CapturedMessageRadio::capture(owner.clone(), &message, "review", "choose", Some(&selected))
            .unwrap();
    let current = captured.revalidate(&owner, &message).unwrap();
    let mut forged = selected.clone();
    forged["description"]["text"] = json!("Changed raw metadata");
    for values in [
        None,
        Some(json!({})),
        Some(json!([])),
        Some(json!({"review":{"choose":{"type":"radio_buttons","selected_option":null}}})),
        Some(json!({"review":{"choose":{"type":"static_select","selected_option":selected}}})),
        Some(json!({"review":{"choose":{"type":"radio_buttons","selected_option":forged}}})),
    ] {
        assert!(
            matches!(block_action_radio(&SlackClient::default(),&workspace(),&current,"A1","T2","web-select",values.as_ref()),
            Err(Error::Api(code)) if code=="block_action_state_invalid")
        );
    }
    let values = json!({"review":{"choose":{"type":"radio_buttons","selected_option":selected}}});
    let mut other = workspace();
    other.user_id = "U2".into();
    assert!(
        matches!(block_action_radio(&SlackClient::default(),&other,&current,"A1","T2","web-select",Some(&values)),
        Err(Error::Api(code)) if code=="block_action_workspace_mismatch")
    );
}

fn owner() -> MessageActionOwner {
    MessageActionOwner {
        account_epoch: 1,
        self_user_id: "U1".into(),
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1719800000.000001".into(),
    }
}

fn message(ephemeral: bool) -> Message {
    serde_json::from_value(json!({
        "channel":"C1", "ts":"1719800000.000001", "bot_id":"B1",
        "thread_ts":"1719700000.000001", "is_ephemeral":ephemeral,
        "blocks":[{"type":"actions", "block_id":"review", "elements":[{
            "type":"button", "action_id":"approve",
            "text":{"type":"plain_text", "text":"Approve & review"},
            "value":"item-1&scope=unchanged", "style":"primary"
        }]}]
    }))
    .unwrap()
}

fn fields(request: PreparedRequest) -> BTreeMap<String, String> {
    assert_eq!(request.method, "POST");
    assert!(
        request
            .url
            .starts_with("https://fixture.slack.com/api/blocks.actions?")
    );
    assert!(
        !request.retry_safe(),
        "app actions must not replay automatically"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form encoding")
    };
    fields.into_iter().collect()
}

#[test]
fn ordinary_thread_button_preserves_action_and_omits_ephemeral_thread_context() {
    let message = message(false);
    let owner = owner();
    let capture =
        CapturedMessageButton::capture(owner.clone(), &message, "review", "approve").unwrap();
    let current = capture.revalidate(&owner, &message).unwrap();
    let request = block_action_button(
        &SlackClient::default(),
        &workspace(),
        &current,
        "A1",
        "T2",
        "web-1719800100000",
        None,
    )
    .unwrap();
    let fields = fields(request);
    assert_eq!(fields["service_id"], "B1");
    assert_eq!(fields["app_id"], "A1");
    assert_eq!(fields["service_team_id"], "T2");
    assert_eq!(fields["client_token"], "web-1719800100000");
    assert_eq!(
        serde_json::from_str::<Value>(&fields["actions"]).unwrap(),
        json!([{
            "type":"button", "action_id":"approve", "block_id":"review",
            "text":{"type":"plain_text", "text":"Approve & review"},
            "value":"item-1&scope=unchanged", "style":"primary"
        }])
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["container"]).unwrap(),
        json!({
            "type":"message", "message_ts":"1719800000.000001", "channel_id":"C1",
            "is_ephemeral":false
        })
    );
    assert!(!fields.contains_key("state"));
    assert!(!fields.contains_key("function_execution_id"));
}

#[test]
fn ephemeral_thread_button_keeps_confirmation_function_and_current_values() {
    let mut raw = serde_json::to_value(message(true)).unwrap();
    raw["function_execution_id"] = json!("Fx1");
    let confirm = json!({
        "title":{"type":"plain_text","text":"Approve?"},
        "text":{"type":"mrkdwn","text":"Review *first*."},
        "confirm":{"type":"plain_text","text":"Approve"},
        "deny":{"type":"plain_text","text":"Cancel"}, "style":"danger"
    });
    raw["blocks"][0]["elements"][0]["confirm"] = confirm.clone();
    let message = serde_json::from_value(raw).unwrap();
    let owner = owner();
    let capture =
        CapturedMessageButton::capture(owner.clone(), &message, "review", "approve").unwrap();
    let current = capture.revalidate(&owner, &message).unwrap();
    let values = json!({"details":{"note":{"type":"plain_text_input","value":"keep"}}});
    let fields = fields(
        block_action_button(
            &SlackClient::default(),
            &workspace(),
            &current,
            "A1",
            "T2",
            "web-1719800100000",
            Some(&values),
        )
        .unwrap(),
    );
    let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
    assert_eq!(actions[0]["confirm"], confirm);
    assert_eq!(fields["function_execution_id"], "Fx1");
    let container: Value = serde_json::from_str(&fields["container"]).unwrap();
    assert_eq!(container["is_ephemeral"], true);
    assert_eq!(container["thread_ts"], "1719700000.000001");
    assert_eq!(
        serde_json::from_str::<Value>(&fields["state"]).unwrap(),
        json!({"values":values})
    );
}

#[test]
fn request_rejects_other_workspace_missing_metadata_and_nonobject_state() {
    let message = message(false);
    let owner = owner();
    let capture =
        CapturedMessageButton::capture(owner.clone(), &message, "review", "approve").unwrap();
    let current = capture.revalidate(&owner, &message).unwrap();
    let client = SlackClient::default();
    let mut session = workspace();
    session.team_id = "T9".into();
    assert!(
        matches!(block_action_button(&client, &session, &current, "A1", "T2", "web-1", None),
        Err(Error::Api(code)) if code == "block_action_workspace_mismatch")
    );
    session = workspace();
    session.user_id = "U2".into();
    assert!(block_action_button(&client, &session, &current, "A1", "T2", "web-1", None).is_err());
    for (app, team, token) in [
        ("", "T2", "web-1"),
        ("A1", " ", "web-1"),
        ("A1", "T2", ""),
        ("A1", "T2", "web-1\n"),
    ] {
        assert!(
            block_action_button(&client, &workspace(), &current, app, team, token, None).is_err()
        );
    }
    for state in [Value::Null, json!([]), json!("values"), json!(true)] {
        assert!(
            matches!(block_action_button(&client, &workspace(), &current, "A1", "T2", "web-1", Some(&state)),
            Err(Error::Api(code)) if code == "block_action_state_invalid")
        );
    }
}

#[test]
fn response_accepts_only_explicit_acknowledgement_and_preserves_plain_feedback() {
    assert_eq!(
        decode_block_action_response(&json!({"ok":true,"unrelated":"ignored"})).unwrap(),
        BlockActionResponse::Accepted
    );
    let result = decode_block_action_response(&json!({
        "ok":false, "error":"app_action_failed", "message":"<b>Review again</b>",
        "private_context":{"secret":"never retain"}
    }))
    .unwrap();
    assert_eq!(
        result,
        BlockActionResponse::Rejected(BlockActionRejection {
            error: "app_action_failed".into(),
            message: Some("<b>Review again</b>".into())
        })
    );
    assert!(!format!("{result:?}").contains("Review again"));
    assert!(!format!("{result:?}").contains("never retain"));
    assert_eq!(
        decode_block_action_response(&json!({"ok":false,"error":"unavailable"})).unwrap(),
        BlockActionResponse::Rejected(BlockActionRejection {
            error: "unavailable".into(),
            message: None
        })
    );
}

#[test]
fn response_rejects_missing_acknowledgement_and_malformed_failure_fields() {
    for value in [
        json!({}),
        json!({"ok":"true"}),
        json!({"ok":false}),
        json!({"ok":false,"error":" "}),
        json!({"ok":false,"error":{}}),
        json!({"ok":false,"error":"failed","message":[]}),
    ] {
        assert!(
            matches!(decode_block_action_response(&value), Err(Error::Api(code)) if code == "block_action_response_invalid")
        );
    }
}
