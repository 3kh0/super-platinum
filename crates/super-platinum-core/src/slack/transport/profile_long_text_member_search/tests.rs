use super::*;
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{
        SlackClient,
        api::custom::texty::members::{Query, RequestFacts},
    },
};
use serde_json::json;
use std::{cell::Cell, time::Instant};

fn planned() -> PreparedRequest {
    let w = WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    };
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &w,
        &json!({"ok":true,"ttl_seconds":60,
        "url":"https://edgeapi.slack.com/cache/T00000001"}),
        now,
    )
    .unwrap();
    Query::capture(&w, " @raw\0\n ", RequestFacts::default())
        .unwrap()
        .request_for_profile_long_text(&SlackClient::default(), &w, &endpoint, now, Some("marker"))
        .unwrap()
}
#[test]
fn final_sdk_and_legacy_grammars_are_distinct_and_other_structural_guards_remain() {
    let req = planned();
    assert!(allowed(&req));
    assert!(!super::super::profile_member_search::allowed(&req));
    assert!(!req.retry_safe());
    let mut legacy = req.clone();
    let RequestBody::Json(body) = &mut legacy.body else {
        unreachable!()
    };
    body.as_object_mut()
        .unwrap()
        .remove("include_profile_only_users");
    assert!(!allowed(&legacy));
    assert!(super::super::profile_member_search::allowed(&legacy));
    for flag in [
        Value::Null,
        json!(false),
        json!("true"),
        json!(1),
        json!([]),
    ] {
        let mut bad = req.clone();
        let RequestBody::Json(body) = &mut bad.body else {
            unreachable!()
        };
        body["include_profile_only_users"] = flag;
        assert!(!allowed(&bad));
    }
    for (key, value) in [
        ("count", json!(24)),
        ("filter", json!("NOT deactivated")),
        ("unknown", json!(true)),
        ("marker", json!("bad\n")),
        ("token", json!("")),
    ] {
        let mut bad = req.clone();
        let RequestBody::Json(body) = &mut bad.body else {
            unreachable!()
        };
        body[key] = value;
        assert!(!allowed(&bad));
    }
    for address in [
        "http://edgeapi.slack.com/cache/T1/users/search",
        "https://example.test/cache/T1/users/search",
        "https://edgeapi.slack.com/cache/T1/channels/search",
        "https://name@edgeapi.slack.com/cache/T1/users/search",
        "https://edgeapi.slack.com:444/cache/T1/users/search",
    ] {
        let mut bad = req.clone();
        bad.url = address.into();
        assert!(!allowed(&bad));
    }
    let mut wrong_method = req.clone();
    wrong_method.method = "GET";
    assert!(!allowed(&wrong_method));
    // This layer checks grammar, not equality against a captured credential owner.
    let mut different = req;
    let RequestBody::Json(body) = &mut different.body else {
        unreachable!()
    };
    body["token"] = json!("other-nonempty");
    assert!(allowed(&different));
}
#[tokio::test]
async fn dedicated_executor_rejects_bad_recipe_before_current_and_retires_valid_read_before_link() {
    let transport = Transport {
        http: None,
        canvas_http: Arc::new(std::sync::OnceLock::new()),
        d_cookie: "synthetic".into(),
        health: Arc::new(watch::Sender::new(Health::Unknown)),
    };
    let calls = Cell::new(0);
    let mut bad = planned();
    let RequestBody::Json(body) = &mut bad.body else {
        unreachable!()
    };
    body["include_profile_only_users"] = json!(false);
    assert!(
        matches!(transport.execute_profile_long_text_member_search_current(bad,||{calls.set(calls.get()+1);false}).await,Err(Error::Api(s)) if s=="invalid_profile_long_text_member_search_request")
    );
    assert_eq!(calls.get(), 0);
    assert!(matches!(
        transport
            .execute_profile_long_text_member_search_current(planned(), || {
                calls.set(calls.get() + 1);
                false
            })
            .await,
        Err(Error::Offline(_))
    ));
    assert_eq!(calls.get(), 1);
}
