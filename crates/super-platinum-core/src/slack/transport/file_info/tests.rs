use super::*;
use crate::{config::WorkspaceSession, slack::SlackClient};
use std::cell::Cell;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(enterprise: bool) -> PreparedRequest {
    let owner = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: enterprise.then(|| "E1".into()),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    SlackClient::default().rest_form(
        &owner,
        "files.info",
        vec![
            ("file", "F1".into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
        ],
    )
}
#[test]
fn only_exact_singleton_factory_is_admitted() {
    for enterprise in [false, true] {
        assert!(allowed(&request(enterprise)));
    }
    for mutation in 0..11 {
        let mut request = request(false);
        match mutation {
            0 => request.method = "GET",
            1 => {
                request.url = request
                    .url
                    .replace("files.info", "search.autocomplete.files")
            }
            2 => request.url = request.url.replace("https:", "http:"),
            3 => {
                request.url = request
                    .url
                    .replace("fixture.slack.com", "slack.com.invalid")
            }
            4 => request.url.push_str("#bad"),
            5 => request.body = RequestBody::Json(serde_json::json!({})),
            _ => {
                let RequestBody::Form(fields) = &mut request.body else {
                    unreachable!()
                };
                match mutation {
                    6 => fields.push(("file".into(), "F1".into())),
                    7 => fields.push(("extra".into(), "true".into())),
                    8 => fields
                        .iter_mut()
                        .find(|(k, _)| k == "token")
                        .unwrap()
                        .1
                        .clear(),
                    9 => fields.iter_mut().find(|(k, _)| k == "count").unwrap().1 = "100".into(),
                    _ => {
                        fields.iter_mut().find(|(k, _)| k == "truncate").unwrap().1 = "false".into()
                    }
                }
            }
        }
        assert!(!allowed(&request), "mutation {mutation}");
    }
}
#[tokio::test]
async fn retired_owner_is_checked_before_and_after_unsent_link_boundary() {
    let transport = Transport::disconnected_fixture("synthetic");
    for after in [false, true] {
        let calls = Cell::new(0);
        let result = transport
            .execute_file_info_current(request(false), || {
                calls.set(calls.get() + 1);
                after && calls.get() == 1
            })
            .await;
        assert!(matches!(result,Err(Error::Offline(ref text)) if text=="File info owner retired"));
        assert_eq!(calls.get(), if after { 2 } else { 1 });
    }
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
        let mut input = [0; 8192];
        stream.read(&mut input).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    let mut request = request(false);
    request.url = format!("http://{address}/api/files.info");
    (request, server)
}
#[tokio::test]
async fn actual_http_bytes_preserve_singleton_map_order_and_team() {
    let body = br#"{"ok":true,"file":{"id":"F1","title":"Canonical","mode":"quip","created":1,"channels":[],"channel_map":{"z":{"id":"CZ","team":"T1"},"a":{"id":"CA","team":"T1"}}}}"#;
    let (request, server) = respond(
        "200 OK",
        "Content-Type: application/json; charset=windows-1252\r\n",
        body,
    )
    .await;
    let transport = Transport::new("synthetic").unwrap();
    let received = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_file_info_once(&request),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    let mut store = crate::slack::api::custom::texty::file_title_source::Store::new();
    store.ingest_info("T1", &received).unwrap();
    assert_eq!(
        store.candidate("F1").unwrap().effective()["channels"],
        serde_json::json!(["CZ", "CA"])
    );
    assert_eq!(store.candidate("F1").unwrap().receipt(), received.raw());
}
#[tokio::test]
async fn invalid_utf8_is_rejected_instead_of_replaced_before_observation() {
    let (request, server) = respond(
        "200 OK",
        "Content-Type: application/json; charset=windows-1252\r\n",
        b"{\"ok\":true,\"files\":[{\"id\":\"F1\",\"title\":\"\xff\"}]}",
    )
    .await;
    let transport = Transport::new("synthetic").unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_file_info_once(&request),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert!(
        matches!(result,Err(Error::Transport(ref text)) if text=="invalid original response UTF8")
    );
}
#[tokio::test]
async fn actual_http_status_rate_limit_and_api_errors_preserve_classification() {
    for (status, headers, body, kind) in [
        ("429 Too Many Requests", "Retry-After: 17\r\n", "", "rate"),
        ("503 Service Unavailable", "Retry-After: 19\r\n", "", "http"),
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"fixture_failure"}"#,
            "api",
        ),
        ("200 OK", "", "{", "json"),
    ] {
        let (request, server) = respond(status, headers, body.as_bytes()).await;
        let transport = Transport::new("synthetic").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.execute_file_info_once(&request),
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
