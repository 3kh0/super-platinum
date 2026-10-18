use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn actual_120_second_429_blocks_new_queries_and_info_without_a_second_request() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    let gate = FileShareReadGate::default();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).await.unwrap();
        assert!(String::from_utf8_lossy(&bytes[..n]).starts_with("POST /api/search.modules.files"));
        stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(250), listener.accept())
                .await
                .is_err()
        );
    });
    for path in [
        "search.modules.files",
        "search.modules.files",
        "files.info",
        "conversations.info",
        "im.list",
    ] {
        let request = PreparedRequest {
            method: "POST",
            url: format!("http://{address}/api/{path}"),
            headers: vec![],
            body: RequestBody::Form(vec![("token".into(), "synthetic".into())]),
        };
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            transport.clone().execute_file_share_read(request, &gate),
        )
        .await
        .unwrap();
        assert!(
            matches!(result,Err(Error::RateLimited{retry_after_secs:Some(seconds)}) if seconds>=119)
        );
        assert!(!gate.expired());
    }
    server.await.unwrap();
}

#[tokio::test]
async fn overlapping_read_and_extreme_deadline_never_dispatch_another_callback() {
    let gate = FileShareReadGate::default();
    let held = gate.until_ms.lock().await;
    assert!(
        gate.execute(|| async { panic!("overlap must not run") })
            .await
            .is_err()
    );
    drop(held);
    let result = gate
        .execute(|| async {
            Err(Error::RateLimited {
                retry_after_secs: Some(u64::MAX),
            })
        })
        .await;
    assert!(matches!(result, Err(Error::RateLimited { .. })));
    assert!(!gate.expired());
    assert!(matches!(
        gate.execute(|| async { panic!("deadline must not run") })
            .await,
        Err(Error::RateLimited { .. })
    ));
}

#[tokio::test]
async fn read_gate_rejects_write_and_foreign_origin_before_transport() {
    let gate = FileShareReadGate::default();
    let transport = Transport::disconnected_fixture("synthetic");
    for url in [
        "https://fixture.slack.com/api/files.share",
        "https://example.com/api/files.info",
        "https://fixture.slack.com/api/im.open",
    ] {
        let request = PreparedRequest {
            method: "POST",
            url: url.into(),
            headers: vec![],
            body: RequestBody::Form(vec![]),
        };
        assert!(matches!(
            transport.execute_file_share_read(request, &gate).await,
            Err(Error::Api(_))
        ));
    }
}

#[tokio::test]
async fn successful_http_rate_limit_and_http_error_preserve_their_full_retry_deadline() {
    for (status, header, body, minimum) in [
        (
            "200 OK",
            "Retry-After: 120\r\n",
            r#"{"ok":false,"error":"ratelimited"}"#,
            119,
        ),
        ("200 OK", "", r#"{"ok":false,"error":"ratelimited"}"#, 29),
        ("503 Service Unavailable", "Retry-After: 120\r\n", "", 119),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            let response = format!(
                "HTTP/1.1 {status}\r\n{header}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        let gate = FileShareReadGate::default();
        let request = PreparedRequest {
            method: "POST",
            url: format!("http://{address}/api/files.info"),
            headers: vec![],
            body: RequestBody::Form(vec![]),
        };
        let first = transport
            .execute_file_share_read(request.clone(), &gate)
            .await;
        if status.starts_with("503") {
            assert!(matches!(
                first,
                Err(Error::HttpStatus {
                    status: 503,
                    retry_after_secs: Some(120)
                })
            ));
        } else {
            assert!(matches!(first, Err(Error::RateLimited { .. })));
        }
        let again = transport
            .clone()
            .execute_file_share_read(request, &gate)
            .await;
        assert!(matches!(again,Err(Error::RateLimited{retry_after_secs:Some(n)}) if n>=minimum));
        assert!(!gate.expired());
        server.await.unwrap();
    }
}

#[tokio::test]
async fn ordinary_execute_keeps_its_existing_api_error_without_rate_retry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        let body = r#"{"ok":false,"error":"ratelimited"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nRetry-After: 120\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let transport = Transport::new("synthetic-cookie").unwrap();
    let request = PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/files.info"),
        headers: vec![],
        body: RequestBody::Form(vec![]),
    };
    let result = tokio::time::timeout(Duration::from_secs(2), transport.execute(request))
        .await
        .unwrap();
    assert!(matches!(result,Err(Error::Api(code)) if code=="ratelimited"));
    server.await.unwrap();
}
