use super::*;
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        url: "https://fixture.slack.com".into(),
        name: "Fixture".into(),
        token: "synthetic-feature-credential".into(),
    }
}
fn channel() -> Value {
    json!({"id":"C00000001","is_channel":true,"is_im":false})
}
fn discovery(s: &WorkspaceSession) -> Value {
    json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",s.team_id),"ttl_seconds":60})
}
#[test]
fn ordinary_and_enterprise_boots_use_exact_permission_wire() {
    for mode in 0..3 {
        let mut s = session();
        if mode == 1 {
            s.enterprise_id = Some("E00000001".into());
        }
        if mode == 2 {
            s.team_id = "E00000001".into();
        }
        let read = Read::capture(&channel(), &s).unwrap();
        let client = SlackClient::default();
        let d = read.discovery(&client, &s).unwrap();
        assert_eq!(d.method, "POST");
        assert_eq!(
            url::Url::parse(&d.url).unwrap().path(),
            "/api/api.getFlannelHttpUrl"
        );
        let RequestBody::Form(form) = d.body else {
            panic!("discovery form")
        };
        assert_eq!(
            form,
            vec![
                ("token".into(), s.token.clone()),
                ("include_external_workspaces".into(), "true".into()),
                ("_x_reason".into(), "client-needs-valid-flannel-url".into())
            ]
        );
        let request = read.request(&client, &s, &discovery(&s)).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            format!("/cache/{}/permissions/info", s.team_id)
        );
        let RequestBody::Json(body) = request.body else {
            panic!("permission JSON")
        };
        let mut expected = json!({"token":s.token,"user_id":"U00000001","channel_id":"C00000001","permissions":["MANAGE_CHANNEL_WORKFLOWS"]});
        if mode == 2 {
            expected["enterprise_token"] = s.token.clone().into();
        }
        assert_eq!(body, expected);
        for excluded in ["_x_reason", "as_admin", "team_ids", "enterprise_id"] {
            assert!(body.get(excluded).is_none());
        }
    }
}
#[test]
fn same_enterprise_discovery_is_accepted_without_changing_boot_credentials() {
    let mut s = session();
    s.enterprise_id = Some("E00000001".into());
    let read = Read::capture(&channel(), &s).unwrap();
    let request = read
        .request(
            &SlackClient::default(),
            &s,
            &json!({"ok":true,"url":"https://edgeapi.slack.com/cache/E00000001","ttl_seconds":60}),
        )
        .unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/cache/E00000001/permissions/info"
    );
    let RequestBody::Json(body) = request.body else {
        panic!()
    };
    assert!(body.get("enterprise_token").is_none());
}
#[test]
fn exact_channel_allowed_is_the_only_positive_source() {
    let read = Read::capture(&channel(), &session()).unwrap();
    for raw in [
        json!({"ok":true,"allowed":["MANAGE_CHANNEL_WORKFLOWS"]}),
        json!({"ok":true,"allowed":["MANAGE_CHANNEL_WORKFLOWS","MANAGE_CHANNEL_WORKFLOWS"],"allowed_per_team":{}}),
    ] {
        assert!(matches!(read.permission(&raw), Ok(Permission::Allowed)));
    }
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"allowed":null}),
        json!({"ok":true,"allowed":[]}),
        json!({"ok":true,"allowed":["MANAGE_CHANNEL_TABS","106"]}),
        json!({"ok":true,"allowed_per_team":{"T00000001":["MANAGE_CHANNEL_WORKFLOWS"]}}),
        json!({"ok":true,"allowed":[],"allowed_per_team":{"T00000001":["MANAGE_CHANNEL_WORKFLOWS"]}}),
    ] {
        assert!(matches!(read.permission(&raw), Ok(Permission::Denied)));
    }
}
#[test]
fn malformed_and_error_envelopes_never_grant_or_become_confirmed_denials() {
    let read = Read::capture(&channel(), &session()).unwrap();
    for raw in [
        json!({}),
        json!({"ok":1,"allowed":["MANAGE_CHANNEL_WORKFLOWS"]}),
        json!({"ok":false}),
        json!({"ok":true,"error":"permission_denied","allowed":["MANAGE_CHANNEL_WORKFLOWS"]}),
        json!({"ok":true,"allowed":"MANAGE_CHANNEL_WORKFLOWS"}),
        json!({"ok":true,"allowed":{}}),
        json!({"ok":true,"allowed":[106]}),
        json!({"ok":true,"allowed":["MANAGE_CHANNEL_WORKFLOWS",false]}),
        json!({"ok":true,"allowed":[""]}),
    ] {
        assert!(read.permission(&raw).is_err());
    }
}
#[test]
fn credential_and_context_replacement_refuses_both_requests() {
    let original = session();
    let read = Read::capture(&channel(), &original).unwrap();
    for change in 0..5 {
        let mut changed = original.clone();
        match change {
            0 => changed.team_id = "T00000002".into(),
            1 => changed.user_id = "U00000002".into(),
            2 => changed.enterprise_id = Some("E00000001".into()),
            3 => changed.token = "replacement-synthetic".into(),
            _ => changed.url = "https://other.slack.com".into(),
        };
        assert!(read.discovery(&SlackClient::default(), &changed).is_err());
        assert!(
            read.request(&SlackClient::default(), &changed, &discovery(&original))
                .is_err()
        );
    }
    assert!(!format!("{read:?}").contains(&original.token));
    assert!(!format!("{read:?}").contains(&original.url));
}
#[test]
fn untrusted_or_decorated_discovery_urls_are_refused() {
    let s = session();
    let read = Read::capture(&channel(), &s).unwrap();
    for endpoint in [
        "https://example.test/cache/T00000001",
        "https://edgeapi.slack.com.evil.test/cache/T00000001",
        "http://edgeapi.slack.com/cache/T00000001",
        "https://edgeapi.slack.com/cache/T00000002",
        "https://edgeapi.slack.com/cache/E00000001",
        "https://edgeapi.slack.com/cache/T00000001/",
        "https://edgeapi.slack.com/cache/T00000001?route=other",
        "https://edgeapi.slack.com/cache/T00000001#fragment",
        "https://user@edgeapi.slack.com/cache/T00000001",
        "https://edgeapi.slack.com:443/cache/T00000001",
    ] {
        assert!(
            read.request(
                &SlackClient::default(),
                &s,
                &json!({"ok":true,"url":endpoint,"ttl_seconds":60})
            )
            .is_err()
        );
    }
    for raw in [
        json!({}),
        json!({"ok":false,"url":discovery(&s)["url"],"ttl_seconds":60}),
        json!({"ok":true,"error":"bad","url":discovery(&s)["url"],"ttl_seconds":60}),
        json!({"ok":true,"url":discovery(&s)["url"],"ttl_seconds":0}),
        json!({"ok":true,"url":discovery(&s)["url"],"ttl_seconds":"60"}),
    ] {
        assert!(read.request(&SlackClient::default(), &s, &raw).is_err());
    }
}
#[test]
fn invalid_or_im_channel_and_inconsistent_enterprise_are_refused() {
    for raw in [
        json!({}),
        json!({"id":"C2"}),
        json!({"id":"C00000001","is_im":true}),
        json!({"id":"C00000001","is_im":"false"}),
        json!({"id":"C00000001","isUnknown":true}),
        json!({"id":"C00000001","isNonExistent":true}),
    ] {
        assert!(Read::capture(&raw, &session()).is_err());
    }
    let mut s = session();
    s.team_id = "E00000001".into();
    s.enterprise_id = Some("E00000002".into());
    assert!(Read::capture(&channel(), &s).is_err());
    s.enterprise_id = Some("E00000001".into());
    assert!(Read::capture(&channel(), &s).is_ok());
}
