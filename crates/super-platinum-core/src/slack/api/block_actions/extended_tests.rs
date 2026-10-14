use super::*;
use crate::block_actions::{
    form::InputKey,
    message_state::MessageInputs,
    validation::{
        CapturedMessageChannels, CapturedMessageConversations, CapturedMessageOverflow,
        CapturedMessageUsers,
    },
};
use crate::slack::{client::RequestBody, models::Message};

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-a-token".into(),
    }
}
fn owner() -> MessageActionOwner {
    MessageActionOwner {
        account_epoch: 1,
        self_user_id: workspace().user_id,
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1719800000.000001".into(),
    }
}
fn message() -> Message {
    serde_json::from_value(json!({"channel":"C1","ts":owner().message_ts,"bot_id":"B1",
        "is_ephemeral":true,"thread_ts":"1719700000.000001","function_execution_id":"Fx1",
        "blocks":[{"type":"actions","block_id":"B","elements":[
            {"type":"multi_users_select","action_id":"users","initial_users":[]},
            {"type":"overflow","action_id":"more","options":[{"text":{"type":"plain_text","text":"Run"},"value":"run"}]},
            {"type":"users_select","action_id":"single","initial_user":"U00000003"}
        ]}]})).unwrap()
}
fn fields(request: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    assert!(
        !request.retry_safe(),
        "new app controls must never replay automatically"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    fields.into_iter().collect()
}

#[test]
fn users_request_has_exact_action_full_state_ephemeral_owner_and_no_replay() {
    let message = message();
    for selected in [vec![], vec!["U00000002".into(), "W00000004".into()]] {
        let captured =
            CapturedMessageUsers::capture(owner(), &message, "B", "users", &selected).unwrap();
        let current = captured.revalidate(&owner(), &message).unwrap();
        let mut model = MessageInputs::parse(&message).unwrap();
        model
            .commit_users(
                &InputKey {
                    block_id: "B".into(),
                    action_id: "users".into(),
                },
                &selected,
            )
            .unwrap();
        let values = model.values().unwrap().unwrap();
        let fields = fields(
            block_action_users(
                &SlackClient::default(),
                &workspace(),
                &current,
                "A1",
                "T_APP",
                "fixture-client-token",
                Some(&values),
            )
            .unwrap(),
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["actions"]).unwrap(),
            json!([{
            "type":"multi_users_select","block_id":"B","action_id":"users",
            "selected_users":selected,"initial_users":[]}])
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["state"]).unwrap(),
            json!({"values":values})
        );
        assert_eq!(values["B"]["single"]["selected_user"], "U00000003");
        assert_eq!(
            serde_json::from_str::<Value>(&fields["container"]).unwrap()["thread_ts"],
            "1719700000.000001"
        );
        assert_eq!(fields["function_execution_id"], "Fx1");
        for bad in [
            None,
            Some(json!({})),
            Some(json!({"B":{"users":{"type":"users_select","selected_users":selected}}})),
        ] {
            assert!(
                block_action_users(
                    &SlackClient::default(),
                    &workspace(),
                    &current,
                    "A1",
                    "T_APP",
                    "fixture-client-token",
                    bad.as_ref()
                )
                .is_err()
            );
        }
        let mut wrong = workspace();
        wrong.team_id = "T2".into();
        assert!(
            block_action_users(
                &SlackClient::default(),
                &wrong,
                &current,
                "A1",
                "T_APP",
                "fixture-client-token",
                Some(&values)
            )
            .is_err()
        );
    }
}

#[test]
fn overflow_is_stateless_but_request_retains_sibling_inputs_and_does_not_replay() {
    let message = message();
    let captured = CapturedMessageOverflow::capture(owner(), &message, "B", "more", "run").unwrap();
    let current = captured.revalidate(&owner(), &message).unwrap();
    let mut model = MessageInputs::parse(&message).unwrap();
    model.initialize();
    let values = model.values().unwrap().unwrap();
    let fields = fields(
        block_action_overflow(
            &SlackClient::default(),
            &workspace(),
            &current,
            "A1",
            "T_APP",
            "fixture-client-token",
            Some(&values),
        )
        .unwrap(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["actions"]).unwrap(),
        json!([{
        "type":"overflow","block_id":"B","action_id":"more",
        "selected_option":{"text":{"type":"plain_text","text":"Run"},"value":"run"}}])
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields["state"]).unwrap(),
        json!({"values":values})
    );
    assert!(values["B"].get("more").is_none());
    assert_eq!(values["B"]["users"]["selected_users"], json!([]));
}

#[test]
fn channels_request_has_exact_action_full_state_ephemeral_owner_and_no_replay() {
    let mut message = message();
    message.blocks[0]["elements"][0] = json!({"type":"multi_channels_select","action_id":"channels",
        "initial_channels":[],"response_url_enabled":true});
    for selected in [vec![], vec!["G00000004".into(), "C00000002".into()]] {
        let captured =
            CapturedMessageChannels::capture(owner(), &message, "B", "channels", &selected)
                .unwrap();
        let current = captured.revalidate(&owner(), &message).unwrap();
        let mut model = MessageInputs::parse(&message).unwrap();
        model
            .commit_channels(
                &InputKey {
                    block_id: "B".into(),
                    action_id: "channels".into(),
                },
                &selected,
            )
            .unwrap();
        let values = model.values().unwrap().unwrap();
        let fields = fields(
            block_action_channels(
                &SlackClient::default(),
                &workspace(),
                &current,
                "A1",
                "T_APP",
                "fixture-client-token",
                Some(&values),
            )
            .unwrap(),
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["actions"]).unwrap(),
            json!([{
            "type":"multi_channels_select","block_id":"B","action_id":"channels",
            "selected_channels":selected,"initial_channels":[]}])
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["state"]).unwrap(),
            json!({"values":values})
        );
        assert_eq!(values["B"]["single"]["selected_user"], "U00000003");
        assert_eq!(
            serde_json::from_str::<Value>(&fields["container"]).unwrap()["thread_ts"],
            "1719700000.000001"
        );
        assert_eq!(fields["function_execution_id"], "Fx1");
        for bad in [
            None,
            Some(json!({})),
            Some(json!({"B":{"channels":{"type":"channels_select","selected_channels":selected}}})),
        ] {
            assert!(
                block_action_channels(
                    &SlackClient::default(),
                    &workspace(),
                    &current,
                    "A1",
                    "T_APP",
                    "fixture-client-token",
                    bad.as_ref()
                )
                .is_err()
            );
        }
        let mut wrong = workspace();
        wrong.team_id = "T2".into();
        assert!(
            block_action_channels(
                &SlackClient::default(),
                &wrong,
                &current,
                "A1",
                "T_APP",
                "fixture-client-token",
                Some(&values)
            )
            .is_err()
        );
    }
}

#[test]
fn conversations_request_has_exact_action_full_state_ephemeral_owner_and_no_replay() {
    let mut message = message();
    message.blocks[0]["elements"][0] = json!({"type":"multi_conversations_select","action_id":"conversations",
        "initial_conversations":[],"response_url_enabled":true});
    for selected in [vec![], vec!["U00000004".into(), "D00000002".into()]] {
        let captured = CapturedMessageConversations::capture(
            owner(),
            &message,
            "B",
            "conversations",
            &selected,
        )
        .unwrap();
        let current = captured.revalidate(&owner(), &message).unwrap();
        let mut model = MessageInputs::parse(&message).unwrap();
        model
            .commit_conversations(
                &InputKey {
                    block_id: "B".into(),
                    action_id: "conversations".into(),
                },
                &selected,
            )
            .unwrap();
        let values = model.values().unwrap().unwrap();
        let fields = fields(
            block_action_conversations(
                &SlackClient::default(),
                &workspace(),
                &current,
                "A1",
                "T_APP",
                "fixture-client-token",
                Some(&values),
            )
            .unwrap(),
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["actions"]).unwrap(),
            json!([{
            "type":"multi_conversations_select","block_id":"B","action_id":"conversations",
            "selected_conversations":selected,"initial_conversations":[]}])
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["state"]).unwrap(),
            json!({"values":values})
        );
        assert_eq!(values["B"]["single"]["selected_user"], "U00000003");
        assert_eq!(
            serde_json::from_str::<Value>(&fields["container"]).unwrap()["thread_ts"],
            "1719700000.000001"
        );
        assert_eq!(fields["function_execution_id"], "Fx1");
        for bad in [
            None,
            Some(json!({})),
            Some(
                json!({"B":{"conversations":{"type":"conversations_select","selected_conversations":selected}}}),
            ),
        ] {
            assert!(
                block_action_conversations(
                    &SlackClient::default(),
                    &workspace(),
                    &current,
                    "A1",
                    "T_APP",
                    "fixture-client-token",
                    bad.as_ref()
                )
                .is_err()
            );
        }
        let mut wrong = workspace();
        wrong.team_id = "T2".into();
        assert!(
            block_action_conversations(
                &SlackClient::default(),
                &wrong,
                &current,
                "A1",
                "T_APP",
                "fixture-client-token",
                Some(&values)
            )
            .is_err()
        );
    }
}
