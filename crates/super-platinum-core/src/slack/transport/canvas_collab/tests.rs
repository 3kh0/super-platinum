use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr, path: &str) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}{path}"),
        headers: Vec::new(),
        body: RequestBody::Form(vec![("token".into(), "fixture-only".into())]),
    }
}
#[tokio::test]
async fn offline_reads_are_never_queued_and_mutation_paths_are_rejected() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    for path in [
        "/api/quip.getControllerInitData",
        "/canvas/-/load-data",
        "/canvas/-/fetch-latest",
    ] {
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_canvas_collab_read(request(address, path)),
        )
        .await
        .expect("must not hold offline");
        assert!(matches!(result, Err(Error::Offline(_))));
    }
    transport.set_health(Health::Online);
    for path in [
        "/canvas/-/edit-document",
        "/canvas/-/load-data.extra",
        "/api/chat.postMessage",
    ] {
        assert!(matches!(
            transport
                .execute_canvas_collab_read(request(address, path))
                .await,
            Err(Error::Api(_))
        ));
    }
    let mut wrong_method = request(address, "/canvas/-/load-data");
    wrong_method.method = "GET";
    assert!(matches!(
        transport.execute_canvas_collab_read(wrong_method).await,
        Err(Error::Api(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn binary_read_is_single_attempt_bounded_and_does_not_follow_redirects() {
    for (status, length) in [
        (200, 4usize),
        (429, 4),
        (503, 4),
        (307, 4),
        (200, crate::canvas::wire::MAX_BYTES + 1),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let len = stream.read(&mut buffer).await.unwrap();
            assert!(
                String::from_utf8_lossy(&buffer[..len]).starts_with("POST /canvas/-/load-data")
            );
            let header = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {length}\r\nRetry-After: 7\r\nLocation: http://{address}/canvas/-/load-data\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&[0, 255, 128, 1]).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_canvas_collab_read(request(address, "/canvas/-/load-data"))
            .await;
        match (status, length) {
            (200, 4) => assert_eq!(result.unwrap(), [0, 255, 128, 1]),
            (200, _) => assert!(matches!(result, Err(Error::Transport(_)))),
            (429, _) => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            )),
            (503 | 307, _) => assert!(
                matches!(result, Err(Error::HttpStatus { status: actual, .. }) if actual == status)
            ),
            _ => unreachable!(),
        }
        server.await.unwrap();
    }
}
