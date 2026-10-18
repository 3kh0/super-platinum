use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/canvas/-/edit-document"),
        headers: vec![],
        body: RequestBody::Form(vec![("token".into(), "synthetic-token".into())]),
    }
}

#[tokio::test]
async fn invalid_and_offline_attempts_are_not_sent_or_queued() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_canvas_edit(request(address))
        )
        .await
        .unwrap(),
        Err(CanvasEditFailure::NotSent(Error::Offline(_)))
    ));
    transport.set_health(Health::Online);
    for path in [
        "/canvas/-/load-data",
        "/canvas/-/edit-document.extra",
        "/api/chat.postMessage",
    ] {
        let mut req = request(address);
        req.url = format!("http://{address}{path}");
        assert!(matches!(
            transport.execute_canvas_edit(req).await,
            Err(CanvasEditFailure::NotSent(_))
        ));
    }
    let mut req = request(address);
    req.method = "GET";
    assert!(matches!(
        transport.execute_canvas_edit(req).await,
        Err(CanvasEditFailure::NotSent(_))
    ));
    let mut req = request(address);
    req.body = RequestBody::Form(vec![("data_binary".into(), "x".repeat(512 * 1024))]);
    assert!(matches!(
        transport.execute_canvas_edit(req).await,
        Err(CanvasEditFailure::NotSent(_))
    ));
    let mut req = request(address);
    req.headers.push(("invalid\nheader".into(), "value".into()));
    assert!(matches!(
        transport.execute_canvas_edit(req).await,
        Err(CanvasEditFailure::NotSent(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn responses_are_single_attempt_without_redirects_and_failures_stay_uncertain() {
    for status in [200, 307, 400, 403, 409, 429, 503] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let length = stream.read(&mut buffer).await.unwrap();
            assert!(
                String::from_utf8_lossy(&buffer[..length])
                    .starts_with("POST /canvas/-/edit-document")
            );
            stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: 4\r\nRetry-After: 9\r\nLocation: http://{address}/canvas/-/edit-document\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            stream.write_all(&[0, 255, 128, 1]).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("synthetic-cookie")
            .unwrap()
            .execute_canvas_edit(request(address))
            .await;
        match status {
            200 => assert_eq!(result.unwrap(), [0, 255, 128, 1]),
            429 => assert!(matches!(
                result,
                Err(CanvasEditFailure::Uncertain(Error::RateLimited {
                    retry_after_secs: Some(9)
                }))
            )),
            _ => assert!(
                matches!(result, Err(CanvasEditFailure::Uncertain(Error::HttpStatus{status: actual,..})) if actual == status)
            ),
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn oversized_or_truncated_acknowledgements_cannot_unlock_a_sent_operation() {
    for advertised in [16, crate::canvas::wire::MAX_BYTES + 1] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            stream.read(&mut buffer).await.unwrap();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {advertised}\r\nConnection: close\r\n\r\nshort").as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        assert!(matches!(
            transport.execute_canvas_edit(request(address)).await,
            Err(CanvasEditFailure::Uncertain(_))
        ));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn comment_archive_boundary_is_narrow_and_offline_never_dispatches() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    let mut req = request(address);
    req.url = format!("http://{address}/api/quip.thread.archive");
    req.body = RequestBody::Form(vec![
        ("token".into(), "synthetic".into()),
        ("channel_id".into(), "C123".into()),
        ("thread_ts".into(), "1700000000.000100".into()),
    ]);
    assert!(comment_allowed(&req));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport.execute_canvas_comment_archive(req.clone()).await,
        Err(CanvasEditFailure::NotSent(Error::Offline(_)))
    ));
    for path in [
        "/api/quip.thread.archiveAll",
        "/api/chat.postMessage",
        "/canvas/-/edit-document",
    ] {
        let mut invalid = req.clone();
        invalid.url = format!("http://{address}{path}");
        assert!(!comment_allowed(&invalid));
    }
    if let RequestBody::Form(fields) = &mut req.body {
        fields.push(("thread_ts".into(), "1700000000.000200".into()));
    }
    assert!(!comment_allowed(&req));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn lost_comment_response_is_one_attempt_and_never_known_unsent() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0u8; 4096];
        let count = socket.read(&mut buffer).await.unwrap();
        assert!(count > 0);
        socket.shutdown().await.unwrap();
        drop(socket);
        assert!(
            tokio::time::timeout(Duration::from_millis(250), listener.accept())
                .await
                .is_err()
        );
    });
    let transport = Transport::new("synthetic-cookie").unwrap();
    let mut req = request(address);
    req.url = format!("http://{address}/api/quip.thread.unarchive");
    req.body = RequestBody::Form(vec![
        ("token".into(), "synthetic".into()),
        ("channel_id".into(), "C123".into()),
        ("thread_ts".into(), "1700000000.000100".into()),
    ]);
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_canvas_comment_archive(req),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_))));
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

fn reply_request(address: std::net::SocketAddr) -> PreparedRequest {
    let mut r = request(address);
    r.url = format!("http://{address}/api/chat.postMessage");
    r.body = RequestBody::Form(vec![
        ("token".into(), "synthetic".into()),
        ("channel".into(), "C123".into()),
        ("thread_ts".into(), "1700000000.000100".into()),
        ("text".into(), "reply".into()),
        ("client_msg_id".into(), uuid::Uuid::new_v4().to_string()),
        ("reply_broadcast".into(), "false".into()),
    ]);
    r
}
#[tokio::test]
async fn reply_boundary_refuses_extra_fields_and_known_offline_without_queue() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let request = reply_request(addr);
    assert!(reply_allowed(&request));
    for key in ["thread_ts", "blocks", "attachments", "metadata", "channel"] {
        let mut invalid = request.clone();
        let RequestBody::Form(fields) = &mut invalid.body else {
            unreachable!()
        };
        fields.push((key.into(), "injected".into()));
        assert!(!reply_allowed(&invalid));
    }
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_canvas_comment_reply(request)
        )
        .await
        .unwrap(),
        Err(CanvasEditFailure::NotSent(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn reply_lost_response_is_exactly_one_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = [0; 4096];
        let n = socket.read(&mut bytes).await.unwrap();
        assert!(String::from_utf8_lossy(&bytes[..n]).starts_with("POST /api/chat.postMessage"));
        socket.shutdown().await.unwrap();
        drop(socket);
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    });
    let transport = Transport::new("synthetic").unwrap();
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_secs(3),
            transport.execute_canvas_comment_reply(reply_request(addr))
        )
        .await
        .unwrap(),
        Err(CanvasEditFailure::Uncertain(_))
    ));
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}
