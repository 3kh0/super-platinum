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
        {"type":"input","block_id":"member","dispatch_action":true,"element":{"type":"users_select","action_id":"a","initial_user":"UINITIAL1","placeholder":{"type":"plain_text","text":"Ignored"}}},
        {"type":"input","block_id":"sibling","optional":true,"element":{"type":"plain_text_input","action_id":"a"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send"}}]}]});
    let form = HomeForm::parse(&raw).unwrap();
    (
        View::decode(&home, raw).unwrap(),
        form,
        InputKey {
            block_id: "member".into(),
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
fn queued_user_preserves_action_with_latest_required_null_and_sibling_form() {
    let (view, mut form, key, workspace) = setup();
    let event = form
        .user_input(&key, Some("USELECT01".into()))
        .unwrap()
        .unwrap();
    let queued = view.static_action(&event, "U1").unwrap();
    assert!(form.user_input(&key, None).unwrap().is_none());
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
                "user-token",
                Some("TCONTEXT"),
            )
            .unwrap(),
    );
    let actions: Value = serde_json::from_str(&sent["actions"]).unwrap();
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        actions,
        json!([{"type":"users_select","block_id":"member","action_id":"a","selected_user":"USELECT01","initial_user":"UINITIAL1"}])
    );
    assert_eq!(state["values"]["member"]["a"]["selected_user"], Value::Null);
    assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
    assert_eq!(sent["service_team_id"], "TCONTEXT");
    assert_eq!(sent["service_id"], "B1");
    let sent = fields(
        view.button_with_form("buttons", "send", "U1", &form)
            .unwrap()
            .prepare(&SlackClient::default(), &workspace, &view, "button-token")
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(state["values"]["member"]["a"]["selected_user"], Value::Null);
}

#[test]
fn user_delivery_refuses_foreign_source_user_and_native_or_public_state_corruption() {
    let (view, mut form, key, workspace) = setup();
    let event = form
        .user_input(&key, Some("USELECT01".into()))
        .unwrap()
        .unwrap();
    let queued = view.static_action(&event, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["blocks"][0]["element"]["initial_user"] = json!("UNEWINIT1");
    let changed = View::decode(&view.home, raw).unwrap();
    assert!(changed.static_action(&event, "U1").is_err());
    assert!(
        queued
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &changed,
                &form,
                "token",
                None
            )
            .is_err()
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
                "token",
                None
            )
            .is_err()
    );
    for mode in 0..3 {
        let mut corrupt = form.clone();
        match mode {
            0 => corrupt
                .set_native_error(&key, String::new(), "Native unsafe".into())
                .unwrap(),
            1 => corrupt.fields.get_mut(&key).unwrap().value = InputValue::User(None),
            _ => {
                corrupt.fields.get_mut(&key).unwrap().kind =
                    crate::block_actions::form::InputKind::Time { timezone: None }
            }
        }
        assert!(
            queued
                .prepare_with_context(
                    &SlackClient::default(),
                    &workspace,
                    &view,
                    &corrupt,
                    "token",
                    None
                )
                .is_err()
        );
    }
}
