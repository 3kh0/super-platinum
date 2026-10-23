use super::*;

#[test]
fn ordinary_dm_has_exact_peer_without_member_permission_substitute() {
    let c = channel();
    let p = peer();
    assert_eq!(c["user"], p["id"]);
    assert_eq!(c["is_im"], true);
    assert!(c.get("is_member").is_none());
    assert_eq!(p["team_id"], "TWORKFLOW");
    assert_eq!(p["is_bot"], false);
    assert_eq!(p["is_app_user"], false);
}
#[test]
fn exact_peer_page_keeps_deleted_and_wrong_identity_distinct() {
    let read = member_info::Read::capture(vec![PEER.into()]).unwrap();
    let good = peer_reply("success");
    assert_eq!(good["can_interact"][PEER], true);
    assert!(member_info::Page::decode(&read, good, None).is_ok());
    let deleted = peer_reply("deleted");
    assert_eq!(deleted["results"][0]["deleted"], true);
    assert!(member_info::Page::decode(&read, deleted, None).is_ok());
    assert!(member_info::Page::decode(&read, peer_reply("mismatched-peer"), None).is_err());
}
#[test]
fn exact_known_peer_factory_requires_interaction_and_profile_flags() {
    let session = WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        enterprise_id: None,
        user_id: "UWORKFLOW".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-dm-test-token".into(),
    };
    let client = SlackClient::default();
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(&session, &discovery(), now).unwrap();
    let read = member_info::Read::capture(vec![PEER.into()]).unwrap();
    let request = endpoint.member_info(&client, &session, now, &read).unwrap();
    assert_eq!(
        safe_body(&request.body),
        json!({"ids":[PEER],"check_interaction":true,"include_profile_only_users":true})
    );
    let mut changed = request.clone();
    if let RequestBody::Json(v) = &mut changed.body {
        v["ids"] = json!(["USHARE002"]);
    }
    assert!(super::super::super::super::picker::same_request(&changed, &request).is_err());
}
