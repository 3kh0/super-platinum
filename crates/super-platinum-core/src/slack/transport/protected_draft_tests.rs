use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr, path: &str) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}{path}?fixture=1"),
        headers: vec![],
        body: RequestBody::Form(vec![]),
    }
}
async fn accept(listener: &tokio::net::TcpListener) -> tokio::net::TcpStream {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut bytes = [0; 8192];
    let count = stream.read(&mut bytes).await.unwrap();
    assert!(count > 0);
    stream
}
async fn response(stream: &mut tokio::net::TcpStream, status: &str, body: &[u8], extra: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    let _ = stream.write_all(body).await;
}
async fn no_second(listener: &tokio::net::TcpListener) {
    assert!(
        tokio::time::timeout(Duration::from_millis(120), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn bytes_are_bounded_before_decoding_at_exact_limit_and_across_chunks() {
    let mut body = Body::default();
    body.push(b"{\"ok\":true}").unwrap();
    body.push(&vec![
        b' ';
        MAX_PROTECTED_DRAFT_RESPONSE_BYTES - body.0.len()
    ])
    .unwrap();
    assert_eq!(body.0.len(), MAX_PROTECTED_DRAFT_RESPONSE_BYTES);
    assert!(body.push(b" ").is_err());
    assert_eq!(body.0.len(), MAX_PROTECTED_DRAFT_RESPONSE_BYTES);
    assert_eq!(body.finish().unwrap(), serde_json::json!({"ok":true}));
    let mut huge = Body::default();
    assert!(
        huge.push(&vec![0; MAX_PROTECTED_DRAFT_RESPONSE_BYTES + 1])
            .is_err()
    );
    assert!(huge.0.is_empty());
    for malformed in [b"{\"ok\":true".as_slice(), b"[]", b"null", &[0xff]] {
        let mut body = Body::default();
        body.push(malformed).unwrap();
        assert!(body.finish().is_err());
    }
}

#[tokio::test]
async fn exact_wire_limit_is_accepted_and_announced_oversize_is_unknown_without_replay() {
    for size in [
        MAX_PROTECTED_DRAFT_RESPONSE_BYTES,
        MAX_PROTECTED_DRAFT_RESPONSE_BYTES + 1,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept(&listener).await;
            let mut body = b"{\"ok\":true}".to_vec();
            body.resize(size, b' ');
            response(&mut stream, "200 OK", &body, "").await;
            drop(stream);
            no_second(&listener).await;
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        let result = transport
            .execute_protected_draft_mutation(request(addr, "/api/drafts.update"), || true)
            .await;
        if size == MAX_PROTECTED_DRAFT_RESPONSE_BYTES {
            assert_eq!(result.unwrap()["ok"], true);
        } else {
            assert!(matches!(
                result,
                Err(ProtectedDraftFailure::Response(Error::Transport(_)))
            ));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn chunked_unknown_length_cannot_bypass_limit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut payload = b"{\"ok\":true}".to_vec();
        payload.resize(MAX_PROTECTED_DRAFT_RESPONSE_BYTES + 1, b' ');
        let head = format!("{:X}\r\n", payload.len());
        stream.write_all(head.as_bytes()).await.unwrap();
        let _ = stream.write_all(&payload).await;
        let _ = stream.write_all(b"\r\n0\r\n\r\n").await;
        drop(stream);
        no_second(&listener).await;
    });
    let transport = Transport::new("synthetic-cookie").unwrap();
    assert!(matches!(
        transport
            .execute_protected_draft_read(request(addr, "/api/drafts.info"), || true)
            .await,
        Err(ProtectedDraftFailure::Response(Error::Transport(_)))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_and_incomplete_mutation_responses_remain_unknown() {
    for complete in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept(&listener).await;
            if complete {
                response(&mut stream, "200 OK", b"{not json}", "").await;
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
            }
            drop(stream);
            no_second(&listener).await;
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        assert!(matches!(
            transport
                .execute_protected_draft_mutation(request(addr, "/api/chat.postMessage"), || true)
                .await,
            Err(ProtectedDraftFailure::Response(Error::Transport(_)))
        ));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn rate_limit_and_redirect_are_not_replayed_and_keep_headers() {
    for (status, path) in [
        ("429 Too Many Requests", "/api/conversations.history"),
        ("302 Found", "/api/drafts.update"),
        ("503 Unavailable", "/api/conversations.history"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept(&listener).await;
            response(
                &mut stream,
                status,
                b"",
                "Retry-After: 9\r\nLocation: https://example.invalid/\r\n",
            )
            .await;
            drop(stream);
            no_second(&listener).await;
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        let error = if path.contains("history") {
            transport
                .execute_protected_draft_read(request(addr, path), || true)
                .await
                .unwrap_err()
        } else {
            transport
                .execute_protected_draft_mutation(request(addr, path), || true)
                .await
                .unwrap_err()
        };
        assert!(error.requested());
        assert!(matches!(
            error.error(),
            Error::RateLimited {
                retry_after_secs: Some(9)
            } | Error::HttpStatus {
                retry_after_secs: Some(9),
                ..
            }
        ));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn owner_retired_during_offline_hold_never_dispatches() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    transport.set_health(Health::Offline);
    let live = Arc::new(AtomicBool::new(true));
    let retired = live.clone();
    let restored = transport.clone();
    let change = async move {
        tokio::task::yield_now().await;
        retired.store(false, Ordering::Release);
        restored.set_health(Health::Online);
    };
    let read = transport
        .execute_protected_draft_read(request(addr, "/api/conversations.history"), || {
            live.load(Ordering::Acquire)
        });
    let (result, ()) = tokio::join!(read, change);
    assert!(matches!(
        result,
        Err(ProtectedDraftFailure::NotRequested(_))
    ));
    no_second(&listener).await;
}

#[tokio::test]
async fn owner_retirement_after_first_read_cannot_become_not_requested_or_retry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let live = Arc::new(AtomicBool::new(true));
    let retired = live.clone();
    let server = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        retired.store(false, Ordering::Release);
        response(&mut stream, "503 Unavailable", b"", "").await;
        drop(stream);
        tokio::time::sleep(Duration::from_millis(300)).await;
        no_second(&listener).await;
    });
    let transport = Transport::new("synthetic-cookie").unwrap();
    let result = transport
        .execute_protected_draft_read(request(addr, "/api/conversations.history"), || {
            live.load(Ordering::Acquire)
        })
        .await;
    assert!(matches!(result, Err(ProtectedDraftFailure::Response(_))));
    server.await.unwrap();
}

#[tokio::test]
async fn complete_original_response_survives_owner_retirement_for_receipt_only() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let live = Arc::new(AtomicBool::new(true));
    let retired = live.clone();
    let server = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        retired.store(false, Ordering::Release);
        response(&mut stream, "200 OK", b"{\"ok\":true}", "").await;
    });
    let transport = Transport::new("synthetic-cookie").unwrap();
    let raw = transport
        .execute_protected_draft_mutation(request(addr, "/api/drafts.update"), || {
            live.load(Ordering::Acquire)
        })
        .await
        .unwrap();
    assert_eq!(raw["ok"], true);
    assert!(!live.load(Ordering::Acquire));
    server.await.unwrap();
}

#[tokio::test]
async fn endpoint_and_offline_preflight_never_enter_http() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    assert!(matches!(
        transport
            .execute_protected_draft_read(request(addr, "/api/chat.postMessage"), || true)
            .await,
        Err(ProtectedDraftFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_protected_draft_mutation(request(addr, "/api/files.delete"), || true)
            .await,
        Err(ProtectedDraftFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_protected_draft_mutation(request(addr, "/api/drafts.update"), || true)
            .await,
        Err(ProtectedDraftFailure::NotRequested(Error::Offline(_)))
    ));
    no_second(&listener).await;
}

#[tokio::test]
async fn safe_history_retry_remains_bounded_and_returns_only_complete_json() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut first = accept(&listener).await;
        response(&mut first, "503 Unavailable", b"", "").await;
        drop(first);
        let mut second = accept(&listener).await;
        response(&mut second, "200 OK", b"{\"ok\":true,\"messages\":[]}", "").await;
        drop(second);
        no_second(&listener).await;
    });
    let transport = Transport::new("synthetic-cookie").unwrap();
    let mut history = request(addr, "/api/conversations.history");
    history.url = format!("http://{addr}/api/conversations.history");
    let raw = transport
        .execute_protected_draft_read(history, || true)
        .await
        .unwrap();
    assert_eq!(raw["messages"], serde_json::json!([]));
    server.await.unwrap();
}

#[tokio::test]
async fn duplicate_receipt_members_at_any_depth_fail_without_mutation_replay() {
    for raw in [
        r#"{"ok":false,"ok":true}"#,
        r#"{"ok":true,"error":"invalid_draft","error":null}"#,
        r#"{"ok":true,"draft":{"id":"OTHER","id":"D1"}}"#,
        r#"{"ok":true,"draft":{"is_deleted":true,"is_deleted":false}}"#,
        r#"{"ok":true,"messages":[{"client_msg_id":"OTHER","client_msg_id":"expected"}]}"#,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept(&listener).await;
            response(&mut stream, "200 OK", raw.as_bytes(), "").await;
            drop(stream);
            no_second(&listener).await;
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        assert!(matches!(
            transport
                .execute_protected_draft_mutation(request(addr, "/api/drafts.update"), || true)
                .await,
            Err(ProtectedDraftFailure::Response(Error::Transport(_)))
        ));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn info_query_cannot_borrow_history_retry_policy() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut stream = accept(&listener).await;
        response(&mut stream, "503 Unavailable", b"", "").await;
        drop(stream);
        // Covers both automatic 250ms and 500ms retry slots.
        assert!(
            tokio::time::timeout(Duration::from_millis(850), listener.accept())
                .await
                .is_err()
        );
    });
    let mut info = request(addr, "/api/drafts.info");
    info.url = format!("http://{addr}/api/drafts.info?x=/api/conversations.history?");
    assert!(
        info.retry_safe(),
        "regression fixture must exercise the legacy substring ambiguity"
    );
    let transport = Transport::new("synthetic-cookie").unwrap();
    assert!(matches!(
        transport.execute_protected_draft_read(info, || true).await,
        Err(ProtectedDraftFailure::Response(Error::HttpStatus {
            status: 503,
            ..
        }))
    ));
    server.await.unwrap();
}
