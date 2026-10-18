use super::*;
use crate::{
    block_actions::home::workflow::directory_observations as observed, config::WorkspaceSession,
    slack::SlackClient,
};
use serde_json::json;
use std::cell::Cell;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request() -> PreparedRequest {
    observed::offline_request(
        &SlackClient::default(),
        &WorkspaceSession {
            team_id: "T00000001".into(),
            enterprise_id: None,
            user_id: "U00000001".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        },
    )
}
fn disconnected() -> Transport {
    Transport {
        http: None,
        canvas_http: Arc::new(std::sync::OnceLock::new()),
        d_cookie: "synthetic".into(),
        health: Arc::new(watch::Sender::new(Health::Unknown)),
    }
}
fn loopback() -> Transport {
    let mut t = disconnected();
    t.http = Some(wreq::Client::builder().no_proxy().build().unwrap());
    t
}
#[test]
fn original_factory_and_enterprise_host_are_admitted_with_existing_retry_classification() {
    let req = request();
    assert!(allowed(&req));
    // This POST is not on PreparedRequest's retry-safe list; do not add it here.
    assert!(!req.retry_safe());
    let mut enterprise = req;
    enterprise.url =
        "https://fixture.enterprise.slack.com/api/search.autocomplete.offlineFeatures".into();
    assert!(allowed(&enterprise));
}
#[test]
fn exact_factory_body_refuses_extra_duplicate_changed_and_missing_fields() {
    for fields in [
        vec![],
        vec![("token", "synthetic")],
        vec![("token", ""), ("_x_reason", "search-offline-features")],
        vec![("token", "synthetic"), ("_x_reason", "other")],
        vec![("token", "synthetic"), ("token", "other")],
        vec![
            ("token", "synthetic"),
            ("_x_reason", "search-offline-features"),
            ("extra", "x"),
        ],
    ] {
        let mut req = request();
        req.body = RequestBody::Form(
            fields
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        );
        assert!(!allowed(&req));
    }
    let mut req = request();
    req.body =
        RequestBody::Json(json!({"token":"synthetic","_x_reason":"search-offline-features"}));
    assert!(!allowed(&req));
    req = request();
    req.method = "GET";
    assert!(!allowed(&req));
}
#[test]
fn only_the_exact_slack_endpoint_is_allowed() {
    for url in [
        "https://evil.test/api/search.autocomplete.offlineFeatures",
        "https://slack.com.evil.test/api/search.autocomplete.offlineFeatures",
        "http://fixture.slack.com/api/search.autocomplete.offlineFeatures",
        "https://fixture.slack.com:444/api/search.autocomplete.offlineFeatures",
        "https://user@fixture.slack.com/api/search.autocomplete.offlineFeatures",
        "https://fixture.slack.com/api/search.autocomplete.offlineFeatures#x",
        "https://fixture.slack.com/api/search.autocomplete.offlineFeatures/",
        "https://fixture.slack.com/api/search.autocomplete.model",
    ] {
        let mut req = request();
        req.url = url.into();
        assert!(!allowed(&req));
    }
}
#[tokio::test]
async fn retired_owner_refuses_before_link_hold_and_invalid_request_before_owner() {
    let t = disconnected();
    t.set_health(Health::Offline);
    let result = t
        .execute_autocomplete_offline_current(request(), || false)
        .await;
    assert!(matches!(result,Err(Error::Offline(ref v)) if v=="Autocomplete offline owner retired"));
    let mut req = request();
    req.method = "GET";
    let result = t
        .execute_autocomplete_offline_current(req, || panic!("not admitted"))
        .await;
    assert!(matches!(result,Err(Error::Api(ref v)) if v=="invalid_autocomplete_offline_request"));
}
#[tokio::test]
async fn current_owner_preserves_disconnected_error_and_post_hold_check() {
    let calls = Cell::new(0);
    let t = disconnected();
    let result = t
        .execute_autocomplete_offline_current(request(), || {
            calls.set(calls.get() + 1);
            true
        })
        .await;
    assert!(matches!(result,Err(Error::Offline(ref v)) if v=="disconnected fixture transport"));
    assert_eq!(calls.get(), 2);
}
#[tokio::test]
async fn owner_retired_during_actual_offline_hold_cannot_dispatch_on_recovery() {
    // Closed loopback proxy never grants CONNECT; even an admission regression
    // cannot reach Slack. No real credentials or network dependency.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = wreq::Proxy::all(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let mut t = disconnected();
    t.http = Some(
        wreq::Client::builder()
            .no_proxy()
            .proxy(proxy)
            .build()
            .unwrap(),
    );
    t.set_health(Health::Offline);
    let current = Cell::new(true);
    let calls = Cell::new(0);
    let mut read = Box::pin(t.execute_autocomplete_offline_current(request(), || {
        calls.set(calls.get() + 1);
        current.get()
    }));
    assert!(futures::poll!(read.as_mut()).is_pending());
    assert_eq!(calls.get(), 1);
    current.set(false);
    t.set_health(Health::Online);
    let result = tokio::time::timeout(Duration::from_secs(1), read)
        .await
        .unwrap();
    assert!(matches!(result,Err(Error::Offline(ref v)) if v=="Autocomplete offline owner retired"));
    assert_eq!(calls.get(), 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
}
async fn respond(
    status: &str,
    extra: &str,
    body: &str,
) -> (PreparedRequest, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let bytes=format!("HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).into_bytes();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut input = [0; 8192];
        stream.read(&mut input).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    let mut req = request();
    req.url = format!("http://{address}/api/search.autocomplete.offlineFeatures");
    (req, server)
}
#[tokio::test]
async fn actual_http_decode_keeps_priority_tie_order_and_original_payload() {
    let wire = r#"{"ok":true,"channel_priority_90d":{"Cz":7,"Ca":8,"Cz":8},"results":{"unrelated":[]},"unknown":{"keep":true}}"#;
    let (req, server) = respond("200 OK", "", wire).await;
    let t = loopback();
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        t.execute_autocomplete_offline_once(&req),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    let (raw, keys) = response.into_parts();
    assert_eq!(raw, serde_json::from_str::<Value>(wire).unwrap());
    assert_eq!(keys, Some(vec!["Cz".into(), "Ca".into()]));
    let features = observed::Offline::decode_ordered(&raw, keys.as_deref()).unwrap();
    assert_eq!(features.processed_top_channels().unwrap(), ["Cz", "Ca"]);
    assert_eq!(t.health(), Health::Online);
}
#[tokio::test]
async fn ordered_decode_keeps_http_api_and_json_error_categories() {
    for (status, extra, wire, kind) in [
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
        let (req, server) = respond(status, extra, wire).await;
        let t = loopback();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            t.execute_autocomplete_offline_once(&req),
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
            "api" => assert!(matches!(result,Err(Error::Api(ref v)) if v=="fixture_failure")),
            _ => assert!(matches!(result, Err(Error::Transport(_)))),
        }
        assert_eq!(t.health(), Health::Online);
    }
}
