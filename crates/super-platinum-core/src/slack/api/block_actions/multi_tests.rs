use super::*;
use crate::block_actions::{
    form::InputKey,
    message_state::MessageInputs,
    validation::{CapturedMessageMulti, MessageActionOwner},
};
use crate::slack::{client::RequestBody, models::Message};
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":value},"value":value,"extra":{"kept":true}})
}
#[test]
fn multi_actions_send_exact_raw_array_and_full_siblings_for_clear_and_empty_values() {
    for kind in ["checkboxes", "multi_static_select"] {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-not-a-token".into(),
        };
        let owner = MessageActionOwner {
            account_epoch: 1,
            self_user_id: "U1".into(),
            team_id: "T1".into(),
            channel_id: "C1".into(),
            message_ts: "1719800000.000001".into(),
        };
        let message:Message=serde_json::from_value(json!({"channel":"C1","ts":owner.message_ts,"bot_id":"B1","is_ephemeral":true,"thread_ts":"1719700000.000001","function_execution_id":"Fx1","blocks":[
            {"type":"actions","block_id":"B","elements":[{"type":kind,"action_id":"choose","options":[option(""),option("b")]}]},
            {"type":"section","block_id":"other","accessory":{"type":"static_select","action_id":"single","options":[option("s")],"initial_option":option("s")}}
        ]})).unwrap();
        for selected in [vec![option(""), option("b")], vec![]] {
            let captured = CapturedMessageMulti::capture(
                owner.clone(),
                &message,
                "B",
                "choose",
                &selected,
                &[],
            )
            .unwrap();
            let current = captured.revalidate(&owner, &message).unwrap();
            let mut model = MessageInputs::parse(&message).unwrap();
            model
                .commit_multi(
                    &InputKey {
                        block_id: "B".into(),
                        action_id: "choose".into(),
                    },
                    &selected,
                    &[],
                )
                .unwrap();
            let values = model.values().unwrap().unwrap();
            let request = block_action_multi(
                &SlackClient::default(),
                &workspace,
                &current,
                "A1",
                "T_APP",
                "web-token",
                Some(&values),
            )
            .unwrap();
            assert!(!request.retry_safe());
            let RequestBody::Form(fields) = request.body else {
                panic!()
            };
            let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
            assert_eq!(
                serde_json::from_str::<Value>(&fields["actions"]).unwrap()[0]["selected_options"],
                json!(selected)
            );
            assert_eq!(
                serde_json::from_str::<Value>(&fields["state"]).unwrap(),
                json!({"values":values})
            );
            assert_eq!(values["other"]["single"]["selected_option"], option("s"));
            assert_eq!(
                serde_json::from_str::<Value>(&fields["container"]).unwrap()["thread_ts"],
                "1719700000.000001"
            );
            assert_eq!(fields["function_execution_id"], "Fx1");
            for bad in [
                None,
                Some(json!({})),
                Some(json!({"B":{"choose":{"type":"wrong","selected_options":selected}}})),
                Some(json!({"B":{"choose":{"type":kind,"selected_options":null}}})),
            ] {
                assert!(
                    block_action_multi(
                        &SlackClient::default(),
                        &workspace,
                        &current,
                        "A1",
                        "T_APP",
                        "web-token",
                        bad.as_ref()
                    )
                    .is_err()
                );
            }
            let mut other = workspace.clone();
            other.user_id = "OTHER".into();
            assert!(
                block_action_multi(
                    &SlackClient::default(),
                    &other,
                    &current,
                    "A1",
                    "T_APP",
                    "web-token",
                    Some(&values)
                )
                .is_err()
            );
        }
    }
}

#[test]
fn untouched_multi_apply_omits_selected_options_in_action_and_full_state() {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-a-token".into(),
    };
    let owner = MessageActionOwner {
        account_epoch: 1,
        self_user_id: "U1".into(),
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1719800000.000001".into(),
    };
    let message:Message=serde_json::from_value(json!({"channel":"C1","ts":owner.message_ts,"bot_id":"B1","blocks":[{"type":"actions","block_id":"B","elements":[{"type":"multi_static_select","action_id":"choose","options":[option("a")]}]}]})).unwrap();
    let captured =
        CapturedMessageMulti::capture_optional(owner.clone(), &message, "B", "choose", None, &[])
            .unwrap();
    let current = captured.revalidate(&owner, &message).unwrap();
    let mut model = MessageInputs::parse(&message).unwrap();
    model
        .commit_multi_optional(
            &InputKey {
                block_id: "B".into(),
                action_id: "choose".into(),
            },
            None,
            &[],
        )
        .unwrap();
    let values = model.values().unwrap().unwrap();
    let request = block_action_multi(
        &SlackClient::default(),
        &workspace,
        &current,
        "A1",
        "T_APP",
        "web-token",
        Some(&values),
    )
    .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    let action = serde_json::from_str::<Value>(&fields["actions"]).unwrap();
    assert!(action[0].get("selected_options").is_none());
    assert_eq!(
        serde_json::from_str::<Value>(&fields["state"]).unwrap(),
        json!({"values":{"B":{"choose":{"type":"multi_static_select"}}}})
    );
    let wrong = json!({"B":{"choose":{"type":"multi_static_select","selected_options":[]}}});
    assert!(
        block_action_multi(
            &SlackClient::default(),
            &workspace,
            &current,
            "A1",
            "T_APP",
            "web-token",
            Some(&wrong)
        )
        .is_err()
    );
}
