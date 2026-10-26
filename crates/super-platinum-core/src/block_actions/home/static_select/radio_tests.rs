use super::*;
use crate::block_actions::form::{InputKey, InputValue};
use crate::slack::client::RequestBody;

fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":value},"value":value,"opaque":{"keep":value}})
}
fn setup() -> (View, HomeForm, InputKey, WorkspaceSession) {
    let home = Home {
        channel: "D1".into(),
        app: "A1".into(),
        installed_team: "T2".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    let raw = json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","blocks":[
        {"type":"input","block_id":"choice","dispatch_action":true,"element":{"type":"radio_buttons","action_id":"a","options":[option("a"),option("b"),option("")],"initial_option":option("a")}},
        {"type":"input","block_id":"sibling","optional":true,"element":{"type":"plain_text_input","action_id":"a"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send"}}]}]});
    let form = HomeForm::parse(&raw).unwrap();
    (
        View::decode(&home, raw).unwrap(),
        form,
        InputKey {
            block_id: "choice".into(),
            action_id: "a".into(),
        },
        WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Synthetic".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
        },
    )
}
fn fields(request: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    fields.into_iter().collect()
}

#[test]
fn queued_radio_action_keeps_occurrence_while_required_null_and_sibling_state_are_latest() {
    let (view, mut form, key, workspace) = setup();
    let event = form.radio_input(&key, Some(1)).unwrap().unwrap();
    let queued = view.static_action(&event, "U1").unwrap();
    assert!(form.radio_input(&key, None).unwrap().is_none());
    form.set(
        &InputKey {
            block_id: "sibling".into(),
            action_id: "a".into(),
        },
        InputValue::Text(" latest ".into()),
    )
    .unwrap();
    let sent = fields(
        queued
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &view,
                &form,
                "radio-token",
                Some("TCONTEXT"),
            )
            .unwrap(),
    );
    let action: Value = serde_json::from_str(&sent["actions"]).unwrap();
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        action,
        json!([{"type":"radio_buttons","block_id":"choice","action_id":"a","selected_option":option("b"),"initial_option":option("a")} ])
    );
    assert_eq!(
        state["values"]["choice"]["a"]["selected_option"],
        Value::Null
    );
    assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
    assert_eq!(sent["service_team_id"], "TCONTEXT");
    assert_eq!(sent["service_id"], "B1");
    let button = view
        .button_with_form("buttons", "send", "U1", &form)
        .unwrap();
    let sent = fields(
        button
            .prepare(&SlackClient::default(), &workspace, &view, "button-token")
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        state["values"]["choice"]["a"]["selected_option"],
        Value::Null
    );
}

#[test]
fn hidden_radio_descriptor_reaches_button_but_new_callback_uses_current_source() {
    let (view, mut form, key, workspace) = setup();
    let old = form.radio_input(&key, Some(1)).unwrap().unwrap();
    let queued = view.static_action(&old, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["hash"] = json!("new");
    raw["blocks"][0]["element"]["options"] = json!([option("a")]);
    form.reconcile(&raw).unwrap();
    let current = View::decode(&view.home, raw).unwrap();
    assert!(current.static_action(&old, "U1").is_err());
    assert!(
        queued
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &current,
                &form,
                "token",
                None
            )
            .is_err()
    );
    let button = current
        .button_with_form("buttons", "send", "U1", &form)
        .unwrap();
    let sent = fields(
        button
            .prepare(&SlackClient::default(), &workspace, &current, "token")
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        state["values"]["choice"]["a"]["selected_option"],
        option("b")
    );
    let event = form.radio_input(&key, Some(0)).unwrap().unwrap();
    let action = current.static_action(&event, "U1").unwrap();
    let sent = fields(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &current,
                &form,
                "token",
                None,
            )
            .unwrap(),
    );
    let actions: Value = serde_json::from_str(&sent["actions"]).unwrap();
    assert_eq!(actions[0]["selected_option"], option("a"));
    assert_eq!(sent["service_team_id"], "T2");
}

#[test]
fn radio_delivery_rejects_foreign_user_token_context_and_mutated_form() {
    let (view, mut form, key, workspace) = setup();
    let event = form.radio_input(&key, Some(2)).unwrap().unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    let mut foreign = workspace.clone();
    foreign.user_id = "U2".into();
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &foreign,
                &view,
                &form,
                "token",
                None
            )
            .is_err()
    );
    for token in ["", "has space"] {
        assert!(
            action
                .prepare_with_context(
                    &SlackClient::default(),
                    &workspace,
                    &view,
                    &form,
                    token,
                    None
                )
                .is_err()
        );
    }
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &view,
                &form,
                "token",
                Some("bad team")
            )
            .is_err()
    );
    form.fields.get_mut(&key).unwrap().value = InputValue::Selected(vec!["forged".into()]);
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &view,
                &form,
                "token",
                None
            )
            .is_err()
    );
    assert!(
        view.button_with_form("buttons", "send", "U1", &form)
            .is_err()
    );
}
