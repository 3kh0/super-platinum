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
        {"type":"input","block_id":"member","dispatch_action":true,"element":{"type":"channels_select","action_id":"a","initial_channel":"CINITIAL1","response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Ignored"}}},
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
fn queued_channel_preserves_action_with_latest_required_null_and_sibling_form() {
    let (view, mut form, key, workspace) = setup();
    let event = form
        .channel_input(&key, Some("CSELECT01".into()))
        .unwrap()
        .unwrap();
    let queued = view.static_action(&event, "U1").unwrap();
    assert!(form.channel_input(&key, None).unwrap().is_none());
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
                "channel-token",
                Some("TCONTEXT"),
            )
            .unwrap(),
    );
    let actions: Value = serde_json::from_str(&sent["actions"]).unwrap();
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        actions,
        json!([{"type":"channels_select","block_id":"member","action_id":"a","selected_channel":"CSELECT01","initial_channel":"CINITIAL1","response_url_enabled":true}])
    );
    assert_eq!(
        state["values"]["member"]["a"],
        json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
    );
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
    assert_eq!(
        state["values"]["member"]["a"],
        json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
    );
}

#[test]
fn channel_delivery_refuses_foreign_source_user_and_native_or_public_state_corruption() {
    let (view, mut form, key, workspace) = setup();
    let event = form
        .channel_input(&key, Some("CSELECT01".into()))
        .unwrap()
        .unwrap();
    let queued = view.static_action(&event, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["blocks"][0]["element"]["initial_channel"] = json!("CNEWINIT1");
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
            1 => corrupt.fields.get_mut(&key).unwrap().value = InputValue::Channel(None),
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

#[test]
fn channel_button_keeps_null_and_old_response_bit_after_fresh_display_fallback() {
    let (view, mut form, key, workspace) = setup();
    let pristine = fields(
        view.button_with_form("buttons", "send", "U1", &form)
            .unwrap()
            .prepare(&SlackClient::default(), &workspace, &view, "initial-token")
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&pristine["state"]).unwrap();
    assert_eq!(
        state["values"]["member"]["a"],
        json!({"type":"channels_select","selected_channel":"CINITIAL1"})
    );
    let parent = form.channel_parent(&key).unwrap();
    assert!(form.channel_input(&key, None).unwrap().is_none());
    let mut raw = view.raw().clone();
    raw["blocks"][0]["element"]["initial_channel"] = json!("CNEWINIT1");
    raw["blocks"][0]["element"]["response_url_enabled"] = json!(false);
    form.reconcile_with_lookup_parents(&raw, &[], &[], &[], &[], &[parent])
        .unwrap();
    assert_eq!(
        form.channel_seed(&key).unwrap().as_deref(),
        Some("CNEWINIT1")
    );
    let current = View::decode(&view.home, raw).unwrap();
    let sent = fields(
        current
            .button_with_form("buttons", "send", "U1", &form)
            .unwrap()
            .prepare(
                &SlackClient::default(),
                &workspace,
                &current,
                "fresh-button",
            )
            .unwrap(),
    );
    let state: Value = serde_json::from_str(&sent["state"]).unwrap();
    assert_eq!(
        state["values"]["member"]["a"],
        json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
    );
    let event = form
        .channel_input(&key, Some("CSELECT02".into()))
        .unwrap()
        .unwrap();
    assert!(event.action().get("response_url_enabled").is_none());
    assert_eq!(event.action()["initial_channel"], "CNEWINIT1");
    assert!(
        form.state().unwrap()["values"]["member"]["a"]
            .get("response_url_enabled")
            .is_none()
    );
}
