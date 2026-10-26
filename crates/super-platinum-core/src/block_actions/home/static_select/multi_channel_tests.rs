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
        {"type":"input","block_id":"member","dispatch_action":true,"element":{"type":"multi_channels_select","action_id":"a","initial_channels":["CINITIAL1"],"max_selected_items":2,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Ignored"}}},
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
fn queued_multi_channel_keeps_order_metadata_with_latest_invalid_parent_and_sibling() {
    let (view, mut form, key, workspace) = setup();
    let e = form
        .multi_channel_input(&key, vec!["GSELECT01".into(), "CSELECT02".into()])
        .unwrap()
        .unwrap();
    let action = view.static_action(&e, "U1").unwrap();
    for invalid in [
        vec![],
        vec!["CSELECT03".into(), "CSELECT02".into(), "CSELECT01".into()],
    ] {
        assert!(
            form.multi_channel_input(&key, invalid.clone())
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
            action
                .prepare_with_context(
                    &SlackClient::default(),
                    &workspace,
                    &view,
                    &form,
                    "token",
                    Some("TCONTEXT"),
                )
                .unwrap(),
        );
        let acts: Value = serde_json::from_str(&sent["actions"]).unwrap();
        let state: Value = serde_json::from_str(&sent["state"]).unwrap();
        assert_eq!(
            acts,
            json!([{"type":"multi_channels_select","block_id":"member","action_id":"a","selected_channels":["GSELECT01","CSELECT02"],"initial_channels":["CINITIAL1"]}])
        );
        assert_eq!(
            state["values"]["member"]["a"]["selected_channels"],
            json!(invalid)
        );
        assert_eq!(state["values"]["sibling"]["a"]["value"], "latest");
        assert_eq!(sent["service_team_id"], "TCONTEXT");
        let button = fields(
            view.button_with_form("buttons", "send", "U1", &form)
                .unwrap()
                .prepare(&SlackClient::default(), &workspace, &view, "button")
                .unwrap(),
        );
        let state: Value = serde_json::from_str(&button["state"]).unwrap();
        assert_eq!(
            state["values"]["member"]["a"]["selected_channels"],
            json!(invalid)
        );
    }
}
#[test]
fn multi_channel_delivery_refuses_stale_view_account_and_corrupt_state() {
    let (view, mut form, key, workspace) = setup();
    let e = form
        .multi_channel_input(&key, vec!["CSELECT01".into()])
        .unwrap()
        .unwrap();
    let action = view.static_action(&e, "U1").unwrap();
    let mut raw = view.raw().clone();
    raw["blocks"][0]["element"]["initial_channels"] = json!([]);
    let changed = View::decode(&view.home, raw).unwrap();
    assert!(changed.static_action(&e, "U1").is_err());
    assert!(
        action
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
    for native in [false, true] {
        let mut bad = form.clone();
        if native {
            bad.set_native_error(&key, "".into(), "Unsafe".into())
                .unwrap();
        } else {
            bad.fields.get_mut(&key).unwrap().value = InputValue::Channels(vec![]);
        }
        assert!(
            action
                .prepare_with_context(
                    &SlackClient::default(),
                    &workspace,
                    &view,
                    &bad,
                    "token",
                    None
                )
                .is_err()
        );
    }
}
