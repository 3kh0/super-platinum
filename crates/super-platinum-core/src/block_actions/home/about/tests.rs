use super::*;
use crate::slack::client::RequestBody;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn fields(request: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    let RequestBody::Form(values) = request.body else {
        panic!("form")
    };
    values
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

#[test]
fn about_and_connection_reads_match_generated_requests_and_actual_form_serializer() {
    let client = SlackClient::default();
    let w = workspace();
    let request = Info::request(&client, &w, "A1").unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/apps.about.info"
    );
    assert_eq!(
        fields(request),
        std::collections::BTreeMap::from([
            ("token".into(), "synthetic".into()),
            ("app_id".into(), "A1".into()),
            ("_x_reason".into(), "fetch-unknown-apps-about-info".into()),
        ])
    );
    let request = Connection::request(&client, &w, "A1").unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/apps.user.connection.status"
    );
    assert_eq!(
        fields(request),
        std::collections::BTreeMap::from([
            ("token".into(), "synthetic".into()),
            ("app_ids".into(), "A1".into()),
            (
                "_x_reason".into(),
                "CFM fetch app connection status (app_about_tab)".into()
            ),
        ])
    );
}

#[test]
fn about_identity_and_typed_fields_are_checked_without_rewriting_unknown_data() {
    let raw = json!({"ok":true,"app_id":"A1","name":" Calendar ","long_description":"Many lines\n retained",
        "future":{"opaque":[null,{"value":42}]}});
    let info = Info::decode("A1", raw.clone()).unwrap();
    assert_eq!(info.raw(), &raw);
    assert_eq!(info.app(), "A1");
    assert_eq!(info.text("name"), Some("Calendar"));
    for (key, value) in [
        ("ok", json!(false)),
        ("app_id", json!("A2")),
        ("name", json!({})),
        ("is_ai_app", json!("true")),
    ] {
        let mut changed = raw.clone();
        changed[key] = value;
        assert!(Info::decode("A1", changed).is_err());
    }
}

#[test]
fn permissions_retain_source_order_and_required_optional_separation_with_raw_custody() {
    let raw = json!({"ok":true,"app_id":"A1","scope_info":{"content":{"heading":"Content","secondary_groups":{
        "user_content":{"heading":"People","scopes_info":[{"description":"Required"},{"description":"Optional","is_optional":true}]},
        "channel_content":{"heading":"Channels","scopes_info":[{"description":"Channels required","is_optional":false}]}
    }}}});
    let info = Info::decode("A1", raw.clone()).unwrap();
    let required = info.permissions("content", false).unwrap();
    assert_eq!(
        required
            .groups
            .iter()
            .map(|x| x.heading.as_str())
            .collect::<Vec<_>>(),
        vec!["Channels", "People"]
    );
    assert_eq!(
        info.permissions("content", true).unwrap().groups[0].descriptions,
        vec!["Optional"]
    );
    assert!(info.permissions("actions", false).is_none());
    assert_eq!(info.raw(), &raw);
}

#[test]
fn only_observed_loaded_connect_or_manage_status_can_prepare_the_same_connect_wire() {
    for (status, connect, manage) in [
        ("unknown", false, false),
        ("disconnected", true, false),
        ("connected", false, true),
        ("dwa_connected", false, false),
        ("ema_configured", false, false),
        ("future", false, false),
    ] {
        let raw = json!({"ok":true,"connections":{"A1":{"status":status,"approval":{"state":"pending"},"opaque":42}}});
        let connection = Connection::decode("A1", raw.clone()).unwrap();
        assert_eq!(connection.raw(), &raw);
        assert_eq!(connection.approval(), Some("pending"));
        assert_eq!(connection.can_connect(true), connect);
        assert_eq!(connection.can_manage(true), manage);
        assert!(!connection.can_connect(false));
        assert!(!connection.can_manage(false));
        assert!(
            connection
                .submit(&SlackClient::default(), &workspace(), "token", false)
                .is_err()
        );
        let request = connection.submit(&SlackClient::default(), &workspace(), "owned-token", true);
        assert_eq!(request.is_ok(), connect || manage);
        if let Ok(request) = request {
            assert_eq!(
                url::Url::parse(&request.url).unwrap().path(),
                "/api/apps.user.connection.submit"
            );
            assert_eq!(
                fields(request),
                std::collections::BTreeMap::from([
                    ("token".into(), "synthetic".into()),
                    ("app_id".into(), "A1".into()),
                    ("service_id".into(), "0".into()),
                    ("type".into(), "connect".into()),
                    ("client_token".into(), "owned-token".into()),
                    (
                        "_x_reason".into(),
                        "submit-user-connection-event-to-app".into()
                    ),
                ])
            );
        }
    }
    let missing = Connection::decode("A1", json!({"ok":true,"connections":{}})).unwrap();
    assert_eq!(missing.status(), "unknown");
    assert!(!missing.can_connect(true));
    assert!(!missing.can_manage(true));
}

#[test]
fn actual_experiment_completion_distinguishes_off_from_unknown() {
    assert_eq!(
        Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap(),
        Flags::default()
    );
    assert!(Flags::decode(&json!({"ok":true})).is_err());
    assert!(
        Flags::decode(&json!({"ok":true,"assignments":{"apps_connection_status":{"group":true}}}))
            .is_err()
    );
    let f=Flags::decode(&json!({"ok":true,"assignments":{"apps_connection_status":{"group":"on"},"agent_about_page":{"group":"on"},"flexpane_content_parity":{"group":"off"}}})).unwrap();
    assert!(f.connection && f.about && !f.parity);
}
#[test]
fn permission_visibility_uses_profile_identity_and_both_captured_experiment_branches() {
    let raw = json!({"ok":true,"app_id":"A1","scope_info":{"content":{"heading":"Content","secondary_groups":{"channel_content":{"heading":"Channels","scopes_info":[{"description":"Read"}]}}}}});
    let mut info = Info::decode("A1", raw).unwrap();
    let profile = Profile::decode(
        "A1",
        json!({"ok":true,"app_profile":{"id":"A1","team_id":"T1","is_distributed":false}}),
    )
    .unwrap();
    let mut flags = Flags {
        connection: false,
        about: true,
        parity: false,
    };
    assert!(!profile.show_permissions(&info, flags, "T1"));
    flags.parity = true;
    assert!(profile.show_permissions(&info, flags, "T1"));
    info.raw["agentforce"] = json!({"is_available":true});
    assert!(!profile.show_permissions(&info, flags, "T1"));
    info.raw.as_object_mut().unwrap().remove("agentforce");
    flags.about = false;
    assert!(!profile.show_permissions(&info, flags, "T1"));
    assert!(Profile::decode("A1", json!({"ok":true,"app_profile":{"id":"A2"}})).is_err());
    assert!(
        Profile::decode(
            "A1",
            json!({"ok":true,"app_profile":{"id":"A1","is_distributed":"true"}})
        )
        .is_err()
    );
}
#[test]
fn assistant_description_requires_actual_ai_profile_or_about_flags() {
    let info = Info::decode(
        "A1",
        json!({"ok":true,"app_id":"A1","assistant_description":"Not automatic permission"}),
    )
    .unwrap();
    let profile = |extra: Value| {
        Profile::decode(
            "A1",
            json!({"ok":true,"app_profile":{"id":"A1","agent_capabilities":extra}}),
        )
        .unwrap()
    };
    assert!(!profile(json!({})).is_ai(&info));
    assert!(profile(json!({"can_respond_in_mpdm":true})).is_ai(&info));
}
