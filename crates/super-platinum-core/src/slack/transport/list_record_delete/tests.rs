use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr, endpoint: &str) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/{endpoint}"),
        headers: vec![],
        body: RequestBody::Form(vec![("token".into(), "synthetic-token".into())]),
    }
}

#[tokio::test]
async fn invalid_and_offline_delete_never_connect_or_wait_for_recovery() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_list_record_delete(request(address, "lists.records.delete"))
        )
        .await
        .unwrap(),
        Err(ListDeleteFailure::NotSent(Error::Offline(_)))
    ));
    transport.set_health(Health::Online);
    for endpoint in [
        "lists.records.deleteMultiple",
        "lists.records.delete.extra",
        "chat.postMessage",
        "lists.records.info",
    ] {
        assert!(matches!(
            transport
                .execute_list_record_delete(request(address, endpoint))
                .await,
            Err(ListDeleteFailure::NotSent(_))
        ));
    }
    let mut req = request(address, "lists.records.delete");
    req.method = "GET";
    assert!(matches!(
        transport.execute_list_record_delete(req).await,
        Err(ListDeleteFailure::NotSent(_))
    ));
    let mut req = request(address, "lists.records.delete");
    req.headers.push(("invalid\nheader".into(), "value".into()));
    assert!(matches!(
        transport.execute_list_record_delete(req).await,
        Err(ListDeleteFailure::NotSent(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn delete_http_and_application_failures_are_uncertain_without_redirects_or_retries() {
    for (status, body) in [
        (200, r#"{"ok":true}"#),
        (200, r#"{"ok":false,"error":"record_deleted"}"#),
        (200, r#"{"ok":false,"error":"ratelimited"}"#),
        (200, "{}"),
        (200, "invalid"),
        (307, "{}"),
        (429, "{}"),
        (503, "{}"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            stream.read(&mut buffer).await.unwrap();
            stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nRetry-After: 9\r\nLocation: http://{address}/api/lists.records.delete\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("synthetic-cookie")
            .unwrap()
            .execute_list_record_delete(request(address, "lists.records.delete"))
            .await;
        if status == 200 && body == r#"{"ok":true}"# {
            assert_eq!(result.unwrap()["ok"], true);
        } else if status == 429 || body.contains("ratelimited") {
            assert!(matches!(
                result,
                Err(ListDeleteFailure::Uncertain(Error::RateLimited {
                    retry_after_secs: Some(9)
                }))
            ));
        } else {
            assert!(matches!(result, Err(ListDeleteFailure::Uncertain(_))));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn lost_delete_response_remains_uncertain_and_info_keeps_error_identity() {
    for info in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            stream.read(&mut buffer).await.unwrap();
            if info {
                let body = r#"{"ok":false,"error":"record_deleted","list_id":"F_OTHER"}"#;
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        if info {
            let raw = transport
                .execute_list_record_info(request(address, "lists.records.info"))
                .await
                .unwrap();
            assert_eq!(raw["error"], "record_deleted");
            assert_eq!(raw["list_id"], "F_OTHER");
            assert!(
                crate::slack_lists::ListRecordReadback::from_response("F_LIST", "R_ITEM", &raw)
                    .is_err()
            );
        } else {
            assert!(matches!(
                transport
                    .execute_list_record_delete(request(address, "lists.records.delete"))
                    .await,
                Err(ListDeleteFailure::Uncertain(Error::Transport(_)))
            ));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn deletion_state_changes_send_once_and_never_follow_redirect_or_replay() {
    for (status, body) in [
        (200, Some(r#"{"ok":true}"#)),
        (200, Some(r#"{"ok":false,"error":"ratelimited"}"#)),
        (200, Some(r#"{"ok":false,"error":"permission_denied"}"#)),
        (200, Some("invalid")),
        (307, Some("{}")),
        (429, Some("{}")),
        (503, Some("{}")),
        (200, None),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(
                std::str::from_utf8(&buffer[..n])
                    .unwrap()
                    .starts_with("POST /api/lists.records.updateDeletion ")
            );
            if let Some(body) = body {
                stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nRetry-After: 9\r\nLocation: http://{address}/api/lists.records.updateDeletion\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("synthetic-cookie")
            .unwrap()
            .execute_list_record_update_deletion(request(address, "lists.records.updateDeletion"))
            .await;
        if status == 200 && body == Some(r#"{"ok":true}"#) {
            assert_eq!(result.unwrap()["ok"], true);
        } else if status == 429 || body.is_some_and(|b| b.contains("ratelimited")) {
            assert!(matches!(
                result,
                Err(ListDeleteFailure::Uncertain(Error::RateLimited {
                    retry_after_secs: Some(9)
                }))
            ));
        } else {
            assert!(matches!(result, Err(ListDeleteFailure::Uncertain(_))));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn deletion_state_rejects_other_paths_and_offline_without_connecting() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    for path in [
        "lists.records.delete",
        "lists.records.create",
        "lists.records.info",
        "lists.records.updateDeletion.extra",
    ] {
        assert!(matches!(
            transport
                .execute_list_record_update_deletion(request(address, path))
                .await,
            Err(ListDeleteFailure::NotSent(_))
        ));
    }
    transport.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_list_record_update_deletion(request(
                address,
                "lists.records.updateDeletion"
            ))
        )
        .await
        .unwrap(),
        Err(ListDeleteFailure::NotSent(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
