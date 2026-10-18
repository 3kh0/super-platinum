use super::*;
fn inputs() -> (Value, User, PostingFacts, WorkspaceSession) {
    let raw = json!({"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"properties":{"tabz":[]}});
    let member: User = serde_json::from_value(json!({"id":"U00000001"})).unwrap();
    let mut facts = PostingFacts::new("C00000001".into());
    facts.observe(&raw, Some(&member));
    let session = WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: Some("E00000001".into()),
        user_id: member.id.clone(),
        url: "https://fixture.slack.com".into(),
        name: "Fixture".into(),
        token: "synthetic".into(),
    };
    (raw, member, facts, session)
}
fn endpoint(session: &WorkspaceSession) -> Value {
    json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",session.team_id),"ttl_seconds":60})
}
#[test]
fn discovered_endpoint_and_boot_context_determine_wire_without_admin_aggregation() {
    for unified in [false, true] {
        let (raw, user, facts, mut session) = inputs();
        if unified {
            session.team_id = "E00000001".into();
            session.enterprise_id = None;
        }
        let read = ChannelPermissionRead::capture(&raw, &user, &facts, &session).unwrap();
        let discovery = read.discovery(&SlackClient::default(), &session).unwrap();
        assert_eq!(
            url::Url::parse(&discovery.url).unwrap().path(),
            "/api/api.getFlannelHttpUrl"
        );
        let crate::slack::client::RequestBody::Form(form) = discovery.body else {
            panic!("discovery form")
        };
        assert_eq!(
            form,
            vec![
                ("token".into(), "synthetic".into()),
                ("include_external_workspaces".into(), "true".into())
            ]
        );
        let request = read
            .request(&SlackClient::default(), &session, &endpoint(&session))
            .unwrap();
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            format!("/cache/{}/permissions/info", session.team_id)
        );
        let crate::slack::client::RequestBody::Json(body) = request.body else {
            panic!("edge JSON")
        };
        let mut expected = json!({"user_id":"U00000001","channel_id":"C00000001","permissions":["MANAGE_CHANNEL_TABS"],"token":"synthetic"});
        if unified {
            expected["enterprise_token"] = "synthetic".into();
        }
        assert_eq!(body, expected);
    }
}
#[test]
fn discovery_is_required_and_cannot_send_credentials_to_other_or_decorated_urls() {
    let (raw, user, facts, session) = inputs();
    let read = ChannelPermissionRead::capture(&raw, &user, &facts, &session).unwrap();
    for url in [
        "https://example.com/cache/T00000001",
        "https://edgeapi.slack.com.evil.invalid/cache/T00000001",
        "http://edgeapi.slack.com/cache/T00000001",
        "https://edgeapi.slack.com/cache/T00000002",
        "https://edgeapi.slack.com/cache/E00000002",
        "https://edgeapi.slack.com/cache/T00000001/",
        "https://edgeapi.slack.com/cache/T00000001?route=other",
        "https://edgeapi.slack.com/cache/T00000001#fragment",
        "https://user@edgeapi.slack.com/cache/T00000001",
        "https://edgeapi.slack.com:443/cache/T00000001",
    ] {
        assert!(
            read.request(
                &SlackClient::default(),
                &session,
                &json!({"ok":true,"url":url,"ttl_seconds":60})
            )
            .is_err(),
            "{url}"
        );
    }
    for invalid in [
        json!({}),
        json!({"ok":false,"url":endpoint(&session)["url"],"ttl_seconds":60}),
        json!({"ok":true,"error":"denied","url":endpoint(&session)["url"],"ttl_seconds":60}),
        json!({"ok":true,"url":endpoint(&session)["url"],"ttl_seconds":0}),
    ] {
        assert!(
            read.request(&SlackClient::default(), &session, &invalid)
                .is_err()
        );
    }
    // A server-selected URL for the same captured Enterprise is not fabricated
    // from membership; it is admitted only through this positive discovery.
    let discovered =
        json!({"ok":true,"url":"https://edgeapi.slack.com/cache/E00000001","ttl_seconds":60});
    let request = read
        .request(&SlackClient::default(), &session, &discovered)
        .unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/cache/E00000001/permissions/info"
    );
    let crate::slack::client::RequestBody::Json(body) = request.body else {
        panic!()
    };
    assert!(body.get("enterprise_token").is_none());
}
#[test]
fn exact_string_grant_and_explicit_absence_are_distinct_from_unknown_responses() {
    let (raw, user, facts, session) = inputs();
    let read = ChannelPermissionRead::capture(&raw, &user, &facts, &session).unwrap();
    assert_eq!(
        read.permission(&json!({"ok":true,"allowed":["MANAGE_CHANNEL_TABS"]})),
        Ok(Permission::Allowed)
    );
    assert_eq!(
        read.permission(&json!({"ok":true,"allowed":[]})),
        Ok(Permission::Denied)
    );
    assert_eq!(
        read.permission(&json!({"ok":true,"allowed":["ASSIGN_CHANNEL_MANAGER"]})),
        Ok(Permission::Denied)
    );
    for raw in [
        json!({"ok":true,"allowed":[119]}),
        json!({"ok":true,"allowed_per_team":{"T00000001":["MANAGE_CHANNEL_TABS"]}}),
        json!({"ok":true,"error":"not_authed","allowed":["MANAGE_CHANNEL_TABS"]}),
        json!({"ok":false,"allowed":["MANAGE_CHANNEL_TABS"]}),
    ] {
        assert!(read.permission(&raw).is_err());
    }
}
#[test]
fn request_cannot_change_viewer_team_or_enterprise_after_capture() {
    for change in 0..3 {
        let (raw, user, facts, mut session) = inputs();
        let read = ChannelPermissionRead::capture(&raw, &user, &facts, &session).unwrap();
        match change {
            0 => session.user_id = "U00000002".into(),
            1 => session.team_id = "T00000002".into(),
            _ => session.enterprise_id = Some("E00000002".into()),
        };
        assert!(matches!(
            read.request(&SlackClient::default(), &session, &endpoint(&session)),
            Err(Fault::Changed)
        ));
    }
}
#[test]
fn scope_does_not_override_explicit_policy_or_membership_and_posting_refusals() {
    for change in 0..6 {
        let (mut raw, user, _, mut session) = inputs();
        match change {
            0 => raw["properties"]["who_can_manage_tabs"] = "managers".into(),
            1 => raw["properties"]["who_can_manage_tabs"] = "everyone".into(),
            2 => raw["is_member"] = false.into(),
            3 => raw["is_read_only"] = true.into(),
            4 => session.enterprise_id = None,
            _ => raw["properties"]["who_can_manage_tabs"] = false.into(),
        };
        let mut facts = PostingFacts::new("C00000001".into());
        facts.observe(&raw, Some(&user));
        assert!(
            ChannelPermissionRead::capture(&raw, &user, &facts, &session).is_err(),
            "case {change}"
        );
    }
    let (mut raw, user, facts, session) = inputs();
    raw["properties"]["who_can_manage_tabs"] = Value::Null;
    assert!(ChannelPermissionRead::capture(&raw, &user, &facts, &session).is_ok());
}

#[test]
fn unified_context_does_not_require_redundant_enterprise_metadata_but_detects_its_change() {
    let (raw, user, facts, mut session) = inputs();
    session.team_id = "E00000001".into();
    session.enterprise_id = None;
    let read = ChannelPermissionRead::capture(&raw, &user, &facts, &session).unwrap();
    session.enterprise_id = Some("E00000001".into());
    assert!(matches!(
        read.discovery(&SlackClient::default(), &session),
        Err(Fault::Changed)
    ));
    session.enterprise_id = Some("E00000002".into());
    assert!(ChannelPermissionRead::capture(&raw, &user, &facts, &session).is_err());
}
