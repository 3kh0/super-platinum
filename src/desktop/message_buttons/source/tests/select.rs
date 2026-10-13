use super::*;
use super_platinum_core::{
    block_actions::{form::InputKey, message_state::MessageInputs},
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};

fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Choice {value}")},"value":value,"metadata":{"exact":true}})
}

fn with_select() -> ShellState {
    let mut shell = shell();
    let confirmation = canonical(&mut shell).blocks[0]["elements"][0]["confirm"].clone();
    canonical(&mut shell).blocks[0]["elements"].as_array_mut().unwrap().push(json!({
        "type":"static_select", "action_id":"choice", "placeholder":{"type":"plain_text","text":"Choose"},
        "options":[option("a"),option("")], "initial_option":option("a"), "confirm":confirmation
    }));
    shell
}

fn workspace(shell: &ShellState) -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-only".into(),
    }
}

fn fields(
    request: super_platinum_core::slack::client::PreparedRequest,
) -> std::collections::BTreeMap<String, String> {
    let RequestBody::Form(fields) = request.body else {
        panic!("form");
    };
    fields.into_iter().collect()
}

#[test]
fn select_capture_uses_canonical_raw_choice_confirmation_and_nullable_identity() {
    let shell = with_select();
    let source = capture(&shell);
    assert!(source.message(&shell).is_ok());
    let selected = source
        .capture_select(&shell, "review", "choice", Some(""))
        .unwrap();
    assert_eq!(selected.selected_value(), Some(Some("")));
    assert!(selected.url().is_none());
    assert!(selected.confirmation().is_some());
    assert_eq!(
        selected.metadata().source,
        BlockElementSource::UnsupportedControl {
            kind: "static_select".into()
        }
    );
    let current = selected.revalidate(&shell).unwrap();
    assert_eq!(current.action()["type"], "static_select");
    assert_eq!(current.action()["selected_option"], option(""));
    assert_eq!(current.container()["message_ts"], TS);
    assert_eq!(current.service_id(), "B1");
    assert_eq!(current.owner(), source.owner());
    assert_eq!(current.function_execution_id(), None);
    let cleared = source
        .capture_select(&shell, "review", "choice", None)
        .unwrap();
    assert_eq!(cleared.selected_value(), Some(None));
    assert_eq!(
        cleared.revalidate(&shell).unwrap().action()["selected_option"],
        Value::Null
    );
    assert_eq!(
        source
            .capture_button(&shell, "review", "approve")
            .unwrap()
            .selected_value(),
        None
    );
    assert!(
        source
            .capture_select(&shell, "review", "choice", Some("invented"))
            .is_err()
    );
}

#[test]
fn select_and_following_button_prepare_share_exact_committed_container_state() {
    let shell = with_select();
    let source = capture(&shell);
    let selected = source
        .capture_select(&shell, "review", "choice", Some(""))
        .unwrap();
    let current = selected.revalidate(&shell).unwrap();
    let mut inputs = MessageInputs::parse(source.message(&shell).unwrap()).unwrap();
    let session = workspace(&shell);
    let client = SlackClient::default();
    assert!(
        current
            .prepare(
                &client,
                &session,
                selected.app_id(),
                selected.app_team_id().unwrap(),
                "web-fixture",
                None
            )
            .is_err()
    );
    inputs
        .commit(
            &InputKey {
                block_id: "review".into(),
                action_id: "choice".into(),
            },
            Some(""),
        )
        .unwrap();
    let state = inputs.values().unwrap().unwrap();
    let select_fields = fields(
        current
            .prepare(
                &client,
                &session,
                selected.app_id(),
                selected.app_team_id().unwrap(),
                "web-fixture",
                Some(&state),
            )
            .unwrap(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&select_fields["state"]).unwrap(),
        json!({"values":state})
    );
    let button = source.capture_button(&shell, "review", "approve").unwrap();
    inputs.initialize();
    let state = inputs.values().unwrap().unwrap();
    let button_fields = fields(
        button
            .revalidate(&shell)
            .unwrap()
            .prepare(
                &client,
                &session,
                button.app_id(),
                button.app_team_id().unwrap(),
                "web-button",
                Some(&state),
            )
            .unwrap(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&button_fields["state"]).unwrap()["values"]["review"]["choice"]
            ["selected_option"],
        option("")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&button_fields["actions"]).unwrap()[0]["type"],
        "button"
    );
}

#[test]
fn select_confirmation_rejects_changed_options_and_unsupported_sibling_state() {
    let mut shell = with_select();
    let source = capture(&shell);
    let selected = source
        .capture_select(&shell, "review", "choice", Some(""))
        .unwrap();
    canonical(&mut shell).blocks[0]["elements"][1]["options"][1]["metadata"] =
        json!({"changed":true});
    assert!(source.message(&shell).is_err());
    assert_eq!(
        selected.revalidate(&shell).err(),
        Some(MessageActionError::StaleMessage)
    );
    let source = capture(&shell);
    assert!(
        source
            .capture_select(&shell, "review", "choice", Some(""))
            .is_ok()
    );
    canonical(&mut shell).blocks.push(json!({"type":"section","block_id":"other","accessory":{"type":"datepicker","action_id":"date"}}));
    let source = capture(&shell);
    assert!(
        source
            .capture_select(&shell, "review", "choice", Some(""))
            .is_err()
    );
    assert!(source.capture_button(&shell, "review", "approve").is_err());
    assert!(source.lookup_target(&shell).is_none());
}
