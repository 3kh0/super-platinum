use super::*;
use crate::block_actions::form::{InputKey, InputValue, home::NumberTrigger};
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
        {"type":"input","block_id":"number","dispatch_action":true,"element":{"type":"number_input","action_id":"a","is_decimal_allowed":true,"initial_value":"1.00","max_value":"5","dispatch_action_config":{"trigger_actions_on":["on_character_entered","on_enter_pressed"]}}},
        {"type":"input","block_id":"sibling","optional":true,"element":{"type":"plain_text_input","action_id":"a"}}]});
    let form = HomeForm::parse(&raw).unwrap();
    let view = View::decode(&home, raw).unwrap();
    (
        view,
        form,
        InputKey {
            block_id: "number".into(),
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
#[test]
fn numeric_immutable_action_reads_latest_invalid_parent_and_sibling() {
    let (view, mut form, key, w) = setup();
    let event = form
        .number_input(&key, Some("2".into()), NumberTrigger::Change)
        .unwrap()
        .unwrap();
    let action = view.text_action(&event, "U1").unwrap();
    for value in [Some("6"), None] {
        assert!(
            form.number_input(&key, value.map(str::to_owned), NumberTrigger::Enter)
                .unwrap()
                .is_none()
        );
        form.set(
            &InputKey {
                block_id: "sibling".into(),
                action_id: "a".into(),
            },
            InputValue::Text("latest".into()),
        )
        .unwrap();
        let prepared = action
            .prepare_with_context(
                &SlackClient::default(),
                &w,
                &view,
                &form,
                "token",
                Some("TCONTEXT"),
            )
            .unwrap();
        assert!(!prepared.retry_safe());
        let RequestBody::Form(fields) = prepared.body else {
            panic!("form")
        };
        let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(fields["service_team_id"], "TCONTEXT");
        assert_eq!(fields["service_id"], "B1");
        let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
        assert_eq!(
            actions[0],
            json!({"type":"number_input","block_id":"number","action_id":"a","value":"2","initial_value":"1.00"})
        );
        let state: Value = serde_json::from_str(&fields["state"]).unwrap();
        assert_eq!(state["values"]["number"]["a"]["value"], json!(value));
        assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
    }
}
#[test]
fn numeric_event_rejects_replaced_source_account_and_native_error() {
    let (view, mut form, key, w) = setup();
    let event = form
        .number_input(&key, Some("002.50".into()), NumberTrigger::Enter)
        .unwrap()
        .unwrap();
    assert!(event.action().get("initial_value").is_none());
    let action = view.text_action(&event, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["hash"] = json!("new");
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
    form.set_native_error(&key, "unseen".into(), "Native invalid".into())
        .unwrap();
    assert!(
        action
            .prepare_with_context(&SlackClient::default(), &w, &view, &form, "token", None)
            .is_err()
    );
}
