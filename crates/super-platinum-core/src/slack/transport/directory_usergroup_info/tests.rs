mod rate_limit;

use super::*;
use serde_json::json;
use std::sync::atomic::AtomicBool;

fn request(ordered: bool) -> PreparedRequest {
    if ordered {
        PreparedRequest {
            method: "POST",
            url: "https://edgeapi.slack.com/cache/T00000001/usergroups/info".into(),
            headers: vec![],
            body: RequestBody::Json(json!({"token":"synthetic","ids":["S00000001"]})),
        }
    } else {
        PreparedRequest {
            method: "POST",
            url: "https://fixture.slack.com/api/subteams.info".into(),
            headers: vec![],
            body: RequestBody::Form(vec![
                ("token".into(), "synthetic".into()),
                ("usergroups".into(), "S00000001".into()),
                ("include_users".into(), "false".into()),
                ("include_count".into(), "true".into()),
                ("include_disabled".into(), "true".into()),
                ("_x_reason".into(), "fallback:fetchRawUserGroupsById".into()),
            ]),
        }
    }
}
#[test]
fn closed_routes_and_body_refuse_cross_endpoint_or_write_fields() {
    for ordered in [false, true] {
        assert!(allowed(&request(ordered), ordered));
        assert!(!allowed(&request(ordered), !ordered));
        assert!(!request(ordered).retry_safe());
        for url in [
            "http://edgeapi.slack.com/cache/T00000001/usergroups/info",
            "https://evil.invalid/api/subteams.info",
            "https://user@fixture.slack.com/api/subteams.info",
            "https://fixture.slack.com/api/subteams.update",
            "https://edgeapi.slack.com/cache/T00000001/usergroups/search",
        ] {
            let mut r = request(ordered);
            r.url = url.into();
            assert!(!allowed(&r, ordered));
        }
    }
    for (key, value) in [
        ("updated_ids", json!({"S00000001":0})),
        ("include_users", json!(true)),
        ("ids", json!([])),
    ] {
        let mut r = request(true);
        let RequestBody::Json(body) = &mut r.body else {
            panic!("json");
        };
        body[key] = value;
        assert!(!allowed(&r, true));
    }
    let mut r = request(false);
    let RequestBody::Form(body) = &mut r.body else {
        panic!("form");
    };
    body.push(("usergroups".into(), "S00000002".into()));
    assert!(!allowed(&r, false));
}
#[tokio::test]
async fn retired_owner_refuses_before_hold_or_http_and_after_link_recovery() {
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    for ordered in [false, true] {
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            transport.execute_group_current(request(ordered), || false, ordered),
        )
        .await
        .unwrap();
        assert!(
            matches!(result,Err(Error::Offline(ref message)) if message.contains("owner retired"))
        );
        let owner = AtomicBool::new(true);
        let work = transport.execute_group_current(
            request(ordered),
            || owner.load(Ordering::Acquire),
            ordered,
        );
        let retire = async {
            tokio::task::yield_now().await;
            owner.store(false, Ordering::Release);
            transport.set_health(Health::Online);
        };
        let (result, ()) =
            tokio::time::timeout(Duration::from_secs(1), async { tokio::join!(work, retire) })
                .await
                .unwrap();
        assert!(
            matches!(result,Err(Error::Offline(ref message)) if message.contains("owner retired"))
        );
        transport.set_health(Health::Offline);
    }
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn response(
    status: &str,
    extra: &str,
    body: &str,
) -> (PreparedRequest, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let bytes=format!("HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).into_bytes();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut input = [0; 8192];
        socket.read(&mut input).await.unwrap();
        socket.write_all(&bytes).await.unwrap();
    });
    let mut req = request(true);
    req.url = format!("http://{address}/cache/T00000001/usergroups/info");
    (req, server)
}
#[tokio::test]
async fn shared_decoder_retains_actual_object_order_and_http_error_category() {
    let body = r#"{"ok":true,"results":{"z":[{"id":"S00000001"}],"a":[]},"future":1}"#;
    let (req, server) = response("200 OK", "", body).await;
    let transport = Transport::new("synthetic").unwrap();
    let page = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_group_once(&req, true),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    let (raw, keys) = page.into_parts();
    assert_eq!(raw, serde_json::from_str::<Value>(body).unwrap());
    assert_eq!(keys, Some(vec!["z".into(), "a".into()]));
    let (req, server) = response("429 Too Many Requests", "Retry-After: 17\r\n", "").await;
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_group_once(&req, false),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert!(matches!(
        result,
        Err(Error::RateLimited {
            retry_after_secs: Some(17)
        })
    ));
}
