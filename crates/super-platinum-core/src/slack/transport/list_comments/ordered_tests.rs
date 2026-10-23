use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        Document, Record,
        comments::{ReplyQuery, Target, Tombstones},
    },
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn request(address: std::net::SocketAddr) -> PreparedRequest {
    let workspace = WorkspaceSession {
        team_id: "TTEST".into(),
        user_id: "UTEST".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture".into(),
    };
    let document = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let record = Record::parse(json!({"id":"Rec1","fields":[]})).unwrap();
    let mut request = api::list_record_comments_replies(
        &SlackClient::default(),
        &workspace,
        &Target::capture(&document, &record).unwrap(),
        &ReplyQuery::default(),
        Tombstones::Include,
    )
    .unwrap();
    request.url = format!("http://{address}/api/conversations.replies");
    request
}

async fn received(stream: &mut TcpStream) {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0 && bytes.len() + count < 128 * 1024);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                assert!(headers.starts_with("POST /api/conversations.replies HTTP/1.1"));
                let fields: BTreeMap<_, _> = url::form_urlencoded::parse(&bytes[end + 4..])
                    .into_owned()
                    .collect();
                assert_eq!(fields["channel"], "C01234567");
                assert_eq!(fields["ts"], "F01234567-Rec1");
                assert_eq!(fields["_x_reason"], "history-api/fetchReplies");
                return;
            }
        }
    }
}

async fn exchange(
    status: u16,
    extra: &str,
    body: &str,
    ordered: bool,
) -> Result<Option<comments::ordered::ResponseWitness>, Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let request = request(listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        received(&mut stream).await;
        stream.write_all(response.as_bytes()).await.unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    });
    let transport = Transport::new("fixture-cookie").unwrap();
    let result = if ordered {
        transport
            .execute_list_comments_read_ordered(request, || true)
            .await
            .map(Some)
    } else {
        transport
            .execute_list_comments_read(request, || true)
            .await
            .map(|value| {
                assert_eq!(value, json!({"ok":false,"error":"denied","opaque":7}));
                None
            })
    };
    server.await.unwrap();
    result
}

#[tokio::test]
async fn original_reply_order_and_unknown_fields_survive_the_single_http_body() {
    let body = r#"{"ok":true,"messages":[{"ts":"1.000001","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"a","style":{"italic":false,"bold":true}}]}]}],"opaque":{"z":7}}]}"#;
    let witness = exchange(200, "", body, true).await.unwrap().unwrap();
    assert_eq!(
        witness.response(),
        &serde_json::from_str::<Value>(body).unwrap()
    );
    let message = witness.message(0).unwrap();
    assert_eq!(message.style_keys(&[0, 0, 0]), Some(vec!["italic", "bold"]));
    assert_eq!(message.raw_message()["opaque"], json!({"z":7}));
}

#[tokio::test]
async fn failed_duplicate_and_invalid_envelopes_never_become_ordered_receipts() {
    for body in [
        r#"{"ok":false,"messages":[]}"#,
        r#"{"ok":true,"error":"denied","messages":[]}"#,
        r#"{"ok":true,"messages":{}}"#,
    ] {
        assert!(matches!(
            exchange(200, "", body, true).await,
            Err(Error::Api(_))
        ));
    }
    for body in [
        r#"{"ok":true,"ok":true,"messages":[]}"#,
        r#"{"ok":true,"messages":[{"ts":"1.1","style":{"bold":true,"bold":false}}]}"#,
        r#"[]"#,
    ] {
        assert!(matches!(
            exchange(200, "", body, true).await,
            Err(Error::Transport(_))
        ));
    }
}

#[tokio::test]
async fn retry_after_and_rate_limits_precede_order_decoding_without_retry() {
    for status in [200, 429, 503] {
        let error = exchange(status, "Retry-After: 7\r\n", "not-json", true)
            .await
            .unwrap_err();
        match error {
            Error::RateLimited { retry_after_secs } => {
                assert_eq!(status, 429);
                assert_eq!(retry_after_secs, Some(7));
            }
            Error::HttpStatus {
                status: actual,
                retry_after_secs,
            } => {
                assert_eq!(actual, status);
                assert_ne!(status, 429);
                assert_eq!(retry_after_secs, Some(7));
            }
            error => panic!("unexpected error: {error:?}"),
        }
    }
    assert!(matches!(
        exchange(200, "", r#"{"ok":false,"error":"ratelimited"}"#, true).await,
        Err(Error::RateLimited {
            retry_after_secs: None
        })
    ));
}

#[tokio::test]
async fn legacy_value_reader_keeps_its_existing_api_envelope_behavior() {
    assert!(
        exchange(
            200,
            "",
            r#"{"ok":false,"error":"denied","opaque":7}"#,
            false
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn retired_or_non_reply_request_is_refused_before_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let original = request(listener.local_addr().unwrap());
    assert!(
        matches!(transport.execute_list_comments_read_ordered(original.clone(), || false).await,
        Err(Error::Api(code)) if code == "list_comment_owner_changed")
    );
    let mut other = original;
    other.url = other.url.replace("conversations.replies", "chat.update");
    assert!(
        matches!(transport.execute_list_comments_read_ordered(other, || true).await,
        Err(Error::Api(code)) if code == "unsupported_list_comments_read")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn retirement_during_original_response_wait_refuses_adoption_and_never_retries() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let request = request(listener.local_addr().unwrap());
    let current = Arc::new(AtomicBool::new(true));
    let server_current = current.clone();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        received(&mut stream).await;
        server_current.store(false, Ordering::SeqCst);
        released.await.unwrap();
        // The dispatched server operation may finish; retirement only refuses its adoption.
        let body = r#"{"ok":true,"messages":[]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    });
    let transport = Transport::new("fixture-cookie").unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        transport.execute_list_comments_read_ordered(request, || current.load(Ordering::SeqCst)),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(Error::Api(code)) if code == "list_comment_owner_changed"));
    release.send(()).unwrap();
    server.await.unwrap();
}
