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
