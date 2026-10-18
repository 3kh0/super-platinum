use super::*;
use serde_json::json;

fn home() -> Value {
    json!({"ok":true,"channel":{"id":"D1","is_im":true,"user":"UBOT"},"app_home":{"app_id":"A1","conversation_id":"D1","messages_tab_enabled":true}})
}
fn profile(value: Option<bool>) -> AppProfile {
    let mut raw = json!({"ok":true,"app_profile":{"id":"A1"}});
    if let Some(v) = value {
        raw["app_profile"]["is_interactable"] = json!(v);
    }
    AppProfile::decode("A1", &raw).unwrap()
}
#[test]
fn app_dm_policy_matches_captured_enabled_readonly_and_conditional_acl() {
    for messages in [false, true] {
        for read_only in [false, true] {
            for acl in [false, true] {
                for interactable in [None, Some(false), Some(true)] {
                    let mut raw = home();
                    raw["app_home"]["messages_tab_enabled"] = json!(messages);
                    raw["app_home"]["messages_tab_read_only_enabled"] = json!(read_only);
                    let h = AppHomePosting::decode("D1", "A1", &raw).unwrap();
                    assert_eq!(
                        h.allows("D1", "A1", acl, Some(&profile(interactable))),
                        messages && !read_only && (!acl || interactable != Some(false))
                    );
                }
            }
        }
    }
    let mut raw = home();
    raw["app_home"]
        .as_object_mut()
        .unwrap()
        .remove("messages_tab_enabled");
    let h = AppHomePosting::decode("D1", "A1", &raw).unwrap();
    assert!(h.allows("D1", "A1", false, None));
    assert!(!h.allows("D1", "A1", true, None));
    assert!(!h.allows("D2", "A1", false, None));
    assert!(!h.allows("D1", "A2", false, None));
}
#[test]
fn completed_metadata_requires_exact_identity_and_typed_flags() {
    for (pointer, value) in [
        ("/ok", json!(false)),
        ("/channel/id", json!("D_OTHER")),
        ("/app_home/app_id", json!("A_OTHER")),
        ("/app_home/conversation_id", json!("D_OTHER")),
        ("/app_home/messages_tab_enabled", json!("true")),
        ("/app_home", Value::Null),
    ] {
        let mut raw = home();
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(AppHomePosting::decode("D1", "A1", &raw).is_err());
    }
    for value in [Value::Null, json!(0), json!("false")] {
        let mut raw = home();
        raw["app_home"]["messages_tab_read_only_enabled"] = value.clone();
        assert!(AppHomePosting::decode("D1", "A1", &raw).is_err());
        assert!(
            AppProfile::decode(
                "A1",
                &json!({"ok":true,"app_profile":{"id":"A1","is_interactable":value}})
            )
            .is_err()
        );
    }
    assert!(AppProfile::decode("A1", &json!({"ok":true,"app_profile":{"id":"A2"}})).is_err());
}
#[test]
fn experiment_assignment_and_member_app_mapping_do_not_infer_permission() {
    assert!(!acl_assignment(&json!({"ok":true,"assignments":{}})).unwrap());
    for group in ["on", "off", "ON"] {
        assert_eq!(
            acl_assignment(
                &json!({"ok":true,"assignments":{"app_acls_for_all_apps":{"group":group}}})
            )
            .unwrap(),
            group == "on"
        );
    }
    assert!(acl_assignment(&json!({"ok":true})).is_err());
    assert!(
        acl_assignment(&json!({"ok":true,"assignments":{"app_acls_for_all_apps":null}})).is_err()
    );
    let mut user: User = serde_json::from_value(
        json!({"id":"UBOT","is_bot":true,"name":"A_FAKE","profile":{"api_app_id":"A1"}}),
    )
    .unwrap();
    assert_eq!(app_id(&user), Some("A1"));
    user.profile.as_mut().unwrap().extra.remove("api_app_id");
    assert_eq!(app_id(&user), None);
}
#[test]
fn profile_request_has_exact_captured_app_and_wire_reason() {
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let request = profile_request(&SlackClient::default(), &w, "A1").unwrap();
    assert!(request.url.contains("/api/apps.profile.get"));
    let crate::slack::client::RequestBody::Form(form) = request.body else {
        panic!("form")
    };
    assert_eq!(
        form,
        vec![
            ("token".into(), "synthetic".into()),
            ("app".into(), "A1".into()),
            ("_x_reason".into(), "fetch-all-apps-by-id".into())
        ]
    );
    assert!(profile_request(&SlackClient::default(), &w, "../bad").is_err());
}

#[test]
fn actual_app_dm_discriminator_is_distinct_from_app_id_association() {
    for (raw, dm, app) in [
        (
            json!({"id":"U1","is_bot":true,"profile":{"api_app_id":"A1"}}),
            Some(true),
            Some("A1"),
        ),
        (
            json!({"id":"U1","is_app_user":true,"profile":{"api_app_id":"A1"}}),
            Some(false),
            Some("A1"),
        ),
        (
            json!({"id":"U1","is_slackbot":true,"profile":{"api_app_id":"A1"}}),
            Some(true),
            None,
        ),
        (
            json!({"id":"U1","is_slackbot":true,"is_app_user":true,"profile":{"api_app_id":"A1"}}),
            Some(true),
            Some("A1"),
        ),
        (json!({"id":"USLACKBOT","is_bot":true}), Some(false), None),
        (json!({"id":"USLACK"}), None, None),
        (json!({"id":"U1","is_slackbot":"true"}), None, None),
    ] {
        let user: User = serde_json::from_value(raw).unwrap();
        assert_eq!(app_dm(&user).ok(), dm);
        assert_eq!(app_id(&user), app);
    }
}

#[test]
fn app_metadata_identity_rejects_path_query_and_non_ascii_syntax_consistently() {
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let client = SlackClient::default();
    for app in ["A1", "A_HOME", &"A".repeat(256)] {
        assert!(profile_request(&client, &w, app).is_ok());
        assert!(AppProfile::decode(app, &json!({"ok":true,"app_profile":{"id":app}})).is_ok());
    }
    for app in [
        "",
        "../bad",
        "A/B",
        "A?token=x",
        "A#other",
        "A%2FB",
        "A bad",
        "A\n",
        "Aé",
        &"A".repeat(257),
    ] {
        assert!(profile_request(&client, &w, app).is_err(), "{app:?}");
        assert!(AppProfile::decode(app, &json!({"ok":true,"app_profile":{"id":app}})).is_err());
        let user: User =
            serde_json::from_value(json!({"id":"UBOT","is_bot":true,"profile":{"api_app_id":app}}))
                .unwrap();
        assert_eq!(app_id(&user), None);
        let mut raw = home();
        raw["app_home"]["app_id"] = json!(app);
        assert!(AppHomePosting::decode("D1", app, &raw).is_err());
        let mut raw = home();
        raw["channel"]["id"] = json!(app);
        raw["app_home"]["conversation_id"] = json!(app);
        assert!(AppHomePosting::decode(app, "A1", &raw).is_err());
    }
}
