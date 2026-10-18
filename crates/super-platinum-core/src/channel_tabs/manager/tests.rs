use super::*;
use serde_json::json;
fn inputs() -> (Value, User, PostingFacts) {
    let raw = json!({"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"properties":{"who_can_manage_tabs":"managers","tabz":[{"id":null,"type":"files","opaque":[1,2]}]}});
    let user: User = serde_json::from_value(json!({"id":"U00000001","deleted":false})).unwrap();
    let mut facts = PostingFacts::new("C00000001".into());
    facts.observe(&raw, Some(&user));
    (raw, user, facts)
}
fn response() -> Value {
    json!({"ok":true,"role_assignments":[{"role_id":"Rl0A","users":["U00000001"]}]})
}
#[test]
fn exact_private_request_uses_authenticated_viewer_and_channel_not_public_endpoint() {
    let (raw, user, facts) = inputs();
    let read = ManagerRead::capture(&raw, &user, &facts).unwrap();
    let mut session = WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: Some("E00000001".into()),
        user_id: user.id,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let request = read.request(&SlackClient::default(), &session).unwrap();
    assert_eq!(
        request.url.split('?').next(),
        Some("https://fixture.enterprise.slack.com/api/admin.roles.entity.listAssignments")
    );
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            ("entity_id".into(), "C00000001".into())
        ]
    );
    session.user_id = "U00000002".into();
    assert!(matches!(
        read.request(&SlackClient::default(), &session),
        Err(Fault::Changed)
    ));
}
#[test]
fn positive_manager_grant_supports_paid_and_enterprise_explicit_policy_without_rewriting_inventory()
{
    let (raw, user, facts) = inputs();
    let before = raw.clone();
    for enterprise in [false, true] {
        assert_eq!(
            permission(&raw, &user, &facts, enterprise, Some("paid")),
            Permission::Unavailable
        );
        assert_eq!(
            ManagerRead::capture(&raw, &user, &facts)
                .unwrap()
                .permission(&response()),
            Ok(Permission::Allowed)
        );
    }
    assert_eq!(raw, before);
    assert_eq!(
        Inventory::from_channel(&raw).unwrap().rows()[0],
        before["properties"]["tabz"][0]
    );
}
#[test]
fn other_role_or_viewer_missing_and_empty_are_not_grants_or_definite_acl_denials() {
    let (raw, user, facts) = inputs();
    let read = ManagerRead::capture(&raw, &user, &facts).unwrap();
    for value in [
        json!({"ok":true,"role_assignments":[]}),
        json!({"ok":true,"role_assignments":[{"role_id":"Other","users":["U00000001"]}]}),
        json!({"ok":true,"role_assignments":[{"role_id":"Rl0A","users":["U00000002"]}]}),
    ] {
        assert_eq!(read.permission(&value), Ok(Permission::Unavailable));
    }
}
#[test]
fn malformed_error_and_conflicting_role_rows_do_not_grant() {
    let (raw, user, facts) = inputs();
    let read = ManagerRead::capture(&raw, &user, &facts).unwrap();
    for value in [
        json!({"ok":false}),
        json!({"ok":true,"error":"permission_denied","role_assignments":[{"role_id":"Rl0A","users":["U00000001"]}]}),
        json!({"ok":true,"role_assignments":[{"role_id":"Rl0A","users":["U00000001"]},{"role_id":"Rl0A","users":[]}]}),
        json!({"ok":true,"role_assignments":[{"role_id":"Rl0A","users":["U00000001",null]}]}),
        json!({"ok":true,"roleAssignments":[{"roleId":"Rl0A","users":["U00000001"]}]}),
    ] {
        assert!(read.permission(&value).is_err());
    }
}
#[test]
fn manager_ingress_cannot_bypass_posting_membership_or_invent_default_enterprise_acl() {
    for change in 0..6 {
        let (mut raw, mut user, _) = inputs();
        match change {
            0 => raw["is_member"] = false.into(),
            1 => raw["is_read_only"] = true.into(),
            2 => raw["is_im"] = true.into(),
            3 => user.deleted = true,
            4 => {
                raw["properties"]
                    .as_object_mut()
                    .unwrap()
                    .remove("who_can_manage_tabs");
            }
            _ => raw["properties"]["who_can_manage_tabs"] = "everyone".into(),
        }
        let mut facts = PostingFacts::new("C00000001".into());
        facts.observe(&raw, Some(&user));
        assert!(
            ManagerRead::capture(&raw, &user, &facts).is_err(),
            "case {change}"
        );
    }
}
