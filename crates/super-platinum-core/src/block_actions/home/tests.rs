use super::*;
use crate::slack::client::RequestBody;
fn payload() -> Value {
    json!({"ok":true,"channel":{"id":"D1"},"app_home":{"app_id":"A1","app_installed_team_id":"T2","conversation_id":"D1","home_tab_enabled":true,"messages_tab_enabled":true,"home_view_id":"V1"},"home_view":{"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","hash":"first","unknown":{"kept":true},"blocks":[{"type":"actions","block_id":"b","elements":[{"type":"button","action_id":"go","text":{"type":"plain_text","text":"Review Équipe 東京"},"value":"","unknown":73}]}]}})
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn fields(r: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    let RequestBody::Form(f) = r.body else {
        panic!("form")
    };
    f.into_iter().collect()
}
#[test]
fn exact_home_identity_and_unknown_raw_are_preserved() {
    let p = payload();
    let (h, v) = Home::decode("D1", &p).unwrap();
    let v = v.unwrap();
    assert_eq!(h.installed_team, "T2");
    assert_eq!(v.raw(), &p["home_view"]);
    assert!(v.button("b", "go", "U1").is_ok());
    for key in ["app_id", "app_installed_team_id", "id", "type"] {
        let mut raw = v.raw().clone();
        raw[key] = json!("foreign");
        assert!(View::decode(&h, raw).is_err())
    }
}
#[test]
fn foreign_channel_null_flags_and_duplicate_controls_refuse() {
    for key in ["channel_id", "conversation_id"] {
        let mut p = payload();
        p["app_home"][key] = json!("D2");
        assert!(Home::decode("D1", &p).is_err())
    }
    let mut p = payload();
    p["app_home"]["home_tab_enabled"] = Value::Null;
    assert!(Home::decode("D1", &p).is_err());
    let mut p = payload();
    let e = p["home_view"]["blocks"][0]["elements"][0].clone();
    p["home_view"]["blocks"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(e);
    assert!(
        Home::decode("D1", &p)
            .unwrap()
            .1
            .unwrap()
            .button("b", "go", "U1")
            .is_err()
    )
}
#[test]
fn unresolved_home_and_unsupported_state_never_gain_button_authority() {
    let mut p = payload();
    p["app_home"]["home_view_id"] = Value::Null;
    p["home_view"] = Value::Null;
    assert!(Home::decode("D1", &p).unwrap().1.is_none());
    let mut p = payload();
    p["home_view"]["blocks"].as_array_mut().unwrap().push(json!({"type":"input","block_id":"draft","element":{"type":"plain_text_input","action_id":"value"}}));
    let v = Home::decode("D1", &p).unwrap().1.unwrap();
    assert!(v.has_controls());
    assert!(v.button("b", "go", "U1").is_err())
}
#[test]
fn calendar_oauth_user_visibility_and_confirmation_are_not_bypassed() {
    for update in [
        json!({"action_id":"calendar_hermes_connect"}),
        json!({"value":"ADZ494LHY_CREATE_EVENT"}),
        json!({"visible_to_user_ids":["U2"]}),
        json!({"confirm":null}),
        json!({"agent_prompt":"hello"}),
    ] {
        let mut p = payload();
        let b = &mut p["home_view"]["blocks"][0]["elements"][0];
        for (k, v) in update.as_object().unwrap() {
            b[k] = v.clone()
        }
        let action = b["action_id"].as_str().unwrap().to_owned();
        let v = Home::decode("D1", &p).unwrap().1.unwrap();
        assert!(v.button("b", &action, "U1").is_err())
    }
}
#[test]
fn actual_requests_use_exact_view_container_and_installed_team() {
    let (h, v) = Home::decode("D1", &payload()).unwrap();
    let v = v.unwrap();
    let b = v.button("b", "go", "U1").unwrap();
    let c = SlackClient::default();
    let w = workspace();
    let f = fields(b.prepare(&c, &w, &v, "web-1790000000000").unwrap());
    assert_eq!(f["service_team_id"], "T2");
    assert_eq!(f["app_id"], "A1");
    assert_eq!(f["service_id"], "B1");
    assert_eq!(
        serde_json::from_str::<Value>(&f["container"]).unwrap(),
        json!({"type":"view","view_id":"V1"})
    );
    assert_eq!(
        serde_json::from_str::<Value>(&f["actions"]).unwrap(),
        json!([{"type":"button","action_id":"go","block_id":"b","text":{"type":"plain_text","text":"Review Équipe 東京"},"value":""}])
    );
    assert!(!f.contains_key("state"));
    let open = fields(opened(&c, &w, &h).unwrap());
    assert_eq!(open["id"], "D1");
    assert_eq!(open["type"], "home");
    assert_eq!(open["service_team_id"], "T2");
    let read = fields(fetch(&c, &w, "D1").unwrap());
    assert_eq!(read["return_app_home"], "true")
}
#[test]
fn captured_button_refuses_any_raw_view_change() {
    let (h, v) = Home::decode("D1", &payload()).unwrap();
    let v = v.unwrap();
    let b = v.button("b", "go", "U1").unwrap();
    let mut raw = v.raw().clone();
    raw["unknown"]["kept"] = json!(false);
    let changed = View::decode(&h, raw).unwrap();
    assert!(!b.same_view(&changed));
    assert!(
        b.prepare(&SlackClient::default(), &workspace(), &changed, "token")
            .is_err()
    )
}
