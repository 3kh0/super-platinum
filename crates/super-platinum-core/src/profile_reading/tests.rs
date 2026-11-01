use super::*;
fn member() -> Value {
    json!({"id":"U1","name":"maya","team_id":"T1","enterprise_user":null,"is_bot":false,"is_app_user":false,"deleted":false,"can_interact":true,"profile":{"display_name":"Maya","email":"maya@fixture.invalid"}})
}
#[test]
fn profile_reading_projects_only_explicit_same_team_human_and_boolean_interaction_branch() {
    let raw = member();
    let p = Projection::from_raw(&raw, "U1", "T1").unwrap();
    assert_eq!(
        p.user().profile.as_ref().unwrap().display_name.as_deref(),
        Some("Maya")
    );
    assert!(!p.user().is_stranger);
    let mut raw = raw;
    raw["can_interact"] = json!(false);
    assert!(
        Projection::from_raw(&raw, "U1", "T1")
            .unwrap()
            .user()
            .is_stranger
    );
}
#[test]
fn profile_reading_omitted_conflicting_sentinel_and_foreign_fields_refuse_without_default_grants() {
    for key in [
        "id",
        "name",
        "team_id",
        "enterprise_user",
        "is_bot",
        "is_app_user",
        "deleted",
        "can_interact",
    ] {
        let mut raw = member();
        raw.as_object_mut().unwrap().remove(key);
        assert!(Projection::from_raw(&raw, "U1", "T1").is_err(), "{key}");
    }
    for (key, value) in [
        ("id", json!("U2")),
        ("name", json!("U1")),
        ("name", json!("")),
        ("team_id", json!("OTHER")),
        ("enterprise_user", json!({"id":"W1"})),
        ("is_bot", json!(true)),
        ("is_app_user", json!(true)),
        ("deleted", json!(true)),
        ("can_interact", json!("true")),
        ("isUnknown", json!(true)),
        ("isNonExistent", json!(true)),
    ] {
        let mut raw = member();
        raw[key] = value;
        assert!(Projection::from_raw(&raw, "U1", "T1").is_err(), "{key}");
    }
}
#[test]
fn profile_reading_reply_handles_source_arrays_and_objects_without_seeding_missing_or_failed_ids() {
    let ids = vec!["U1".into(), "U2".into()];
    assert_eq!(
        from_reply(&json!({"results":[member()]}), &ids, "T1")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        from_reply(&json!({"results":{"one":[member()]}}), &ids, "T1")
            .unwrap()
            .len(),
        1
    );
    assert!(
        from_reply(&json!({"results":[]}), &ids, "T1")
            .unwrap()
            .is_empty()
    );
    assert!(
        from_reply(
            &json!({"results":[member()],"failed_ids":["U1"]}),
            &ids,
            "T1"
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn profile_reading_ambiguous_unrequested_pending_and_error_envelopes_refuse() {
    let ids = vec!["U1".into()];
    for raw in [
        json!({"results":[member(),member()]}),
        json!({"results":[{"id":"U2"}]}),
        json!({"results":[member()],"pending":["U1"]}),
        json!({"results":[member()],"next_marker":"more"}),
        json!({"results":[member()],"ok":false}),
        json!({"results":[member()],"error":"denied"}),
    ] {
        assert!(from_reply(&raw, &ids, "T1").is_err());
    }
    assert!(
        from_reply(
            &json!({"results":[member()]}),
            &["U1".into(), "U1".into()],
            "T1"
        )
        .is_err()
    );
}
#[test]
fn profile_reading_request_uses_original_client_flannel_zero_interaction_branch_and_refuses_other_namespace()
 {
    let mut session = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U0".into(),
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    let client = crate::slack::SlackClient::default();
    let request =
        crate::slack::edge::profile_users_info(&client, &session, &["U1".into()]).unwrap();
    assert!(
        request
            .url
            .starts_with("https://edgeapi.slack.com/cache/T1/users/info?")
    );
    let crate::slack::client::RequestBody::Json(body) = request.body else {
        panic!()
    };
    assert_eq!(body["updated_ids"], json!({"U1":0}));
    assert_eq!(body["check_interaction"], json!(true));
    assert_eq!(body["include_profile_only_users"], json!(true));
    session.enterprise_id = Some("E1".into());
    assert!(crate::slack::edge::profile_users_info(&client, &session, &["U1".into()]).is_err());
    session.enterprise_id = None;
    assert!(crate::slack::edge::profile_users_info(&client, &session, &["bad/id".into()]).is_err());
}
