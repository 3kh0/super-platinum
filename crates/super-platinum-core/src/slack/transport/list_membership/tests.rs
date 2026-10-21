use super::*;
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{SlackClient, api::custom::texty::list_membership::Read},
};
use std::{cell::Cell, time::Instant};
fn request(membership: bool) -> PreparedRequest {
    let owner = WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    };
    let now = Instant::now();
    let endpoint=DirectoryEndpoint::decode(&owner,&serde_json::json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/T00000001"}),now).unwrap();
    let read = if membership {
        Read::membership(&owner, "CLIST0001", vec!["ULIST0001".into()]).unwrap()
    } else {
        Read::first_members(&owner, "CLIST0001").unwrap()
    };
    endpoint
        .list_membership(&SlackClient::default(), &owner, now, &read)
        .unwrap()
}
fn disconnected() -> Transport {
    Transport {
        http: None,
        canvas_http: Arc::new(std::sync::OnceLock::new()),
        d_cookie: "synthetic".into(),
        health: Arc::new(watch::Sender::new(Health::Unknown)),
    }
}
#[test]
fn only_exact_factory_shapes_and_secure_original_route_are_admitted() {
    for membership in [false, true] {
        assert!(allowed(&request(membership)));
        for url in [
            "http://edgeapi.slack.com/cache/T1/users/list",
            "https://other.test/cache/T1/users/list",
            "https://edgeapi.slack.com/cache/T1/users/search",
            "https://edgeapi.slack.com/cache/T1/users/list#fragment",
        ] {
            let mut req = request(membership);
            req.url = url.into();
            assert!(!allowed(&req));
        }
        let mut req = request(membership);
        req.method = "GET".into();
        assert!(!allowed(&req));
        for key in ["token", "enterprise_token", "extra"] {
            let mut req = request(membership);
            let RequestBody::Json(ref mut body) = req.body else {
                panic!("JSON")
            };
            body[key] = Value::Null;
            assert!(!allowed(&req));
        }
    }
    let mut req = request(false);
    let RequestBody::Json(ref mut body) = req.body else {
        panic!("JSON")
    };
    body["count"] = 100.into();
    assert!(!allowed(&req));
    let mut req = request(true);
    let RequestBody::Json(ref mut body) = req.body else {
        panic!("JSON")
    };
    body["as_admin"] = true.into();
    assert!(!allowed(&req));
}
#[tokio::test]
async fn http_none_is_never_upgraded_by_membership_read() {
    for membership in [false, true] {
        let transport = disconnected();
        let calls = Cell::new(0);
        let result = transport
            .execute_list_membership_current(request(membership), || {
                calls.set(calls.get() + 1);
                true
            })
            .await;
        assert!(
            matches!(result,Err(Error::Offline(ref text)) if text=="disconnected fixture transport")
        );
        assert_eq!(calls.get(), 2);
        assert!(transport.http.is_none());
    }
}
#[tokio::test]
async fn original_owner_checked_before_hold_and_again_after_unsent_failure() {
    for retire_at in [1, 2] {
        let transport = disconnected();
        let calls = Cell::new(0);
        let result = transport
            .execute_list_membership_current(request(false), || {
                calls.set(calls.get() + 1);
                calls.get() < retire_at
            })
            .await;
        assert!(
            matches!(result,Err(Error::Offline(ref text)) if text=="Profile member search owner retired")
        );
        assert_eq!(calls.get(), retire_at);
    }
}
#[tokio::test]
async fn invalid_read_refuses_before_owner_callback_or_dispatch() {
    let transport = disconnected();
    let calls = Cell::new(0);
    let mut req = request(false);
    req.url = req.url.replace("users/list", "users/search");
    let result = transport
        .execute_list_membership_current(req, || {
            calls.set(calls.get() + 1);
            true
        })
        .await;
    assert!(matches!(result,Err(Error::Api(ref text)) if text=="invalid_list_membership_request"));
    assert_eq!(calls.get(), 0);
}
