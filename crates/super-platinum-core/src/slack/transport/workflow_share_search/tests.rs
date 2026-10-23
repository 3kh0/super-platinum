use super::*;
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::SlackClient,
    workflow_share::search::{Kind, Query, Read},
};
use serde_json::json;
use std::time::Instant;
fn request(kind: Kind) -> PreparedRequest {
    let w = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    };
    let now = Instant::now();
    let e = DirectoryEndpoint::decode(
        &w,
        &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/T1"}),
        now,
    )
    .unwrap();
    Read::new(&w, kind, &Query::new("Ada").unwrap(), None)
        .unwrap()
        .request(&SlackClient::default(), &w, &e, now)
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
fn share_search_closed_factory_admits_only_exact_preset() {
    for kind in [Kind::Members, Kind::Channels] {
        assert!(allowed(&request(kind)));
        assert!(!request(kind).retry_safe());
        for (key, value) in [
            ("count", json!(25)),
            ("fuzz", json!(2)),
            ("filter", json!("")),
            ("top_users", json!([])),
            ("query", json!(null)),
            ("marker", json!("")),
            ("include_profile_only_users", json!(false)),
        ] {
            let mut r = request(kind);
            let RequestBody::Json(b) = &mut r.body else {
                unreachable!()
            };
            b[key] = value;
            assert!(!allowed(&r), "{key}");
        }
    }
}
#[test]
fn share_search_closed_routes_body_and_methods() {
    for url in [
        "http://edgeapi.slack.com/cache/T1/users/search",
        "https://foreign.test/cache/T1/users/search",
        "https://edgeapi.slack.com/cache/T1/users/info",
        "https://edgeapi.slack.com/cache/T1/mpims/search",
        "https://user@edgeapi.slack.com/cache/T1/users/search",
        "https://edgeapi.slack.com/cache/T1/users/search#x",
    ] {
        let mut r = request(Kind::Members);
        r.url = url.into();
        assert!(!allowed(&r));
    }
    let mut r = request(Kind::Members);
    r.method = "GET";
    assert!(!allowed(&r));
    r.method = "POST";
    r.body = RequestBody::Form(vec![]);
    assert!(!allowed(&r));
}
#[tokio::test]
async fn share_search_retired_owner_never_dispatches() {
    let t = disconnected();
    assert!(matches!(
        t.execute_workflow_share_search_current(request(Kind::Members), || false)
            .await,
        Err(Error::Offline(_))
    ));
}
#[tokio::test]
async fn share_search_current_offline_preserves_issued_failure() {
    let t = disconnected();
    assert!(
        matches!(t.execute_workflow_share_search_current(request(Kind::Channels),||true).await,Err(Error::Offline(ref s)) if s=="disconnected fixture transport")
    );
}
#[test]
fn share_search_same_bytes_order_survives_decode_and24_aggregation() {
    let response = directory_response::decode_field_bytes(
        br#"{"ok":true,"results":{"z":{"id":"Z"},"2":{"id":"TWO"},"a":[{"id":"A"}]}}"#,
        "results",
    )
    .unwrap();
    let (raw, keys) = response.into_parts();
    assert_eq!(keys.as_ref().unwrap(), &["2", "z", "a"]);
    let mut pages = crate::workflow_share::search::Pages::default();
    pages.accept(raw, keys.as_deref()).unwrap();
    assert_eq!(
        pages
            .rows()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["TWO", "Z", "A"]
    );
}

#[tokio::test]
async fn share_search_retirement_after_unsent_link_failure_overrides_adoption() {
    let t = disconnected();
    let calls = std::cell::Cell::new(0);
    let result = t
        .execute_workflow_share_search_current(request(Kind::Members), || {
            calls.set(calls.get() + 1);
            calls.get() == 1
        })
        .await;
    assert!(
        matches!(result,Err(Error::Offline(ref s)) if s=="Workflow Share search owner retired")
    );
    assert_eq!(calls.get(), 3);
}
#[tokio::test]
async fn share_search_invalid_wire_rejected_before_owner_or_network() {
    let t = disconnected();
    let mut r = request(Kind::Members);
    let RequestBody::Json(b) = &mut r.body else {
        unreachable!()
    };
    b["count"] = json!(25);
    let result = t
        .execute_workflow_share_search_current(r, || panic!("invalid wire queried owner"))
        .await;
    assert!(matches!(result,Err(Error::Api(ref s)) if s=="invalid_workflow_share_search_request"));
}
