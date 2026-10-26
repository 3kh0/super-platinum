use super::*;
use crate::block_actions::{
    external::ExternalOption,
    form::{InputKey, InputValue},
};
use crate::slack::client::RequestBody;

fn option(label: &str) -> ExternalOption {
    ExternalOption::parse(
        &json!({"text":{"type":"plain_text","text":label,"emoji":false},"value":"same",
        "description":{"type":"plain_text","text":"Exact details"},"opaque":{"keep":true}}),
    )
    .unwrap()
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
        {"type":"input","block_id":"pick","dispatch_action":true,"element":{"type":"external_select","action_id":"a",
         "initial_option":option("Initial").raw(),"placeholder":{"type":"plain_text","text":"Find","emoji":true}}},
        {"type":"input","block_id":"notes","optional":true,"element":{"type":"plain_text_input","action_id":"a"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send"}}]}]});
    let form = HomeForm::parse(&raw).unwrap();
    (
        View::decode(&home, raw).unwrap(),
        form,
        InputKey {
            block_id: "pick".into(),
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
fn queued_exact_external_action_uses_latest_required_null_and_sibling_state() {
    let (view, mut form, key, ws) = setup();
    let event = form
        .external_input(&key, Some(option("Queued")))
        .unwrap()
        .unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    assert!(form.external_input(&key, None).unwrap().is_none());
    form.set(
        &InputKey {
            block_id: "notes".into(),
            action_id: "a".into(),
        },
        InputValue::Text(" latest ".into()),
    )
    .unwrap();
    let sent = fields(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &ws,
                &view,
                &form,
                "event-token",
                Some("TCONTEXT"),
            )
            .unwrap(),
    );
    let actions: Value = serde_json::from_str(&sent["actions"]).unwrap();
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        actions,
        json!([{"type":"external_select","block_id":"pick","action_id":"a","selected_option":option("Queued").raw(),
        "initial_option":option("Initial").raw(),"placeholder":{"type":"plain_text","text":"Find","emoji":true}}])
    );
    assert_eq!(
        state["values"]["pick"]["a"],
        json!({"type":"external_select","selected_option":null})
    );
    assert_eq!(state["values"]["notes"]["a"]["value"], "latest");
    assert_eq!(sent["service_team_id"], "TCONTEXT");
    assert_eq!(sent["service_id"], "B1");
    assert_eq!(sent["_x_reason"], "dispatch_action_to_developer");
    assert_eq!(
        form.external_seed(&key).unwrap().unwrap(),
        option("Initial")
    );
    let sent = fields(
        view.button_with_form("buttons", "send", "U1", &form)
            .unwrap()
            .prepare(&SlackClient::default(), &ws, &view, "button-token")
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(state["values"]["pick"]["a"]["selected_option"], Value::Null);
}

#[test]
fn external_delivery_refuses_changed_source_user_context_token_and_corrupt_parent() {
    let (view, mut form, key, ws) = setup();
    let event = form
        .external_input(&key, Some(option("Queued")))
        .unwrap()
        .unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["blocks"][0]["element"]["placeholder"]["text"] = json!("Changed");
    let changed = View::decode(&view.home, raw).unwrap();
    assert!(changed.static_action(&event, "U1").is_err());
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &ws, &changed, &form, "token", None)
            .is_err()
    );
    let mut wrong = ws.clone();
    wrong.user_id = "UOTHER".into();
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &wrong, &view, &form, "token", None)
            .is_err()
    );
    for token in ["", "bad token"] {
        assert!(
            action
                .prepare_with_context(&SlackClient::default(), &ws, &view, &form, token, None)
                .is_err()
        );
    }
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &ws,
                &view,
                &form,
                "token",
                Some("bad context")
            )
            .is_err()
    );
    form.fields.get_mut(&key).unwrap().value = InputValue::External(Some(option("Uncommitted")));
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &ws, &view, &form, "token", None)
            .is_err()
    );
}

mod multiple;
