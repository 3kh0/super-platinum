use super::*;
use crate::block_actions::form::{
    InputKey, InputValue,
    home::{EmailMount, EmailTrigger},
};
use crate::slack::client::RequestBody;
fn setup() -> (View, HomeForm, InputKey, WorkspaceSession) {
    let home = Home {
        channel: "D1".into(),
        app: "A1".into(),
        installed_team: "T2".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    let raw = json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","blocks":[
        {"type":"input","block_id":"email","dispatch_action":true,"element":{"type":"email_text_input","action_id":"a","initial_value":"initial@example.com","placeholder":{"type":"plain_text","text":"Contact"},"dispatch_action_config":{"trigger_actions_on":["on_character_entered","on_enter_pressed"]}}},
        {"type":"input","block_id":"notes","optional":true,"element":{"type":"plain_text_input","action_id":"a"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send"},"value":"send"}]}]});
    let form = HomeForm::parse(&raw).unwrap();
    let view = View::decode(&home, raw).unwrap();
    (
        view,
        form,
        InputKey {
            block_id: "email".into(),
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
fn fields(prepared: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    assert!(!prepared.retry_safe());
    let RequestBody::Form(fields) = prepared.body else {
        panic!("form")
    };
    fields.into_iter().collect()
}
#[test]
fn email_queued_literal_uses_latest_invalid_parent_and_sibling() {
    let (view, mut form, key, w) = setup();
    let event = form
        .email_input(&key, "Queued+Tag@Example.COM".into(), EmailTrigger::Change)
        .unwrap()
        .unwrap();
    let action = view.text_action(&event, "U1").unwrap();
    for value in ["invalid", ""] {
        assert!(
            form.email_input(&key, value.into(), EmailTrigger::Enter)
                .unwrap()
                .is_none()
        );
        form.set(
            &InputKey {
                block_id: "notes".into(),
                action_id: "a".into(),
            },
            InputValue::Text("Latest sibling".into()),
        )
        .unwrap();
        let request = fields(
            action
                .prepare_with_context(
                    &SlackClient::default(),
                    &w,
                    &view,
                    &form,
                    "token",
                    Some("TCONTEXT"),
                )
                .unwrap(),
        );
        assert_eq!(request["service_team_id"], "TCONTEXT");
        assert_eq!(request["service_id"], "B1");
        let actions: Value = serde_json::from_str(&request["actions"]).unwrap();
        assert_eq!(
            actions[0],
            json!({"type":"email_text_input","block_id":"email","action_id":"a","value":"Queued+Tag@Example.COM"})
        );
        let state: Value = serde_json::from_str(&request["state"]).unwrap();
        assert_eq!(state["values"]["email"]["a"]["value"], value);
        assert_eq!(state["values"]["notes"]["a"]["value"], "Latest sibling");
    }
}
#[test]
fn email_current_source_account_and_native_safety_gate_request_preparation() {
    let (view, mut form, key, w) = setup();
    let event = form
        .email_input(&key, "a@example.com".into(), EmailTrigger::Enter)
        .unwrap()
        .unwrap();
    let action = view.text_action(&event, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["hash"] = json!("replacement");
    let changed = View::decode(&view.home, raw).unwrap();
    assert!(changed.text_action(&event, "U1").is_err());
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &w, &changed, &form, "token", None)
            .is_err()
    );
    let mut foreign = w.clone();
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
    form.set_native_error(&key, "unseen".into(), "Native custody".into())
        .unwrap();
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &w, &view, &form, "token", None)
            .is_err()
    );
}
#[test]
fn email_mount_never_requires_an_action_to_make_button_parent_available() {
    let (view, mut form, key, _w) = setup();
    assert_eq!(
        form.email_mount(&key, EmailMount::NewParent).unwrap(),
        "initial@example.com"
    );
    assert_eq!(
        form.state().unwrap()["values"]["email"]["a"],
        json!({"type":"email_text_input"})
    );
    assert!(
        view.button_with_form("buttons", "send", "U1", &form)
            .is_ok()
    );
    form.email_mount(&key, EmailMount::RetainedParent).unwrap();
    assert_eq!(
        form.state().unwrap()["values"]["email"]["a"]["value"],
        "initial@example.com"
    );
    assert!(
        view.button_with_form("buttons", "send", "U1", &form)
            .is_ok()
    );
}
