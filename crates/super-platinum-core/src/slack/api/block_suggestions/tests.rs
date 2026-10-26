use super::*;
use crate::block_actions::{external::ExternalOption, message_state::MessageInputs};
use crate::slack::client::RequestBody;
use std::collections::BTreeMap;
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
fn key() -> InputKey {
    InputKey {
        block_id: "b".into(),
        action_id: "a".into(),
    }
}
fn owner() -> MessageActionOwner {
    MessageActionOwner {
        account_epoch: 1,
        self_user_id: "U1".into(),
        team_id: "T1".into(),
        channel_id: "C1".into(),
        message_ts: "1719700000.000001".into(),
    }
}
fn message() -> Message {
    serde_json::from_value(json!({"ts":owner().message_ts,"bot_id":"B1","function_execution_id":"Fx1","blocks":[{"type":"actions","block_id":"b","elements":[{"type":"external_select","action_id":"a"}]}]})).unwrap()
}
fn fields(request: PreparedRequest) -> BTreeMap<String, String> {
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let mut fields: BTreeMap<String, String> = fields.into_iter().collect();
    assert_eq!(fields.remove("token").as_deref(), Some("fixture-token"));
    fields
}
fn modal() -> ModalView {
    ModalView {
        id: "V1".into(),
        app_id: "A1".into(),
        kind: "modal".into(),
        previous_view_id: None,
        origin_conversation: None,
        viewed_conversation: None,
        needs_content: false,
        payload: Some(
            json!({"id":"V1","type":"modal","app_id":"A1","bot_id":"B2","app_installed_team_id":"T2","title":{"type":"plain_text","text":"Form"},"blocks":[{"type":"input","block_id":"b","optional":true,"label":{"type":"plain_text","text":"Choice"},"element":{"type":"external_select","action_id":"a"}}]}),
        ),
    }
}
#[test]
fn message_suggestions_exact_route_query_and_no_state_retry() {
    let context = SuggestionsContext::message(&owner(), &message(), &key(), "A1", "T2").unwrap();
    let request = block_suggestions(
        &SlackClient::default(),
        &workspace(),
        &context,
        "  \u{1F600}&x=1 ",
    )
    .unwrap();
    assert!(request.url.contains("/api/blocks.suggestions"));
    let fields = fields(request);
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "action_id",
            "app_id",
            "block_id",
            "container",
            "function_execution_id",
            "service_id",
            "service_team_id",
            "value"
        ]
    );
    assert_eq!(fields["value"], "  \u{1F600}&x=1 ");
    assert_eq!(fields["service_id"], "B1");
    assert_eq!(fields["service_team_id"], "T2");
    assert_eq!(fields["function_execution_id"], "Fx1");
    assert_eq!(
        serde_json::from_str::<Value>(&fields["container"]).unwrap(),
        json!({"type":"message","message_ts":owner().message_ts,"channel_id":"C1","is_ephemeral":false})
    );
    let mut wrong = workspace();
    wrong.user_id = "U2".into();
    assert!(block_suggestions(&SlackClient::default(), &wrong, &context, "").is_err());
    assert!(
        block_suggestions(
            &SlackClient::default(),
            &workspace(),
            &context,
            &"\u{1F600}".repeat(MAX_QUERY_UNITS / 2)
        )
        .is_ok()
    );
    assert!(
        block_suggestions(
            &SlackClient::default(),
            &workspace(),
            &context,
            &"\u{1F600}".repeat(MAX_QUERY_UNITS / 2 + 1)
        )
        .is_err()
    );
}
#[test]
fn modal_installed_team_route_is_required_and_never_current_team_fallback() {
    let scope = ModalScope {
        account_epoch: 1,
        team: "T1".into(),
        realtime_generation: 1,
    };
    let mut view = modal();
    let context = SuggestionsContext::modal(&scope, "U1", &view, &key()).unwrap();
    let fields =
        fields(block_suggestions(&SlackClient::default(), &workspace(), &context, "").unwrap());
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        vec![
            "action_id",
            "app_id",
            "block_id",
            "container",
            "service_id",
            "service_team_id",
            "value"
        ]
    );
    assert_eq!(fields["service_team_id"], "T2");
    assert_eq!(fields["service_id"], "B2");
    assert_eq!(
        serde_json::from_str::<Value>(&fields["container"]).unwrap(),
        json!({"type":"view","view_id":"V1"})
    );
    for name in ["bot_id", "app_installed_team_id", "app_id", "id"] {
        let mut changed = view.clone();
        changed
            .payload
            .as_mut()
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(name);
        assert!(SuggestionsContext::modal(&scope, "U1", &changed, &key()).is_err());
    }
    view.needs_content = true;
    assert!(SuggestionsContext::modal(&scope, "U1", &view, &key()).is_err());
}
#[test]
fn external_action_requires_exact_committed_raw_and_current_owner() {
    let mut message = message();
    let owner = owner();
    let option = ExternalOption::parse(
        &json!({"text":{"type":"plain_text","text":"Remote","emoji":false},"value":"remote","opaque":{"kept":[1,false,null]},"url":"https://example.test/not-navigation"}),
    )
    .unwrap();
    let captured =
        CapturedMessageExternal::capture(owner.clone(), &message, "b", "a", Some(&option)).unwrap();
    assert_eq!(captured.selected_option(), Some(option.raw()));
    let current = captured.revalidate(&owner, &message).unwrap();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    inputs.commit_external(&key(), Some(&option)).unwrap();
    let mut state = inputs.values().unwrap().unwrap();
    assert!(
        super::super::block_action_external(
            &SlackClient::default(),
            &workspace(),
            &current,
            "A1",
            "T2",
            "token",
            Some(&state)
        )
        .is_ok()
    );
    state["b"]["a"]["selected_option"]["opaque"]["kept"][0] = json!(2);
    assert!(
        super::super::block_action_external(
            &SlackClient::default(),
            &workspace(),
            &current,
            "A1",
            "T2",
            "token",
            Some(&state)
        )
        .is_err()
    );
    state = inputs.values().unwrap().unwrap();
    state["b"]["a"]["selected_option"]["text"]["text"] = json!("Forged metadata");
    assert!(
        super::super::block_action_external(
            &SlackClient::default(),
            &workspace(),
            &current,
            "A1",
            "T2",
            "token",
            Some(&state)
        )
        .is_err()
    );
    message.blocks[0]["elements"][0]["min_query_length"] = json!(0);
    assert!(captured.revalidate(&owner, &message).is_err());
    let mut wrong = owner.clone();
    wrong.account_epoch += 1;
    assert!(captured.revalidate(&wrong, &message).is_err());
}
#[test]
fn decode_needs_positive_envelope_and_preserves_empty_groups_precedence() {
    assert_eq!(
        decode_external_suggestions(&json!({"ok":true,"option_groups":[],"options":[{}]})).unwrap(),
        ExternalSuggestions::Groups(vec![])
    );
    for raw in [
        json!({"options":[]}),
        json!({"ok":false,"error":"app_rejected"}),
        json!({"ok":true,"options":[{}]}),
        json!([]),
    ] {
        assert!(decode_external_suggestions(&raw).is_err());
    }
}

#[test]
fn multi_external_suggestions_share_exact_routing_and_no_retry() {
    let mut message = message();
    message.blocks[0]["elements"][0]["type"] = json!("multi_external_select");
    let multi = SuggestionsContext::message(&owner(), &message, &key(), "A1", "T2").unwrap();
    let multi_fields =
        fields(block_suggestions(&SlackClient::default(), &workspace(), &multi, "query").unwrap());
    message.blocks[0]["elements"][0]["type"] = json!("external_select");
    let single = SuggestionsContext::message(&owner(), &message, &key(), "A1", "T2").unwrap();
    assert_eq!(
        multi_fields,
        fields(block_suggestions(&SlackClient::default(), &workspace(), &single, "query").unwrap())
    );
    let scope = ModalScope {
        account_epoch: 1,
        team: "T1".into(),
        realtime_generation: 1,
    };
    let mut view = modal();
    view.payload.as_mut().unwrap()["blocks"][0]["element"]["type"] = json!("multi_external_select");
    let context = SuggestionsContext::modal(&scope, "U1", &view, &key()).unwrap();
    assert_eq!(
        fields(block_suggestions(&SlackClient::default(), &workspace(), &context, "").unwrap())["service_team_id"],
        "T2"
    );
    view.payload
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("app_installed_team_id");
    assert!(SuggestionsContext::modal(&scope, "U1", &view, &key()).is_err());
}
#[test]
fn multi_external_action_checks_order_raw_metadata_owner_and_revision() {
    let mut message = message();
    message.blocks[0]["elements"][0]["type"] = json!("multi_external_select");
    message.blocks[0]["elements"][0]["initial_options"] = json!([]);
    let options:Vec<_>=["z","a"].iter().map(|value|ExternalOption::parse(&json!({"text":{"type":"plain_text","text":value,"emoji":false},"value":value,"description":{"type":"plain_text","text":"Exact"}})).unwrap()).collect();
    let captured =
        CapturedMessageExternalOptions::capture(owner(), &message, "b", "a", &options).unwrap();
    assert_eq!(
        captured.selected_options(),
        options.iter().map(|o| o.raw().clone()).collect::<Vec<_>>()
    );
    let current = captured.revalidate(&owner(), &message).unwrap();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    inputs.commit_external_options(&key(), &options).unwrap();
    let state = inputs.values().unwrap().unwrap();
    let prepare = |state: Option<&Value>, workspace: &WorkspaceSession| {
        super::super::block_action_external_options(
            &SlackClient::default(),
            workspace,
            &current,
            "A1",
            "T2",
            "token",
            state,
        )
    };
    let request = prepare(Some(&state), &workspace()).unwrap();
    assert!(!request.retry_safe());
    assert!(prepare(None, &workspace()).is_err());
    let mut wrong = workspace();
    wrong.user_id = "other".into();
    assert!(prepare(Some(&state), &wrong).is_err());
    let mut reordered = state.clone();
    reordered["b"]["a"]["selected_options"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(prepare(Some(&reordered), &workspace()).is_err());
    let mut forged = state.clone();
    forged["b"]["a"]["selected_options"][0]["description"]["text"] = json!("Forged");
    assert!(prepare(Some(&forged), &workspace()).is_err());
    let mut wrong_owner = owner();
    wrong_owner.account_epoch += 1;
    assert!(captured.revalidate(&wrong_owner, &message).is_err());
    message.blocks[0]["elements"][0]["max_selected_items"] = json!(1);
    assert!(captured.revalidate(&owner(), &message).is_err());
    assert!(
        CapturedMessageExternalOptions::capture(owner(), &message, "b", "a", &options).is_err()
    );
}
