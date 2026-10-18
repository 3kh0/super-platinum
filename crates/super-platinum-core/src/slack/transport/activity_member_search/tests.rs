use super::*;
use crate::{
    block_actions::{
        directory_endpoint::DirectoryEndpoint,
        workflow_management::activity_members::{Page, Query},
    },
    config::WorkspaceSession,
    slack::SlackClient,
};
use serde_json::json;
use std::{cell::Cell, time::Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request() -> PreparedRequest {
    let owner = WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000999".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    };
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &owner,
        &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/T00000001"}),
        now,
    )
    .unwrap();
    Query::capture(&owner, "@Ada")
        .unwrap()
        .request(&SlackClient::default(), &owner, &endpoint, now, None)
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

fn loopback_transport() -> Transport {
    let mut transport = disconnected();
    // Ignore environment proxies: all response fixtures bind 127.0.0.1 only.
    transport.http = Some(wreq::Client::builder().no_proxy().build().unwrap());
    transport
}

#[test]
fn actual_query_plan_is_admitted_without_changing_safe_retry_policy() {
    let req = request();
    assert!(allowed(&req));
    assert!(!req.retry_safe());
    let RequestBody::Json(body) = req.body else {
        panic!("JSON plan")
    };
    assert_eq!(body["query"], "Ada");
    assert_eq!(body["count"], 20);
    assert_eq!(body["filter"], "NOT deactivated");
}

#[test]
fn closed_body_rejects_invalid_tokens_markers_and_other_body_encodings() {
    for (field, value) in [
        ("query", json!("a\0b")),
        ("query", json!("x".repeat(4097))),
        ("query", json!(false)),
        ("enterprise_token", json!("")),
        ("marker", json!("bad\nmarker")),
        ("marker", json!("x".repeat(1025))),
        ("enable_workspace_ranking", json!(false)),
        ("include_profile_only_users", json!(false)),
        ("ids", json!(["U00000001"])),
    ] {
        let mut req = request();
        let RequestBody::Json(body) = &mut req.body else {
            unreachable!()
        };
        body[field] = value;
        assert!(!allowed(&req), "{field}");
    }
    let mut req = request();
    req.body = RequestBody::Form(vec![]);
    assert!(!allowed(&req));
    req.body = RequestBody::Json(json!([]));
    assert!(!allowed(&req));
    for url in [
        "https://edgeapi.slack.com:444/cache/T00000001/users/search",
        "https://edgeapi.slack.com/cache//users/search",
        "https://edgeapi.slack.com/cache/T%2F00000001/users/search",
        "https://edgeapi.slack.com/cache/T-00000001/users/search",
        "not a URL",
    ] {
        let mut req = request();
        req.url = url.into();
        assert!(!allowed(&req));
    }
}

#[tokio::test]
async fn retired_owner_refuses_before_offline_hold_or_http_capability() {
    let transport = disconnected();
    transport.set_health(Health::Offline);
    let calls = Cell::new(0);
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        transport.execute_activity_member_search_current(request(), || {
            calls.set(calls.get() + 1);
            false
        }),
    )
    .await
    .unwrap();
    assert!(
        matches!(result, Err(Error::Offline(ref text)) if text == "Activity member search owner retired")
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(transport.health(), Health::Offline);
}

#[tokio::test]
async fn disconnected_current_owner_retains_link_failure_and_rechecks_after_await() {
    let transport = disconnected();
    let calls = Cell::new(0);
    let result = transport
        .execute_activity_member_search_current(request(), || {
            calls.set(calls.get() + 1);
            true
        })
        .await;
    assert!(
        matches!(result, Err(Error::Offline(ref text)) if text == "disconnected fixture transport")
    );
    assert_eq!(calls.get(), 2);
}

#[tokio::test]
async fn owner_retired_during_real_offline_hold_cannot_dispatch_after_recovery() {
    // Any accidental send can only reach this local proxy, never Slack. No
    // proxy server grants CONNECT, so even a failing regression is offline.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = wreq::Proxy::all(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let mut transport = disconnected();
    transport.http = Some(
        wreq::Client::builder()
            .no_proxy()
            .proxy(proxy)
            .build()
            .unwrap(),
    );
    transport.set_health(Health::Offline);
    let current = Cell::new(true);
    let calls = Cell::new(0);
    let mut read = Box::pin(
        transport.execute_activity_member_search_current(request(), || {
            calls.set(calls.get() + 1);
            current.get()
        }),
    );
    assert!(futures::poll!(read.as_mut()).is_pending());
    assert_eq!(calls.get(), 1, "actually parked inside await_link");
    current.set(false);
    transport.set_health(Health::Online);
    let result = tokio::time::timeout(Duration::from_secs(1), read)
        .await
        .unwrap();
    assert!(
        matches!(result, Err(Error::Offline(ref text)) if text == "Activity member search owner retired")
    );
    assert_eq!(calls.get(), 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn public_wrapper_rejects_nonallowed_request_before_current_or_send() {
    let transport = disconnected();
    let mut req = request();
    req.url = "http://127.0.0.1:1/cache/T00000001/users/search".into();
    let result = transport
        .execute_activity_member_search_current(req, || {
            panic!("invalid request has no owner admission")
        })
        .await;
    assert!(
        matches!(result, Err(Error::Api(ref text)) if text == "invalid_activity_member_search_request")
    );
}

#[tokio::test]
async fn retained_order_oracle_runs_through_shared_http_decoder() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let (req, server) = respond("200 OK", "", case["wire"].as_str().unwrap()).await;
        let transport = loopback_transport();
        let response = tokio::time::timeout(
            Duration::from_secs(3),
            transport.execute_activity_member_once(&req),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        let (raw, keys) = response.into_parts();
        assert_eq!(raw, case["raw"], "{}", case["label"]);
        assert_eq!(json!(keys), case["keys"], "{}", case["label"]);
        // The retained oracle includes intentionally domain-invalid groups;
        // raw/order custody is independent of Page's later validation.
    }
}

#[test]
fn closed_search_admission_does_not_widen_endpoints_or_request_shapes() {
    let req = request();
    assert!(allowed(&req));
    assert!(!req.retry_safe());
    for url in [
        "https://evil.test/cache/T00000001/users/search",
        "http://edgeapi.slack.com/cache/T00000001/users/search",
        "https://edgeapi.slack.com/api/chat.postMessage",
        "https://edgeapi.slack.com/cache/T00000001/channels/search",
        "https://edgeapi.slack.com/cache/T00000001/users/search/",
        "https://user@edgeapi.slack.com/cache/T00000001/users/search",
        "https://edgeapi.slack.com/cache/T00000001/users/search#fragment",
    ] {
        let mut req = request();
        req.url = url.into();
        assert!(!allowed(&req));
    }
    for (key, value) in [
        ("count", json!(21)),
        ("filter", json!("NOT bots")),
        ("fuzz", json!(2)),
        ("marker", json!("")),
        ("extra", json!(true)),
        ("token", json!("")),
    ] {
        let mut req = request();
        let RequestBody::Json(body) = &mut req.body else {
            unreachable!()
        };
        body[key] = value;
        assert!(!allowed(&req));
    }
    let mut req = request();
    req.method = "GET";
    assert!(!allowed(&req));
    let mut req = request();
    let RequestBody::Json(body) = &mut req.body else {
        unreachable!()
    };
    body["enterprise_token"] = json!("synthetic-enterprise");
    body["marker"] = json!("cursor");
    assert!(allowed(&req));
}

async fn respond(
    status: &str,
    extra: &str,
    body: &str,
) -> (PreparedRequest, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let bytes=format!("HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).into_bytes();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut input = [0; 8192];
        stream.read(&mut input).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    let mut req = request();
    req.url = format!("http://{address}/cache/T00000001/users/search");
    (req, task)
}
#[tokio::test]
async fn shared_success_decoder_captures_order_without_rewriting_payload() {
    let body = r#"{"results":{"z":[{"id":"U00000001"}],"a":[{"id":"U00000002"}]},"unknown":{"keep":true}}"#;
    let (req, server) = respond("200 OK", "", body).await;
    let transport = loopback_transport();
    let value = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_activity_member_once(&req),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    let (value, keys) = value.into_parts();
    assert_eq!(value, serde_json::from_str::<Value>(body).unwrap());
    assert_eq!(keys, Some(vec!["z".into(), "a".into()]));
    let page = Page::decode_ordered(value, keys.as_deref()).unwrap();
    assert_eq!(
        page.rows()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["U00000001", "U00000002"]
    );
    assert_eq!(transport.health(), Health::Online);
}
#[tokio::test]
async fn shared_http_api_and_json_errors_keep_existing_categories_and_retry_header() {
    for (status, extra, body, kind) in [
        ("429 Too Many Requests", "Retry-After: 17\r\n", "", "rate"),
        ("503 Service Unavailable", "Retry-After: 19\r\n", "", "http"),
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"fixture_failure"}"#,
            "api",
        ),
        ("200 OK", "", "{", "decode"),
    ] {
        let (req, server) = respond(status, extra, body).await;
        let transport = loopback_transport();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.execute_activity_member_once(&req),
        )
        .await
        .unwrap();
        server.await.unwrap();
        match kind {
            "rate" => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(17)
                })
            )),
            "http" => assert!(matches!(
                result,
                Err(Error::HttpStatus {
                    status: 503,
                    retry_after_secs: Some(19)
                })
            )),
            "api" => assert!(matches!(result,Err(Error::Api(ref e)) if e=="fixture_failure")),
            _ => assert!(matches!(result, Err(Error::Transport(_)))),
        }
        assert_eq!(transport.health(), Health::Online);
    }
}
