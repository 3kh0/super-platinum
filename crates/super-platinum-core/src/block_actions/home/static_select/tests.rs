use super::*;
use crate::block_actions::form::{InputKey, InputValue};
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
    let raw = json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","blocks":[
        {"type":"input","block_id":"choice","dispatch_action":true,"element":{"type":"static_select","action_id":"a","options":[
        {"text":{"type":"plain_text","text":"A"},"value":"a"},{"text":{"type":"plain_text","text":"B"},"value":"b"}]}},
        {"type":"input","block_id":"sibling","optional":true,"element":{"type":"plain_text_input","action_id":"a"}}]});
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
fn queued_option_is_fixed_while_delivery_reads_latest_state_and_exact_context() {
    let (view, mut form, key, w) = setup();
    let event = form.static_input(&key, Some("b".into())).unwrap().unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    assert!(form.static_input(&key, None).unwrap().is_none());
    form.set(
        &InputKey {
            block_id: "sibling".into(),
            action_id: "a".into(),
        },
        InputValue::Text(" latest ".into()),
    )
    .unwrap();
    for route in [None, Some(""), Some("TCONTEXT")] {
        let sent = fields(
            action
                .prepare_with_context(&SlackClient::default(), &w, &view, &form, "token", route)
                .unwrap(),
        );
        assert_eq!(
            sent["service_team_id"],
            route.filter(|s| !s.is_empty()).unwrap_or("T2")
        );
        assert_eq!(sent["service_id"], "B1");
        assert_eq!(sent["_x_reason"], "dispatch_action_to_developer");
        let actions: Value = serde_json::from_str(&sent["actions"]).unwrap();
        assert_eq!(actions[0]["selected_option"]["value"], "b");
        let state: Value = serde_json::from_str(&sent["state"]).unwrap();
        assert!(state["values"]["choice"]["a"]["selected_option"].is_null());
        assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
    }
    assert!(view.button_with_form("choice", "a", "U1", &form).is_err());
}

#[test]
fn event_cannot_cross_view_account_token_context_or_unattested_form_boundaries() {
    let (view, mut form, key, w) = setup();
    let event = form.static_input(&key, Some("b".into())).unwrap().unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["hash"] = json!("replacement");
    let replacement = View::decode(&view.home, raw).unwrap();
    assert!(replacement.static_action(&event, "U1").is_err());
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &w,
                &replacement,
                &form,
                "token",
                None
            )
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
    for (token, context) in [("", None), ("bad token", None), ("token", Some("bad team"))] {
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

#[test]
fn queued_multi_action_uses_latest_invalid_array_and_sibling_state() {
    let (single, _, key, workspace) = setup();
    let mut raw = single.raw().clone();
    raw["blocks"][0]["element"]["type"] = json!("multi_static_select");
    raw["blocks"][0]["element"]["max_selected_items"] = json!(1);
    let view = View::decode(&single.home, raw.clone()).unwrap();
    let mut form = HomeForm::parse(&raw).unwrap();
    let event = form
        .multi_static_input(&key, vec!["b".into()])
        .unwrap()
        .unwrap();
    let action = view.static_action(&event, "U1").unwrap();
    form.set(
        &InputKey {
            block_id: "sibling".into(),
            action_id: "a".into(),
        },
        InputValue::Text(" latest ".into()),
    )
    .unwrap();
    for invalid in [vec!["b".into(), "a".into()], vec![]] {
        assert!(
            form.multi_static_input(&key, invalid.clone())
                .unwrap()
                .is_none()
        );
        let sent = fields(
            action
                .prepare_with_context(
                    &SlackClient::default(),
                    &workspace,
                    &view,
                    &form,
                    "multi-token",
                    Some("TCONTEXT"),
                )
                .unwrap(),
        );
        assert_eq!(sent["service_team_id"], "TCONTEXT");
        let actions: Value = serde_json::from_str(&sent["actions"]).unwrap();
        assert_eq!(actions[0]["type"], "multi_static_select");
        assert_eq!(
            actions[0]["selected_options"],
            json!([raw["blocks"][0]["element"]["options"][1]])
        );
        let state: Value = serde_json::from_str(&sent["state"]).unwrap();
        let actual: Vec<_> = state["values"]["choice"]["a"]["selected_options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| option["value"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(actual, invalid);
        assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
    }
    let mut replacement_raw = raw;
    replacement_raw["hash"] = json!("replacement");
    let replacement = View::decode(&view.home, replacement_raw).unwrap();
    assert!(replacement.static_action(&event, "U1").is_err());
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &replacement,
                &form,
                "multi-token",
                None
            )
            .is_err()
    );
    let mut foreign = workspace;
    foreign.user_id = "U2".into();
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &foreign,
                &view,
                &form,
                "multi-token",
                None
            )
            .is_err()
    );
}
