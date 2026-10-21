use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CHANNEL: &str = "C01234567";
const TS: &str = "1700000001.000001";
const NAME: &str = "wave";
fn request(adding: bool) -> PreparedRequest {
    let workspace = WorkspaceSession {
        team_id: "TTEST".into(),
        user_id: "UTEST".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let factory = if adding {
        api::reactions_add
    } else {
        api::reactions_remove
    };
    factory(
        &SlackClient::default(),
        &workspace,
        CHANNEL.into(),
        TS.into(),
        NAME.into(),
    )
}
fn set(request: &mut PreparedRequest, key: &str, value: &str) {
    let RequestBody::Form(fields) = &mut request.body else {
        panic!("form")
    };
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = value.into();
    } else {
        fields.push((key.into(), value.into()));
    }
}
#[test]
fn exact_existing_factories_bind_operation_and_all_target_fields() {
    for adding in [true, false] {
        let r = request(adding);
        assert!(allowed(&r));
        assert!(matches_target(&r, CHANNEL, TS, NAME, adding));
        assert!(!matches_target(&r, CHANNEL, TS, NAME, !adding));
        assert!(!matches_target(&r, "C76543210", TS, NAME, adding));
        assert!(!matches_target(
            &r,
            CHANNEL,
            "1700000002.000001",
            NAME,
            adding
        ));
        assert!(!matches_target(&r, CHANNEL, TS, "other", adding));
        let f = fields(&r, path(adding)).unwrap();
        assert_eq!(f.len(), 4);
        assert_eq!(f["name"], NAME);
        assert_eq!(f["timestamp"], TS);
    }
}
#[test]
fn malformed_extra_duplicate_and_foreign_requests_are_not_admitted() {
    for (k, v) in [
        ("channel", "D01234567"),
        ("timestamp", "F01234567-Rec1"),
        ("timestamp", "bad"),
        ("name", ""),
        ("name", "bad name"),
        ("name", "bad\nname"),
        ("token", ""),
        ("ts", TS),
        ("file", "F01234567"),
    ] {
        let mut r = request(true);
        set(&mut r, k, v);
        assert!(!allowed(&r), "{k}:{v}");
    }
    let mut r = request(true);
    set(&mut r, "name", &"x".repeat(257));
    assert!(!allowed(&r));
    for name in ["+1", "thumbsup::skin-tone-2", "custom_name"] {
        let mut r = request(false);
        set(&mut r, "name", name);
        assert!(matches_target(&r, CHANNEL, TS, name, false));
    }
    for url in [
        "https://foreign.example/api/reactions.add",
        "https://fixture.slack.com/api/reactions.add/",
        "https://fixture.slack.com/api/reactions.add?unknown=1",
        "https://fixture.slack.com/api/chat.postMessage",
        "https://user@fixture.slack.com/api/reactions.add",
        "https://fixture.slack.com:444/api/reactions.add",
        "https://fixture.slack.com/api/reactions.add#fragment",
    ] {
        let mut r = request(true);
        r.url = url.into();
        assert!(!allowed(&r));
    }
    let mut r = request(true);
    r.method = "GET";
    assert!(!allowed(&r));
    let mut r = request(true);
    let RequestBody::Form(f) = &mut r.body else {
        panic!()
    };
    f.push(("name".into(), NAME.into()));
    assert!(!allowed(&r));
}

#[tokio::test]
async fn wrong_binding_stale_owner_and_offline_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let local = || {
        let mut r = request(true);
        r.url = format!("http://{addr}/api/reactions.add");
        r
    };
    for stale in [false, true] {
        assert!(matches!(
            transport
                .execute_list_comment_reaction(
                    local(),
                    CHANNEL,
                    TS,
                    if stale { NAME } else { "wrong" },
                    true,
                    || !stale
                )
                .await,
            Err(ListCommentReactionFailure::NotRequested(_))
        ));
    }
    let mut calls = 0;
    assert!(matches!(
        transport
            .execute_list_comment_reaction(local(), CHANNEL, TS, NAME, true, || {
                calls += 1;
                calls == 1
            })
            .await,
        Err(ListCommentReactionFailure::NotRequested(_))
    ));
    assert_eq!(calls, 2);
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_comment_reaction(local(), CHANNEL, TS, NAME, true, || true)
            .await,
        Err(ListCommentReactionFailure::NotRequested(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut data = Vec::new();
    loop {
        let mut buffer = [0u8; 2048];
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buffer[..n]);
        if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&data[..i]).to_ascii_lowercase();
            let len: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if data.len() >= i + 4 + len {
                return data;
            }
        }
    }
}

#[tokio::test]
async fn issued_raw_response_survives_retirement_without_retry_or_domain_settlement() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for adding in [true, false] {
        for raw in [
            json!({"ok":true,"opaque":{"keep":[2,1]}}),
            json!({"ok":false,"error":"already_reacted"}),
            json!({"ok":false}),
            json!({"other":"unclassified"}),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let current = Arc::new(AtomicBool::new(true));
            let owned = current.clone();
            let expected = raw.clone();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                let text = String::from_utf8(request).unwrap();
                assert!(text.starts_with(&format!("POST {} ", path(adding))));
                assert!(
                    text.contains("channel=C01234567")
                        && text.contains("timestamp=1700000001.000001")
                        && text.contains("name=wave")
                );
                owned.store(false, Ordering::SeqCst);
                let body = serde_json::to_vec(&raw).unwrap();
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                stream.write_all(&body).await.unwrap();
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(60), listener.accept())
                        .await
                        .is_err()
                );
            });
            let mut r = request(adding);
            r.url = format!("http://{addr}{}", path(adding));
            let result = Transport::new("fixture-cookie")
                .unwrap()
                .execute_list_comment_reaction(r, CHANNEL, TS, NAME, adding, || {
                    current.load(Ordering::SeqCst)
                })
                .await
                .unwrap();
            assert_eq!(result, expected);
            assert!(!current.load(Ordering::SeqCst));
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn dispatched_invalid_json_dropped_body_redirect_and_rate_limit_are_unconfirmed_once() {
    for (status, headers, body) in [
        ("200 OK", "", Some("not-json")),
        ("200 OK", "", Some("{\"ok\":true,\"ok\":false}")),
        ("200 OK", "", Some("[]")),
        ("200 OK", "", None),
        ("302 Found", "Location: /api/reactions.add\r\n", Some("{}")),
        ("429 Too Many Requests", "Retry-After: 3\r\n", Some("{}")),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await;
            if let Some(body) = body {
                stream.write_all(format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(60), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut r = request(true);
        r.url = format!("http://{addr}/api/reactions.add");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_comment_reaction(r, CHANNEL, TS, NAME, true, || true)
            .await;
        if status.starts_with("429") {
            assert!(matches!(
                result,
                Err(ListCommentReactionFailure::Unconfirmed(
                    Error::RateLimited {
                        retry_after_secs: Some(3)
                    }
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ListCommentReactionFailure::Unconfirmed(_))
            ));
        }
        server.await.unwrap();
    }
}
