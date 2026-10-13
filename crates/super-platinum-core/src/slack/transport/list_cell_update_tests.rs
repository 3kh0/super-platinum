use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr, path: &str) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}{path}"),
        headers: Vec::new(),
        body: RequestBody::Form(Vec::new()),
    }
}
#[tokio::test]
async fn cell_update_rejects_offline_or_unrelated_requests_without_later_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    for path in ["/api/lists.cells.update"] {
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_list_cell_update(request(address, path)),
        )
        .await
        .expect("cell update must not hold offline");
        assert!(matches!(result, Err(Error::Offline(_))));
    }
    transport.set_health(Health::Online);
    for path in [
        "/api/files.open",
        "/api/lists.cells.update.extra",
        "/api/lists.cells.update/",
        "/api/lists.open",
    ] {
        assert!(matches!(
            transport
                .execute_list_cell_update(request(address, path))
                .await,
            Err(Error::Api(_))
        ));
    }
    let mut get = request(address, "/api/lists.cells.update");
    get.method = "GET";
    assert!(matches!(
        transport.execute_list_cell_update(get).await,
        Err(Error::Api(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn cell_update_dispatch_is_single_attempt_even_for_retryable_http_errors() {
    for path in ["/api/lists.cells.update"] {
        for status in ["200 OK", "429 Too Many Requests", "503 Service Unavailable"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: 11\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{{\"ok\":true}}"
            );
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                let len = stream.read(&mut buffer).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&buffer[..len]).starts_with(&format!("POST {path}"))
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            });
            let transport = Transport::new("fixture-cookie").unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                transport.execute_list_cell_update(request(address, path)),
            )
            .await
            .expect("cell update must not retry");
            match status {
                "200 OK" => assert_eq!(result.unwrap()["ok"], true),
                "429 Too Many Requests" => assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(7)
                    })
                )),
                _ => assert!(matches!(result, Err(Error::HttpStatus { status: 503, .. }))),
            }
            server.await.unwrap();
        }
    }
}
