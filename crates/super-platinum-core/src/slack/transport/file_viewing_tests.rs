use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr, path: &str) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}{path}"),
        headers: Vec::new(),
        body: RequestBody::Form(vec![("file_id".into(), "F_TEST".into())]),
    }
}

#[tokio::test]
async fn offline_file_viewing_returns_before_recovery_and_never_dispatches() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    for endpoint in ["/api/files.open", "/api/files.ping", "/api/files.close"] {
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_file_viewing(request(listener.local_addr().unwrap(), endpoint)),
        )
        .await
        .expect("viewing request must never hold for recovery");
        assert!(matches!(result, Err(Error::Offline(_))));
    }
    transport.set_health(Health::Online);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn file_viewing_rejects_other_endpoints_and_methods_before_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for path in [
        "/api/files.info",
        "/api/files.open.extra",
        "/api/files.open/",
        "/api/lists.open",
    ] {
        assert!(
            matches!(transport.execute_file_viewing(request(listener.local_addr().unwrap(),path)).await,Err(Error::Api(code)) if code=="unsupported_file_viewing_request")
        );
    }
    let mut req = request(listener.local_addr().unwrap(), "/api/files.open");
    req.method = "GET";
    assert!(matches!(
        transport.execute_file_viewing(req).await,
        Err(Error::Api(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn file_viewing_dispatches_once_and_preserves_rate_limit_or_success() {
    for (status, body) in [
        ("429 Too Many Requests", "{}"),
        ("503 Service Unavailable", "{}"),
        ("200 OK", r#"{"ok":true,"should_subscribe_and_ping":true}"#),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(String::from_utf8_lossy(&buffer[..count]).starts_with("POST /api/files.open"));
            socket.write_all(response.as_bytes()).await.unwrap();
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(150), listener.accept())
                    .await
                    .is_err(),
                "viewing operation was retried"
            );
        });
        let transport = Transport::new("fixture-cookie").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            transport.execute_file_viewing(request(address, "/api/files.open")),
        )
        .await
        .expect("viewing operation entered retry delay");
        match status {
            "429 Too Many Requests" => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            )),
            "503 Service Unavailable" => {
                assert!(matches!(result, Err(Error::HttpStatus { status: 503, .. })))
            }
            _ => assert_eq!(result.unwrap()["should_subscribe_and_ping"], true),
        }
        server.await.unwrap();
    }
}
