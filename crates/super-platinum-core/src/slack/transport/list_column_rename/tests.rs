use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST".into(),
        url: format!("http://{address}/api/lists.columns.update"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("key".into(), "title".into()),
            ("name".into(), "New title".into()),
            ("options".into(), "{\"format\":\"text\"}".into()),
            ("backfill_missing_cells_with_default".into(), "true".into()),
            ("_x_reason".into(), "update-column".into()),
        ]),
    }
}
#[tokio::test]
async fn retired_offline_and_wrong_request_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let mut bad = Vec::new();
    for path in [
        "/api/lists.cells.update",
        "/api/lists.columns.update/",
        "/api/lists.columns.update?cursor=guessed",
    ] {
        let mut req = request(address);
        req.url = format!("http://{address}{path}");
        bad.push(req);
    }
    let mut extra = request(address);
    if let RequestBody::Form(fields) = &mut extra.body {
        fields.push(("type".into(), "number".into()));
    }
    bad.push(extra);
    let mut get = request(address);
    get.method = "GET";
    bad.push(get);
    let mut foreign = request(address);
    foreign.url = "https://foreign.example/api/lists.columns.update".into();
    bad.push(foreign);
    for req in bad {
        assert!(matches!(
            transport.execute_list_column_rename(req, || true).await,
            Err(ListColumnRenameFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_column_rename(request(address), || false)
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    let mut calls = 0;
    assert!(matches!(
        transport
            .execute_list_column_rename(request(address), || {
                calls += 1;
                calls == 1
            })
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_column_rename(request(address), || true)
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn http_denial_or_retry_after_never_replays_and_retains_known_limit() {
    for status in [200, 429, 503] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            stream.read(&mut bytes).await.unwrap();
            stream.write_all(format!("HTTP/1.1 {status} Result\r\nContent-Length: 11\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{{\"ok\":true}}").as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_column_rename(request(address), || true)
            .await;
        match result {
            Err(ListColumnRenameFailure::Unconfirmed(Error::RateLimited {
                retry_after_secs: Some(7),
            })) if status == 429 => {}
            Err(ListColumnRenameFailure::Unconfirmed(Error::HttpStatus {
                status: actual,
                retry_after_secs: Some(7),
            })) if actual == status => {}
            other => panic!("unexpected {other:?}"),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn duplicate_json_and_lost_response_remain_unconfirmed() {
    for body in [Some("{\"ok\":false,\"ok\":true}"), None] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            stream.read(&mut bytes).await.unwrap();
            if let Some(body) = body {
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_column_rename(request(address), || true)
            .await;
        assert!(matches!(
            result,
            Err(ListColumnRenameFailure::Unconfirmed(_))
        ));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn original_response_is_retained_after_current_owner_retires() {
    use std::sync::atomic::AtomicBool;
    let live = Arc::new(AtomicBool::new(true));
    let retired = live.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        stream.read(&mut bytes).await.unwrap();
        retired.store(false, Ordering::SeqCst);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .await
            .unwrap();
    });
    let result = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_column_rename(request(address), || live.load(Ordering::SeqCst))
        .await
        .unwrap();
    assert_eq!(result, serde_json::json!({"ok":true}));
    assert!(!live.load(Ordering::SeqCst));
    server.await.unwrap();
}
