use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api::custom::texty::files},
};
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
    files::request(&SlackClient::default(), &owner, " @Cafe\u{301} ")
}
#[test]
fn actual_factory_is_closed_without_extra_options_or_enterprise_body_changes() {
    for enterprise in [false, true] {
        let request = request(enterprise);
        assert!(allowed(&request));
        assert!(!request.retry_safe());
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert_eq!(fields.len(), 4);
        assert!(fields.contains(&("query".into(), " @Cafe\u{301} ".into())));
        assert!(request.url.contains(if enterprise {
            "fixture.enterprise.slack.com"
        } else {
            "fixture.slack.com"
        }));
    }
    for mutation in 0..12 {
        let mut request = request(false);
        match mutation {
            0 => request.method = "GET",
            1 => {
                request.url = request
                    .url
                    .replace("fixture.slack.com", "slack.com.invalid")
            }
            2 => request.url = request.url.replace("https:", "http:"),
            3 => {
                request.url = request
                    .url
                    .replace("search.autocomplete.files", "files.info")
            }
            4 => request.url = request.url.replace("https://", "https://user@"),
            5 => request.url.push_str("#fragment"),
            6 => request.body = RequestBody::Json(serde_json::json!({})),
            _ => {
                let RequestBody::Form(fields) = &mut request.body else {
                    unreachable!()
                };
                match mutation {
                    7 => fields.push(("token".into(), "synthetic".into())),
                    8 => fields.push(("count".into(), "25".into())),
                    9 => fields
                        .iter_mut()
                        .find(|(k, _)| k == "token")
                        .unwrap()
                        .1
                        .clear(),
                    10 => {
                        fields
                            .iter_mut()
                            .find(|(k, _)| k == "include_shares")
                            .unwrap()
                            .1 = "false".into()
                    }
                    _ => {
                        fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 =
                            "other".into()
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
            .execute_profile_file_search_current(request(false), || {
                calls.set(calls.get() + 1);
                after && calls.get() == 1
            })
            .await;
        assert!(
            matches!(result,Err(Error::Offline(ref text)) if text=="Profile file search owner retired")
        );
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
    request.url = format!("http://{address}/api/search.autocomplete.files");
    (request, server)
}
#[tokio::test]
async fn actual_http_utf8_ignores_charset_conversion_and_keeps_original_map_order() {
    let body = "{\"ok\":true,\"files\":[{\"id\":\"F1\",\"title\":\"Caf?\",\"mode\":\"quip\",\"created\":1,\"channels\":[],\"channel_map\":{\"z\":{\"id\":\"CZ\",\"team\":\"T1\"},\"a\":{\"id\":\"CA\",\"team\":\"T1\"}}}]}";
    let (request, server) = respond(
        "200 OK",
        "Content-Type: application/json; charset=windows-1252\r\n",
        body.as_bytes(),
    )
    .await;
    let transport = Transport::new("synthetic").unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_profile_file_search_once(&request),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    assert_eq!(response.raw()["files"][0]["title"], "Caf?");
    let mut store = crate::slack::api::custom::texty::file_title_source::Store::new();
    let owner = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    store
        .ingest_batch_ordered(
            response.raw(),
            response.rows(),
            &owner,
            response.channel_map_order(),
        )
        .unwrap();
    assert_eq!(
        store.candidate("F1").unwrap().effective()["channels"],
        serde_json::json!(["CZ", "CA"])
    );
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
        transport.execute_profile_file_search_once(&request),
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
            transport.execute_profile_file_search_once(&request),
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
