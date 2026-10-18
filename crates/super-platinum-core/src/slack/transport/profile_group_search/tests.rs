use super::*;
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{SlackClient, api::custom::texty::groups::Query},
};
use serde_json::json;
use std::time::Instant;

fn request(enterprise: bool, org_wide: bool) -> PreparedRequest {
    let owner = WorkspaceSession {
        team_id: if enterprise { "E00000001" } else { "T00000001" }.into(),
        enterprise_id: enterprise.then(|| "E00000001".into()),
        user_id: "U00000001".into(),
        name: "Synthetic".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    };
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &owner,
        &json!({"ok":true,"ttl_seconds":60,
        "url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id)}),
        now,
    )
    .unwrap();
    Query::capture(&owner, " @Cafe\u{301}\0\n ", org_wide)
        .unwrap()
        .request(
            &SlackClient::default(),
            &owner,
            &endpoint,
            now,
            Some("cursor"),
        )
        .unwrap()
}

#[test]
fn actual_group_factory_workspace_enterprise_and_org_wide_requests_are_admitted() {
    for enterprise in [false, true] {
        for org_wide in [false, true] {
            let req = request(enterprise, org_wide);
            assert!(allowed(&req));
            assert!(!req.retry_safe());
            let RequestBody::Json(body) = req.body else {
                panic!("expected JSON")
            };
            assert_eq!(body["org_wide"], org_wide);
            assert_eq!(body["count"], 25);
            assert_eq!(body["query"], " @Cafe\u{301}\0\n ");
            assert_eq!(body.get("enterprise_token").is_some(), enterprise);
        }
    }
}

#[test]
fn group_read_rejects_wrong_endpoint_auth_payload_and_member_only_options() {
    for mode in 0..13 {
        let mut req = request(false, true);
        match mode {
            0 => req.method = "GET".into(),
            1 => req.url = req.url.replace("edgeapi.slack.com", "example.invalid"),
            2 => req.url = req.url.replace("usergroups/search", "users/search"),
            3 => req.url = req.url.replace("usergroups/search", "usergroups/info"),
            _ => {
                let RequestBody::Json(body) = &mut req.body else {
                    panic!("expected JSON")
                };
                match mode {
                    4 => body["token"] = json!(""),
                    5 => body["count"] = json!(100),
                    6 => {
                        body.as_object_mut().unwrap().remove("org_wide");
                    }
                    7 => body["org_wide"] = json!("true"),
                    8 => body["fuzz"] = json!(1),
                    9 => body["top_users"] = json!([]),
                    10 => body["enable_workspace_ranking"] = json!(true),
                    11 => body["query"] = json!(false),
                    _ => body["marker"] = json!("bad\n"),
                }
            }
        }
        assert!(!allowed(&req), "mode {mode}");
    }
}

#[tokio::test]
async fn retired_group_owner_and_invalid_request_never_reach_transport() {
    let transport = Transport::disconnected_fixture("synthetic");
    assert!(matches!(
        transport
            .execute_profile_group_search_current(request(false, true), || false)
            .await,
        Err(Error::Offline(_))
    ));
    let mut req = request(false, true);
    req.url = req.url.replace("usergroups/search", "usergroups/info");
    assert!(matches!(
        transport
            .execute_profile_group_search_current(req, || true)
            .await,
        Err(Error::Api(_))
    ));
}
