use super::*;
use super_platinum_core::{
    block_actions::posting_policy::Flags,
    channel_workflows::{Previews, feature_permission::Permission},
};

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        enterprise_id: None,
        user_id: "UWORKFLOW".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-fixture-token".into(),
    }
}
fn request(target: &str, trigger: &str) -> protocol::Request {
    let session = session();
    let raw = json!({"id":target,"is_member":true});
    let member = json!({"id":session.user_id,"is_restricted":false});
    let previews = Previews::decode(&[], json!({"ok":true,"triggers":[]})).unwrap();
    protocol::Add::new(
        &json!({"id":trigger,"type":"shortcut"}),
        &raw,
        &previews,
        protocol::Facts {
            team_id: &session.team_id,
            top_level_team_id: &session.team_id,
            self_member: &member,
            posting: Flags {
                read_only: false,
                thread_only: false,
            },
            can_post: true,
            manage: Permission::Allowed,
        },
    )
    .unwrap()
    .request(&SlackClient::default(), &session)
    .unwrap()
}
#[test]
fn original_closed_factory_target_and_token_redaction_are_exact() {
    let s = session();
    let client = SlackClient::default();
    let raw = inspect(&request(raw::DEST, TRIGGER), &client, &s).unwrap();
    assert_eq!(
        raw["fields"],
        json!([
            ["trigger_ids", TRIGGER],
            ["channel_id", raw::DEST],
            ["_x_reason", protocol::REASON]
        ])
    );
    assert!(!raw.to_string().contains(&s.token));
    assert!(inspect(&request(raw::PRIVATE, TRIGGER), &client, &s).is_err());
    assert!(inspect(&request(raw::DEST, "TR_OTHER"), &client, &s).is_err());
    let mut foreign = s.clone();
    foreign.url = "https://foreign.slack.com".into();
    assert!(inspect(&request(raw::DEST, TRIGGER), &client, &foreign).is_err());
}
#[test]
fn permission_factory_binds_actual_user_team_and_channel_without_token_in_ledger() {
    let s = session();
    let c = SlackClient::default();
    let p = PermissionRead::capture(&json!({"id":raw::DEST}), &s).unwrap();
    let req = p.request(&c, &s, &discovery(&s)).unwrap();
    let next = p.request(&c, &s, &discovery(&s)).unwrap();
    assert_ne!(req.url, next.url);
    assert!(super::super::super::picker::same_request(&req, &next).is_ok());
    assert_eq!(method(&req), Some("permissions/info"));
    assert_eq!(
        safe_body(&req.body),
        json!({"user_id":s.user_id,"channel_id":raw::DEST,"permissions":["MANAGE_CHANNEL_WORKFLOWS"]})
    );
    let mut wrong = req.clone();
    wrong.url = wrong.url.replace("TWORKFLOW", "TOTHER001");
    assert!(super::super::super::picker::same_request(&wrong, &req).is_err());
    let mut wrong = req.clone();
    if let RequestBody::Json(body) = &mut wrong.body {
        body["permissions"] = json!(["MANAGE_CHANNEL_TABS"]);
    }
    assert!(super::super::super::picker::same_request(&wrong, &req).is_err());
    assert!(!safe_body(&req.body).to_string().contains(&s.token));
}
#[test]
fn fresh_readback_is_separate_from_ack_and_missing_case_remains_missing() {
    let before = channel(false, "success").unwrap();
    let after = channel(true, "success").unwrap();
    assert!(before.pointer("/properties/channel_workflows").is_none());
    assert_eq!(
        after.pointer("/properties/channel_workflows/0/workflow_trigger_id"),
        Some(&json!(TRIGGER))
    );
    assert_eq!(channel(true, "readback-missing").unwrap(), before);
    let mut reduced = after.clone();
    reduced.as_object_mut().unwrap().remove("properties");
    assert_eq!(reduced, before);
    assert_eq!(channel(false, "success").unwrap(), before);
}
#[test]
fn each_held_response_releases_once_and_retired_receiver_is_reported() {
    let m = Model::new();
    let (tx, mut rx) = oneshot::channel();
    m.0.lock().unwrap().held.push((1, tx));
    assert_eq!(
        m.release(1).unwrap(),
        json!({"serial":1,"receiverRetired":false,"delivered":true})
    );
    assert_eq!(rx.try_recv(), Ok(()));
    assert!(m.release(1).is_err());
    let (tx, rx) = oneshot::channel();
    drop(rx);
    m.0.lock().unwrap().held.push((2, tx));
    assert_eq!(
        m.release(2).unwrap(),
        json!({"serial":2,"receiverRetired":true,"delivered":false})
    );
}
#[test]
fn dropped_pending_future_records_nonreturn_without_grant_or_attempt() {
    let m = Model::new();
    drop(Settlement {
        model: m.clone(),
        serial: 1,
        returned: false,
    });
    let snapshot = m.snapshot();
    assert_eq!(
        snapshot["settlements"],
        json!([{"serial":1,"returned":false}])
    );
    assert_eq!(snapshot["acknowledged"], false);
    assert_eq!(snapshot["attempts"], json!([]));
}
#[test]
fn named_case_allowlist_and_form_redaction_remain_closed() {
    for s in [
        "success",
        "denied",
        "rejected",
        "readback-missing",
        "readback-failure",
        "held",
    ] {
        assert!(supported(s));
    }
    for s in ["", "unknown", "first-add-success", "success-extra"] {
        assert!(!supported(s));
    }
    assert_eq!(
        safe_body(&RequestBody::Form(vec![
            ("token".into(), "secret".into()),
            ("enterprise_token".into(), "secret2".into()),
            ("trigger_ids".into(), TRIGGER.into())
        ])),
        json!([["trigger_ids", TRIGGER]])
    );
}

#[test]
fn actual_add_preflight_remains_with_parent_while_browse_is_separate() {
    let session = session();
    let client = SlackClient::default();
    let add = client.rest_form(
        &session,
        "bookmarks.list",
        vec![
            ("channel", raw::DEST.into()),
            ("include_folders", "true".into()),
            ("include_legacy_workflows", "true".into()),
            ("_x_reason", "bookmarks-list-api".into()),
        ],
    );
    let browse = Read::bookmarks(raw::DEST)
        .unwrap()
        .request(&client, &session);
    assert!(add_preflight(&add));
    assert!(!add_preflight(&browse));
    let mut foreign = add.clone();
    foreign.url = foreign
        .url
        .replace("/bookmarks.list", "/conversations.info");
    assert!(!add_preflight(&foreign));
}
