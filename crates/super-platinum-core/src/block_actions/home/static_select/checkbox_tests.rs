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
        {"type":"input","block_id":"choice","dispatch_action":true,"element":{"type":"checkboxes","action_id":"a","options":[option("a"),option("b")]}},
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
fn checkbox_queued_action_is_immutable_while_required_empty_and_siblings_are_latest() {
    let (view, mut form, key, workspace) = setup();
    let mut local = form.checkbox_selection(&key).unwrap();
    let event = form
        .checkbox_input(&key, &mut local, 1, true)
        .unwrap()
        .unwrap();
    let queued = view.static_action(&event, "U1").unwrap();
    assert!(
        form.checkbox_input(&key, &mut local, 1, false)
            .unwrap()
            .is_none()
    );
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
                "checkbox-token",
                Some("TCONTEXT"),
            )
            .unwrap(),
    );
    let action: Value = serde_json::from_str(&sent["actions"]).unwrap();
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        action,
        json!([{"type":"checkboxes","block_id":"choice","action_id":"a","selected_options":[option("b")]}])
    );
    assert_eq!(
        state["values"]["choice"]["a"]["selected_options"],
        json!([])
    );
    assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
    assert_eq!(sent["service_team_id"], "TCONTEXT");
    assert_eq!(sent["service_id"], "B1");
    assert!(
        view.button_with_form("buttons", "send", "U1", &form)
            .is_ok(),
        "Required checkbox empty does not block ordinary Home Button"
    );
    let mut foreign = workspace.clone();
    foreign.user_id = "U2".into();
    assert!(
        queued
            .prepare_with_context(
                &SlackClient::default(),
                &foreign,
                &view,
                &form,
                "checkbox-token",
                None
            )
            .is_err()
    );
    let mut changed = view.raw().clone();
    changed["hash"] = json!("new");
    let changed = View::decode(&view.home, changed).unwrap();
    assert!(changed.static_action(&event, "U1").is_err());
    assert!(
        queued
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &changed,
                &form,
                "checkbox-token",
                None
            )
            .is_err()
    );
}

#[test]
fn hidden_old_checkbox_descriptors_reach_current_button_and_new_action_without_recanonicalizing() {
    let (view, mut form, key, workspace) = setup();
    let mut local = form.checkbox_selection(&key).unwrap();
    let old_event = form
        .checkbox_input(&key, &mut local, 1, true)
        .unwrap()
        .unwrap();
    let mut raw = view.raw().clone();
    raw["blocks"][0]["element"]["options"] = json!([option("a")]);
    raw["hash"] = json!("new");
    form.reconcile(&raw).unwrap();
    let current = View::decode(&view.home, raw).unwrap();
    assert!(current.static_action(&old_event, "U1").is_err());
    let event = form
        .checkbox_input(&key, &mut local, 0, true)
        .unwrap()
        .unwrap();
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
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        actions[0]["selected_options"],
        json!([option("b"), option("a")])
    );
    assert_eq!(
        state["values"]["choice"]["a"]["selected_options"],
        json!([option("b"), option("a")])
    );
    let button = current
        .button_with_form("buttons", "send", "U1", &form)
        .unwrap();
    let sent = fields(
        button
            .prepare(
                &SlackClient::default(),
                &workspace,
                &current,
                "button-token",
            )
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        state["values"]["choice"]["a"]["selected_options"],
        json!([option("b"), option("a")])
    );
}
