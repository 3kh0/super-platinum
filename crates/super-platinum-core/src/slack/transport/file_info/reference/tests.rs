use super::*;
use crate::{config::WorkspaceSession, slack::SlackClient};
use std::cell::Cell;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request() -> PreparedRequest {
    SlackClient::default().rest_form(
        &WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        },
        "files.info",
        vec![
            ("file", "F1".into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
        ],
    )
}
async fn respond(
    status: &str,
    headers: &str,
    body: &[u8],
) -> (PreparedRequest, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut bytes = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.read(&mut [0; 8192]).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    let mut req = request();
    req.url = format!("http://{address}/api/files.info");
    (req, server)
}

#[tokio::test]
async fn exact_request_and_current_owner_are_required_before_network() {
    let transport = Transport::disconnected_fixture("synthetic");
    let mut bad = request();
    bad.body = RequestBody::Json(serde_json::json!({"file":"F1"}));
    assert!(
        matches!(transport.execute_file_reference_current(bad, || true).await, Err(Error::Api(ref text)) if text=="invalid_file_info_request")
    );
    for after in [false, true] {
        let calls = Cell::new(0);
        let result = transport
            .execute_file_reference_current(request(), || {
                calls.set(calls.get() + 1);
                after && calls.get() == 1
            })
            .await;
        assert!(matches!(result,Err(Error::Offline(ref text)) if text=="File info owner retired"));
        assert_eq!(calls.get(), if after { 2 } else { 1 });
    }
}

#[tokio::test]
async fn actual_response_bytes_retain_only_three_api_error_carriers_without_reclassifying_others() {
    for (status, headers, body, expected) in [
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"not_visible","filetype":"quip","title":"Secret","opaque":{"keep":1}}"#,
            "private",
        ),
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"template_not_visible","opaque":9}"#,
            "template",
        ),
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"file_deleted","title":"Ignored secret","opaque":9}"#,
            "deleted",
        ),
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"file_not_found"}"#,
            "missing",
        ),
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"access_denied"}"#,
            "api",
        ),
        ("429 Too Many Requests", "Retry-After: 17\r\n", "", "rate"),
        ("503 Service Unavailable", "Retry-After: 19\r\n", "", "http"),
        ("200 OK", "", "{", "json"),
    ] {
        let (req, server) = respond(status, headers, body.as_bytes()).await;
        let transport = Transport::new("synthetic").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.file_reference_once(&req, "F1"),
        )
        .await
        .unwrap();
        server.await.unwrap();
        match expected {
            "private" | "template" | "deleted" => {
                let response = result.unwrap();
                assert_eq!(
                    response.raw(),
                    &serde_json::from_str::<Value>(body).unwrap()
                );
                assert_eq!(response.source()["id"], "F1");
                if expected == "private" {
                    assert_eq!(response.source()["file_access"], "not_visible");
                } else if expected == "template" {
                    assert_eq!(response.source()["mode"], "template_not_visible");
                } else {
                    assert_eq!(
                        response.source(),
                        &serde_json::json!({"id":"F1","mode":"tombstone","is_deleted":true})
                    );
                }
            }
            "missing" => {
                assert!(matches!(result,Err(Error::Api(ref text)) if text=="file_not_found"))
            }
            "api" => assert!(matches!(result,Err(Error::Api(ref text)) if text=="access_denied")),
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
            _ => assert!(matches!(result, Err(Error::Transport(_)))),
        }
    }
}

#[tokio::test]
async fn late_status_cannot_escape_current_read_fence() {
    for body in [
        br#"{"ok":false,"error":"not_visible"}"#.as_slice(),
        br#"{"ok":false,"error":"file_deleted"}"#.as_slice(),
    ] {
        let (req, server) = respond("200 OK", "", body).await;
        let transport = Transport::new("synthetic").unwrap();
        let calls = Cell::new(0);
        let result = transport
            .file_reference_admitted(&req, "F1", &|| {
                calls.set(calls.get() + 1);
                calls.get() < 3
            })
            .await;
        server.await.unwrap();
        assert!(matches!(result,Err(Error::Offline(ref text)) if text=="File info owner retired"));
        assert_eq!(calls.get(), 3);
    }
}

#[tokio::test]
async fn legacy_file_deleted_remains_api_error_without_reference_admission() {
    let (req, server) = respond(
        "200 OK",
        "",
        br#"{"ok":false,"error":"file_deleted","opaque":9}"#,
    )
    .await;
    let transport = Transport::new("synthetic").unwrap();
    assert!(
        matches!(transport.execute_file_info_once(&req).await, Err(Error::Api(ref code)) if code == "file_deleted")
    );
    server.await.unwrap();
}

#[tokio::test]
async fn legacy_api_error_behavior_and_original_order_are_unchanged() {
    let (req, server) = respond(
        "200 OK",
        "",
        br#"{"ok":false,"error":"not_visible","title":"Secret"}"#,
    )
    .await;
    let transport = Transport::new("synthetic").unwrap();
    assert!(
        matches!(transport.execute_file_info_once(&req).await,Err(Error::Api(ref text)) if text=="not_visible")
    );
    server.await.unwrap();
    let (req, server) = respond("200 OK","",br#"{"ok":true,"file":{"id":"F1","mode":"quip","title":"Normal","created":1,"channels":[],"channel_map":{"z":{"id":"CZ","team":"T1"},"a":{"id":"CA","team":"T1"}}}}"#).await;
    let response = transport.file_reference_once(&req, "F1").await.unwrap();
    server.await.unwrap();
    let mut store = crate::slack::api::custom::texty::file_title_source::Store::new();
    store.ingest_reference("T1", &response).unwrap();
    assert_eq!(
        store.candidate("F1").unwrap().effective()["channels"],
        serde_json::json!(["CZ", "CA"])
    );
}

#[tokio::test]
async fn wrong_file_and_invalid_original_utf8_are_never_reference_receipts() {
    for body in [
        br#"{"ok":true,"file":{"id":"F2","mode":"quip"}}"#.as_slice(),
        b"{\"ok\":false,\"error\":\"not_visible\",\"title\":\"\xff\"}",
    ] {
        let (req, server) = respond("200 OK", "", body).await;
        let transport = Transport::new("synthetic").unwrap();
        assert!(transport.file_reference_once(&req, "F1").await.is_err());
        server.await.unwrap();
    }
}
