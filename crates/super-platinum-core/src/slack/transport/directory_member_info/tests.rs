use super::*;
use serde_json::json;

fn request() -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: "https://edgeapi.slack.com/cache/T00000001/users/info".into(),
        headers: vec![],
        body: RequestBody::Json(
            json!({"token":"synthetic","ids":["U00000001"],"check_interaction":true,"include_profile_only_users":true}),
        ),
    }
}
#[test]
fn closed_info_wrapper_refuses_other_paths_mixed_id_modes_and_write_fields() {
    let req = request();
    assert!(allowed(&req));
    assert!(!req.retry_safe());
    let mut conditional = request();
    let RequestBody::Json(body) = &mut conditional.body else {
        panic!("json")
    };
    body.as_object_mut().unwrap().remove("ids");
    body["updated_ids"] = json!({"U00000001":101});
    assert!(allowed(&conditional));
    assert!(!conditional.retry_safe());
    for url in [
        "http://edgeapi.slack.com/cache/T00000001/users/info",
        "https://evil.invalid/cache/T00000001/users/info",
        "https://edgeapi.slack.com/cache/T00000001/users/search",
        "https://user@edgeapi.slack.com/cache/T00000001/users/info",
        "https://edgeapi.slack.com/cache/T00000001/users/info/",
    ] {
        let mut req = request();
        req.url = url.into();
        assert!(!allowed(&req));
    }
    for (field, value) in [
        // Adding updated_ids while retaining ids is still a forbidden mixture.
        ("updated_ids", json!({"U00000001":1})),
        ("text", json!("write")),
        ("check_interaction", json!(false)),
        ("include_profile_only_users", json!(false)),
        ("ids", json!([])),
    ] {
        let mut req = request();
        let RequestBody::Json(body) = &mut req.body else {
            panic!("json")
        };
        body[field] = value;
        assert!(!allowed(&req));
    }
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    req.url = format!("http://{address}/cache/T00000001/users/info");
    (req, task)
}
#[tokio::test]
async fn shared_success_decoder_captures_order_without_rewriting_payload() {
    let body = r#"{"results":{"z":[{"id":"U00000001"}],"a":[{"id":"U00000002"}]},"unknown":{"keep":true}}"#;
    let (req, server) = respond("200 OK", "", body).await;
    let transport = Transport::new("synthetic-cookie").unwrap();
    let ordered = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_directory_member_once(&req),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    let (value, keys) = ordered.into_parts();
    assert_eq!(value, serde_json::from_str::<Value>(body).unwrap());
    assert_eq!(keys, Some(vec!["z".into(), "a".into()]));
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
        let transport = Transport::new("synthetic-cookie").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.execute_directory_member_once(&req),
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
