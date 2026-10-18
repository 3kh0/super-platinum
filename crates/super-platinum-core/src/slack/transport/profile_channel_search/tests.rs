use super::*;
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{
        SlackClient,
        api::custom::texty::channels::{Page, Query, RequestFacts},
    },
};
use serde_json::json;
use std::{cell::Cell, time::Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn planned(
    raw: &str,
    facts: RequestFacts,
    marker: Option<&str>,
    enterprise: bool,
) -> PreparedRequest {
    let owner = WorkspaceSession {
        team_id: if enterprise { "E00000001" } else { "T00000001" }.into(),
        enterprise_id: enterprise.then(|| "E00000001".into()),
        user_id: "U00000999".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
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
    Query::capture(&owner, raw, facts)
        .unwrap()
        .request(&SlackClient::default(), &owner, &endpoint, now, marker)
        .unwrap()
}
fn request() -> PreparedRequest {
    planned("#ship", RequestFacts::default(), None, false)
}
fn disconnected() -> Transport {
    Transport {
        http: None,
        canvas_http: Arc::new(std::sync::OnceLock::new()),
        d_cookie: "synthetic".into(),
        health: Arc::new(watch::Sender::new(Health::Unknown)),
    }
}
fn local_transport() -> Transport {
    let mut transport = disconnected();
    transport.http = Some(wreq::Client::builder().no_proxy().build().unwrap());
    transport
}

#[test]
fn actual_factory_raw_query_hints_markers_and_enterprise_plan_are_admitted_exactly() {
    for top in [
        json!([]),
        json!(["C00000002", "C00000001", "C00000002"]),
        json!({"opaque":1}),
        json!(true),
        json!(7),
    ] {
        for enterprise in [false, true] {
            let req = planned(
                " #Cafe\u{301}\0\n ",
                RequestFacts {
                    top_channels: Some(top.clone()),
                    default_workspace: Some("T00000002".into()),
                },
                Some("cursor"),
                enterprise,
            );
            assert!(allowed(&req));
            assert!(!req.retry_safe());
            let RequestBody::Json(body) = req.body else {
                unreachable!()
            };
            assert_eq!(body["query"], " #Cafe\u{301}\0\n ");
            assert_eq!(body["count"], 25);
            assert_eq!(body["filter"], "xws");
            assert_eq!(body["top_channels"], top);
            assert_eq!(body["default_workspace"], "T00000002");
            assert_eq!(body.get("enterprise_token").is_some(), enterprise);
        }
    }
    assert!(allowed(&planned("", RequestFacts::default(), None, false)));
    assert!(allowed(&planned(
        &"x".repeat(4097),
        RequestFacts::default(),
        Some(&"m".repeat(4096)),
        false
    )));
}

#[test]
fn closed_body_and_origin_refuse_nonfactory_fields_without_narrowing_source_hints() {
    for (key, value) in [
        ("token", json!("")),
        ("enterprise_token", json!(false)),
        ("query", json!(false)),
        ("query", json!("x".repeat(1_048_577))),
        ("count", json!(20)),
        ("fuzz", json!(2)),
        ("filter", json!("NOT deactivated")),
        ("include_record_channels", json!(false)),
        ("check_membership", json!(false)),
        ("ids", json!(["C00000001"])),
        ("top_channels", Value::Null),
        ("top_channels", json!(false)),
        ("top_channels", json!(0)),
        ("top_channels", json!("x".repeat(1_048_577))),
        ("default_workspace", json!("")),
        ("default_workspace", json!(true)),
        ("default_workspace", json!("T".repeat(4097))),
        ("marker", json!("")),
        ("marker", json!("bad\nmarker")),
        ("marker", json!("m".repeat(4097))),
    ] {
        let mut req = request();
        let RequestBody::Json(body) = &mut req.body else {
            unreachable!()
        };
        body[key] = value;
        assert!(!allowed(&req), "{key}");
    }
    for url in [
        "http://edgeapi.slack.com/cache/T00000001/channels/search",
        "https://evil.test/cache/T00000001/channels/search",
        "https://edgeapi.slack.com:444/cache/T00000001/channels/search",
        "https://edgeapi.slack.com/cache/T00000001/users/search",
        "https://edgeapi.slack.com/cache/T00000001/channels/info",
        "https://edgeapi.slack.com/cache/T00000001/mpims/search",
        "https://edgeapi.slack.com/cache/T00000001/channels/search/",
        "https://edgeapi.slack.com/cache//channels/search",
        "https://edgeapi.slack.com/cache/X00000001/channels/search",
        "https://edgeapi.slack.com/cache/T%2F00000001/channels/search",
        "https://user@edgeapi.slack.com/cache/T00000001/channels/search",
        "https://edgeapi.slack.com/cache/T00000001/channels/search#fragment",
    ] {
        let mut req = request();
        req.url = url.into();
        assert!(!allowed(&req));
    }
    let mut req = request();
    req.method = "GET";
    assert!(!allowed(&req));
    req.method = "POST";
    req.body = RequestBody::Form(vec![]);
    assert!(!allowed(&req));
    req.body = RequestBody::Json(json!([]));
    assert!(!allowed(&req));
}

#[tokio::test]
async fn retired_owner_refuses_before_link_hold_or_http_capability() {
    let transport = disconnected();
    transport.set_health(Health::Offline);
    let calls = Cell::new(0);
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        transport.execute_profile_channel_search_current(request(), || {
            calls.set(calls.get() + 1);
            false
        }),
    )
    .await
    .unwrap();
    assert!(
        matches!(result,Err(Error::Offline(ref text)) if text=="Profile channel search owner retired")
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(transport.health(), Health::Offline);
}

#[tokio::test]
async fn disconnected_current_owner_preserves_existing_link_failure_after_current_recheck() {
    let transport = disconnected();
    let calls = Cell::new(0);
    let result = transport
        .execute_profile_channel_search_current(request(), || {
            calls.set(calls.get() + 1);
            true
        })
        .await;
    assert!(
        matches!(result,Err(Error::Offline(ref text)) if text=="disconnected fixture transport")
    );
    assert_eq!(calls.get(), 2);
}

#[tokio::test]
async fn original_owner_retired_during_offline_hold_never_dispatches_after_recovery() {
    // A regression can reach only this local proxy, never Slack. It grants no CONNECT.
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
        transport.execute_profile_channel_search_current(request(), || {
            calls.set(calls.get() + 1);
            current.get()
        }),
    );
    assert!(futures::poll!(read.as_mut()).is_pending());
    assert_eq!(calls.get(), 1);
    current.set(false);
    transport.set_health(Health::Online);
    let result = tokio::time::timeout(Duration::from_secs(1), read)
        .await
        .unwrap();
    assert!(
        matches!(result,Err(Error::Offline(ref text)) if text=="Profile channel search owner retired")
    );
    assert_eq!(calls.get(), 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn invalid_request_refuses_before_owner_callback_or_dispatch() {
    let mut req = request();
    req.url = "http://127.0.0.1:1/cache/T00000001/channels/search".into();
    let result = disconnected()
        .execute_profile_channel_search_current(req, || {
            panic!("invalid request cannot admit owner")
        })
        .await;
    assert!(
        matches!(result,Err(Error::Api(ref text)) if text=="invalid_profile_channel_search_request")
    );
}

async fn respond(
    status: &str,
    extra: &str,
    body: &str,
) -> (PreparedRequest, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let bytes = format!("HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).into_bytes();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        stream.read(&mut request).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    let mut req = request();
    req.url = format!("http://{address}/cache/T00000001/channels/search");
    (req, server)
}

#[tokio::test]
async fn same_byte_object_order_and_raw_outcomes_flow_into_actual_profile_page_decoder() {
    let body = r#"{"ok":true,"results":{"z":[{"id":"COLD"}],"10":{"id":"CTEN"},"2":[{"id":"CTWO"}],"a":[{"id":"CA"}],"z":[{"id":"CZ","opaque":true},{"id":"CZ","opaque":"duplicate"}]},"mpims":[{"id":"GMPIM"}],"huddles":[{"id":"CHUDDLE"}],"member_channels":["CZ"],"pending_ids":["CPENDING"],"failed_ids":["CFAILED"],"invalid_ids":["CINVALID"],"next_marker":"next","unknown":{"keep":true}}"#;
    let (req, server) = respond("200 OK", "", body).await;
    let transport = local_transport();
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_profile_channel_search_once(&req),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    let (raw, keys) = response.into_parts();
    assert_eq!(raw, serde_json::from_str::<Value>(body).unwrap());
    assert_eq!(
        keys,
        Some(vec!["2".into(), "10".into(), "z".into(), "a".into()])
    );
    let page = Page::decode_ordered(raw, keys.as_deref()).unwrap();
    assert_eq!(
        page.rows()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["CTWO", "CTEN", "CZ", "CZ", "CA", "GMPIM", "CHUDDLE"]
    );
    assert_eq!(page.marker(), Some("next"));
    assert_eq!(page.outcomes().membership_ids, ["CZ"]);
    assert_eq!(page.outcomes().pending_ids, ["CPENDING"]);
    assert_eq!(page.outcomes().failed_ids, ["CFAILED"]);
    assert_eq!(page.outcomes().invalid_ids, ["CINVALID"]);
    assert_eq!(transport.health(), Health::Online);
}

#[tokio::test]
async fn shared_error_categories_retry_after_and_health_remain_unchanged() {
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
        let transport = local_transport();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.execute_profile_channel_search_once(&req),
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
            "api" => assert!(matches!(result,Err(Error::Api(ref text)) if text=="fixture_failure")),
            _ => assert!(matches!(result, Err(Error::Transport(_)))),
        }
        assert_eq!(transport.health(), Health::Online);
    }
}
