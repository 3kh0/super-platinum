use super::*;
use crate::block_actions::form::{InputKey, InputValue, home::TextTrigger};
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
    let raw = json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","blocks":[{"type":"input","block_id":"text","dispatch_action":true,"label":{"type":"plain_text","text":"Text"},"element":{"type":"plain_text_input","action_id":"a","max_length":4}},{"type":"input","block_id":"sibling","optional":true,"element":{"type":"plain_text_input","action_id":"a"}}]});
    let form = HomeForm::parse(&raw).unwrap();
    let view = View::decode(&home, raw).unwrap();
    (
        view,
        form,
        InputKey {
            block_id: "text".into(),
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
fn queued_action_value_is_fixed_but_delivery_uses_latest_form_and_context() {
    let (view, mut form, key, workspace) = setup();
    let event = form
        .text_input(&key, "a".into(), TextTrigger::Enter)
        .unwrap()
        .unwrap();
    let action = view.text_action(&event, "U1").unwrap();
    form.text_input(&key, " b ".into(), TextTrigger::Change)
        .unwrap();
    form.set(
        &InputKey {
            block_id: "sibling".into(),
            action_id: "a".into(),
        },
        InputValue::Text(" latest ".into()),
    )
    .unwrap();
    for context in [None, Some(""), Some("TCONTEXT1")] {
        let request = fields(
            action
                .prepare_with_context(
                    &SlackClient::default(),
                    &workspace,
                    &view,
                    &form,
                    "token",
                    context,
                )
                .unwrap(),
        );
        assert_eq!(
            request["service_team_id"],
            context.filter(|v| !v.is_empty()).unwrap_or("T2")
        );
        assert_eq!(request["service_id"], "B1");
        let sent: Value = serde_json::from_str(&request["actions"]).unwrap();
        assert_eq!(sent[0]["value"], "a");
        let state: Value = serde_json::from_str(&request["state"]).unwrap();
        assert_eq!(state["values"]["text"]["a"]["value"], "b");
        assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
    }
    assert!(view.button_with_form("text", "a", "U1", &form).is_err());
}
#[test]
fn subsequent_invalid_enter_does_not_rewrite_or_forbid_queued_valid_action() {
    let (view, mut form, key, w) = setup();
    let event = form
        .text_input(&key, "a".into(), TextTrigger::Enter)
        .unwrap()
        .unwrap();
    let action = view.text_action(&event, "U1").unwrap();
    assert!(
        form.text_input(&key, "invalid".into(), TextTrigger::Enter)
            .unwrap()
            .is_none()
    );
    let sent = fields(
        action
            .prepare_with_context(&SlackClient::default(), &w, &view, &form, "token", None)
            .unwrap(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&sent["actions"]).unwrap()[0]["value"],
        "a"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&sent["state"]).unwrap()["values"]["text"]["a"]["value"],
        "invalid"
    );
}
#[test]
fn replaced_view_account_route_and_native_error_retire_preparation() {
    let (view, mut form, key, w) = setup();
    let event = form
        .text_input(&key, "a".into(), TextTrigger::Enter)
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
    for (token, context) in [("", None), ("bad token", None), ("token", Some("T BAD"))] {
        assert!(
            action
                .prepare_with_context(&SlackClient::default(), &w, &view, &form, token, context)
                .is_err()
        );
    }
    form.set_native_error(&key, "bad".into(), "unattested".into())
        .unwrap();
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &w, &view, &form, "token", None)
            .is_err()
    );
}
