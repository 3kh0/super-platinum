use super::*;
use crate::slack::client::RequestBody;

fn workspace(id: &str) -> WorkspaceSession {
    WorkspaceSession {
        team_id: id.into(),
        enterprise_id: id.starts_with('E').then(|| id.into()),
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn boot(id: &str) -> Value {
    json!({"ok":true,"self":{"id":"U1"},"team":{"id":id,"prefs":{
        "auth_mode":"normal","sso_choose_username":false,"display_pronouns":true}},"workspaces":[]})
}
fn profile() -> Value {
    json!({"ok":true,"profile":{"real_name":"Fixture Person","display_name":"fixture","pronouns":"they/them", "unknown":{"preserve":true},"status_text":"keep"}})
}
fn decode(
    w: &WorkspaceSession,
    b: &Value,
    p: &Value,
    e: Option<&Value>,
) -> FreshOwnProfileCapabilities {
    FreshOwnProfileCapabilities::from_responses(w, b, p, &profile(), &json!({"ok":true,"team_prefs":{"toplevel_prefs":b.pointer("/team/prefs").cloned().unwrap_or(Value::Null)}}), e).unwrap()
}
#[test]
fn standalone_permissions_are_explicit_and_unknown_is_not_denied_or_allowed() {
    let w = workspace("T1");
    let b = boot("T1");
    let known = decode(
        &w,
        &b,
        &json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]}),
        None,
    );
    for field in [Field::FullName, Field::DisplayName, Field::Pronouns] {
        assert!(known.capability(field).allowed());
    }
    assert_eq!(known.team_id(), "T1");
    assert_eq!(known.user_id(), "U1");
    assert_eq!(known.profile_raw()["unknown"], json!({"preserve":true}));
    assert_eq!(known.profile().status_text.as_deref(), Some("keep"));
    let denied = decode(&w, &b, &json!({"ok":true,"allowed":[]}), None);
    assert!(matches!(
        denied.capability(Field::FullName),
        ProfileEditCapability::Denied(_)
    ));
    for response in [
        json!({"ok":true}),
        json!({"ok":true,"allowed":null}),
        json!({"ok":true,"allowed":[69]}),
    ] {
        let unknown = decode(&w, &b, &response, None);
        assert!(matches!(
            unknown.capability(Field::FullName),
            ProfileEditCapability::Unknown(_)
        ));
        assert!(!unknown.capability(Field::DisplayName).allowed());
        assert!(unknown.capability(Field::FullName).reason().is_some());
    }
}
#[test]
fn messaging_sso_uses_team_policy_and_never_admin_enterprise_auth_policy() {
    let w = workspace("E1");
    let mut b = boot("E1");
    let allowed = json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]});
    let org = json!({"ok":true,"locked_prefs":{"auth_mode":"saml","display_pronouns":true}});
    assert!(
        decode(&w, &b, &allowed, Some(&org))
            .capability(Field::DisplayName)
            .allowed()
    );
    b["team"]["prefs"]["auth_mode"] = json!("saml");
    let locked = decode(&w, &b, &allowed, Some(&org));
    assert!(matches!(
        locked.capability(Field::DisplayName),
        ProfileEditCapability::Denied(_)
    ));
    assert!(locked.capability(Field::FullName).allowed());
    b["team"]["prefs"]["sso_choose_username"] = json!(true);
    assert!(
        decode(&w, &b, &allowed, Some(&org))
            .capability(Field::DisplayName)
            .allowed()
    );
    b["team"].as_object_mut().unwrap().remove("prefs");
    assert!(matches!(
        decode(&w, &b, &allowed, Some(&org)).capability(Field::DisplayName),
        ProfileEditCapability::Unknown(_)
    ));
}
#[test]
fn grid_aggregates_only_internal_workspace_permissions_and_uses_locked_pronouns() {
    let w = workspace("E1");
    let mut b = boot("E1");
    b["workspaces"] = json!([{"id":"T1"},{"id":"T2","allow_external_users":false},{"id":"T3","allow_external_users":true}]);
    let p = json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"],"allowed_per_team":{"T1":["UPDATE_NAME"],"T2":["UPDATE_DISPLAY_NAME"],"T3":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]}});
    let org = json!({"ok":true,"locked_prefs":{"display_pronouns":false},"org_prefs":{"display_pronouns":true},"unlocked_prefs":{"display_pronouns":true}});
    let cap = decode(&w, &b, &p, Some(&org));
    assert!(cap.capability(Field::FullName).allowed());
    assert!(cap.capability(Field::DisplayName).allowed());
    assert!(matches!(
        cap.capability(Field::Pronouns),
        ProfileEditCapability::Denied(_)
    ));
    let external_only = decode(
        &w,
        &b,
        &json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"],"allowed_per_team":{"T3":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]}}),
        Some(&org),
    );
    assert!(!external_only.capability(Field::FullName).allowed());
    let incomplete = decode(
        &w,
        &b,
        &json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]}),
        None,
    );
    assert!(matches!(
        incomplete.capability(Field::FullName),
        ProfileEditCapability::Unknown(_)
    ));
    assert!(matches!(
        incomplete.capability(Field::Pronouns),
        ProfileEditCapability::Unknown(_)
    ));
    let org = json!({"ok":true,"locked_prefs":{"display_pronouns":true}});
    assert!(
        decode(&w, &b, &p, Some(&org))
            .capability(Field::Pronouns)
            .allowed()
    );
}
#[test]
fn request_shapes_match_standalone_grid_and_empty_internal_inventory() {
    let c = SlackClient::default();
    let w = workspace("T1");
    let boot_req = own_profile_boot_request(&c, &w);
    assert!(boot_req.url.contains("/api/client.init?"));
    let RequestBody::Form(fields) = boot_req.body else {
        panic!("form")
    };
    assert!(
        fields
            .iter()
            .any(|(k, v)| k == "include_relevant_onboarding" && v == "false")
    );
    let request = own_profile_permissions_request(&c, &w, &boot("T1")).unwrap();
    assert!(request.url.contains("/cache/T1/permissions/info?"));
    let RequestBody::Json(body) = request.body else {
        panic!("json")
    };
    assert_eq!(
        body["permissions"],
        json!(["UPDATE_NAME", "UPDATE_DISPLAY_NAME"])
    );
    assert_eq!(body["user_id"], "U1");
    assert_eq!(body["as_admin"], true);
    assert!(body.get("team_ids").is_none());
    let w = workspace("E1");
    let mut b = boot("E1");
    b["workspaces"] = json!([{"id":"T1"},{"id":"T2","allow_external_users":true}]);
    let request = own_profile_permissions_request(&c, &w, &b).unwrap();
    let RequestBody::Json(body) = request.body else {
        panic!("json")
    };
    assert_eq!(body["team_ids"], json!(["T1"]));
    assert!(body.get("as_admin").is_none());
    b["workspaces"] = json!([]);
    let request = own_profile_permissions_request(&c, &w, &b).unwrap();
    let RequestBody::Json(body) = request.body else {
        panic!("json")
    };
    assert_eq!(body["as_admin"], true);
    assert!(body.get("team_ids").is_none());
}
#[test]
fn malformed_or_wrong_owner_boot_and_profile_never_create_a_grant() {
    let w = workspace("E1");
    let allowed = json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]});
    for b in [
        json!({"ok":true}),
        boot("E2"),
        {
            let mut b = boot("E1");
            b["self"]["id"] = json!("U2");
            b
        },
        {
            let mut b = boot("E1");
            b.as_object_mut().unwrap().remove("workspaces");
            b
        },
        {
            let mut b = boot("E1");
            b["workspaces"] = json!([{"id":"T1"},{"id":"T1"}]);
            b
        },
    ] {
        assert!(own_profile_permissions_request(&SlackClient::default(), &w, &b).is_err());
    }
    for p in [
        json!({"ok":true}),
        json!({"ok":false,"error":"not_authed"}),
        json!({"ok":true,"user_id":"U2","profile":{}}),
    ] {
        assert!(
            FreshOwnProfileCapabilities::from_responses(
                &w,
                &boot("E1"),
                &allowed,
                &p,
                &json!({"ok":true,"team_prefs":{"toplevel_prefs":{}}}),
                None
            )
            .is_err()
        );
    }
}
#[test]
fn explicit_workspace_in_grid_keeps_workspace_permission_scope_but_org_pronouns() {
    let mut w = workspace("T1");
    w.enterprise_id = Some("E1".into());
    let b = boot("T1");
    let request = own_profile_permissions_request(&SlackClient::default(), &w, &b).unwrap();
    let RequestBody::Json(body) = request.body else {
        panic!("json")
    };
    assert!(body.get("team_ids").is_none());
    assert_eq!(body["as_admin"], true);
    let cap = decode(
        &w,
        &b,
        &json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]}),
        Some(&json!({"ok":true,"locked_prefs":{"display_pronouns":false}})),
    );
    assert!(!cap.capability(Field::Pronouns).allowed());
}

#[test]
fn messaging_extras_is_authority_and_embedded_or_workspace_prefs_cannot_grant() {
    let w = workspace("T1");
    let init = boot("T1");
    let permissions = json!({"ok":true,"allowed":["UPDATE_NAME","UPDATE_DISPLAY_NAME"]});
    let extras = json!({"ok":true,"team_prefs":{"workspace_prefs":{"T1":{"auth_mode":"normal","display_pronouns":true}}}});
    let cap = FreshOwnProfileCapabilities::from_responses(
        &w,
        &init,
        &permissions,
        &profile(),
        &extras,
        None,
    )
    .unwrap();
    assert!(cap.capability(Field::FullName).allowed());
    assert!(matches!(
        cap.capability(Field::DisplayName),
        ProfileEditCapability::Unknown(_)
    ));
    assert!(matches!(
        cap.capability(Field::Pronouns),
        ProfileEditCapability::Unknown(_)
    ));
    let request = own_profile_team_preferences_request(&SlackClient::default(), &w);
    assert!(request.url.contains("/api/client.extras?"));
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let extras = fields.iter().find(|(k, _)| k == "extras").unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&extras.1).unwrap(),
        json!(["team_prefs"])
    );
}
