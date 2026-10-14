use super::*;
use crate::block_actions::{form::InputKey, message_state::MessageInputs};
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
    serde_json::from_value(json!({"channel":"C1","ts":"1719800000.000001","bot_id":"B1","blocks":[
 {"type":"section","block_id":"section","accessory":{"type":"channels_select","action_id":"empty"}},
 {"type":"actions","block_id":"channels","elements":[{"type":"channels_select","action_id":"pick","initial_channel":"C00000001","response_url_enabled":true,"confirm":{"title":{"type":"plain_text","text":"Change?"},"text":{"type":"mrkdwn","text":"Apply *channel*?"},"confirm":{"type":"plain_text","text":"Apply"},"deny":{"type":"plain_text","text":"Cancel"}}},{"type":"channels_select","action_id":"empty"},{"type":"button","action_id":"submit","text":{"type":"plain_text","text":"Submit"}}]}]})).unwrap()
}
fn key() -> InputKey {
    InputKey {
        block_id: "channels".into(),
        action_id: "pick".into(),
    }
}
fn capture(m: &Message, id: Option<&str>) -> Result<CapturedMessageChannel, MessageActionError> {
    CapturedMessageChannel::capture(owner(), m, "channels", "pick", id)
}
#[test]
fn channel_initial_full_state_and_committed_response_flag_are_distinct() {
    let m = message();
    let mut inputs = MessageInputs::parse(&m).unwrap();
    assert_eq!(inputs.values().unwrap(), None);
    inputs.initialize();
    let initial = inputs.values().unwrap().unwrap();
    assert_eq!(
        initial["section"]["empty"],
        json!({"type":"channels_select"})
    );
    assert!(initial["channels"].get("empty").is_none());
    assert_eq!(
        initial["channels"]["pick"],
        json!({"type":"channels_select","selected_channel":"C00000001"})
    );
    inputs.commit(&key(), Some("C00000002")).unwrap();
    assert_eq!(
        inputs.values().unwrap().unwrap()["channels"]["pick"],
        json!({"type":"channels_select","selected_channel":"C00000002","response_url_enabled":true})
    );
    let before = inputs.clone();
    for id in [
        None,
        Some(""),
        Some("U00000001"),
        Some("D00000001"),
        Some("C1"),
    ] {
        assert!(inputs.commit(&key(), id).is_err());
        assert_eq!(inputs, before);
    }
}
#[test]
fn channel_capture_confirmation_and_exact_state_wire() {
    let m = message();
    let c = capture(&m, Some("C00000002")).unwrap();
    let current = c.revalidate(&owner(), &m).unwrap();
    assert_eq!(
        current.action(),
        &json!({"type":"channels_select","block_id":"channels","action_id":"pick","selected_channel":"C00000002","initial_channel":"C00000001","response_url_enabled":true,"confirm":m.blocks[1]["elements"][0]["confirm"]})
    );
    let mut inputs = MessageInputs::parse(&m).unwrap();
    inputs.commit(&key(), Some("C00000002")).unwrap();
    let workspace = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    };
    let client = crate::slack::SlackClient::default();
    let values = inputs.values().unwrap().unwrap();
    let request = crate::slack::api::block_action_channel(
        &client,
        &workspace,
        &current,
        "A1",
        "T1",
        "synthetic",
        Some(&values),
    )
    .unwrap();
    assert!(!request.retry_safe());
    assert!(crate::slack::api::block_action_channel(&client,&workspace,&current,"A1","T1","synthetic",Some(&json!({"channels":{"pick":{"type":"conversations_select","selected_channel":"C00000002"}}}))).is_err());
    assert!(capture(&m, None).is_err());
}
#[test]
fn channel_specialization_malformed_initial_and_stale_confirmation_fail_closed() {
    let m = message();
    let c = capture(&m, Some("C00000002")).unwrap();
    for name in [
        "filter",
        "default_to_current_conversation",
        "requested_scope",
        "service_team_id",
    ] {
        let mut bad = m.clone();
        bad.blocks[1]["elements"][0][name] = json!(true);
        assert!(MessageInputs::parse(&bad).is_err());
        assert!(capture(&bad, Some("C00000002")).is_err());
    }
    for value in [json!(7), json!("U00000001"), json!("D00000001")] {
        let mut bad = m.clone();
        bad.blocks[1]["elements"][0]["initial_channel"] = value;
        assert!(MessageInputs::parse(&bad).is_err());
    }
    for case in 0..4 {
        let mut changed = m.clone();
        match case {
            0 => changed.blocks[1]["elements"][0]["initial_channel"] = json!("C00000003"),
            1 => changed.blocks[1]["elements"][0]["confirm"]["text"]["text"] = json!("changed"),
            2 => changed.bot_id = Some("B2".into()),
            _ => changed.blocks.clear(),
        };
        assert!(c.revalidate(&owner(), &changed).is_err());
    }
    let mut other = owner();
    other.account_epoch += 1;
    assert!(c.revalidate(&other, &m).is_err());
}
#[test]
fn channel_display_reset_and_control_type_replacement_preserve_wire_without_cross_hydration() {
    let mut m = message();
    let mut inputs = MessageInputs::parse(&m).unwrap();
    inputs.commit(&key(), Some("C00000002")).unwrap();
    m.blocks[1]["elements"][0]["initial_channel"] = json!("C00000003");
    inputs.reconcile(&m).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        form::InputValue::Channel(Some("C00000003".into()))
    );
    assert_eq!(inputs.saved_channel(&key()), Some(&json!("C00000002")));
    m.blocks[1]["elements"][0] = json!({"type":"conversations_select","action_id":"pick","initial_conversation":"U00000001"});
    inputs.reconcile(&m).unwrap();
    assert_eq!(
        inputs.fields[&key()].value,
        form::InputValue::Conversation(Some("U00000001".into()))
    );
    assert!(inputs.saved_conversation(&key()).is_none());
}
