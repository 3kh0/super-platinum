use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST".into(),
        url: format!("http://{address}/api/lists.columns.delete"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("key".into(), "title".into()),
            ("_x_reason".into(), "delete-column".into()),
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
        "/api/lists.columns.delete/",
        "/api/lists.columns.delete?cursor=guessed",
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
    foreign.url = "https://foreign.example/api/lists.columns.delete".into();
    bad.push(foreign);
    for req in bad {
        assert!(matches!(
            transport.execute_list_column_delete(req, || true).await,
            Err(ListColumnDeleteFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_column_delete(request(address), || false)
            .await,
        Err(ListColumnDeleteFailure::NotRequested(_))
    ));
    let mut calls = 0;
    assert!(matches!(
        transport
            .execute_list_column_delete(request(address), || {
                calls += 1;
                calls == 1
            })
            .await,
        Err(ListColumnDeleteFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_column_delete(request(address), || true)
            .await,
        Err(ListColumnDeleteFailure::NotRequested(_))
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
            .execute_list_column_delete(request(address), || true)
            .await;
        match result {
            Err(ListColumnDeleteFailure::Unconfirmed(Error::RateLimited {
                retry_after_secs: Some(7),
            })) if status == 429 => {}
            Err(ListColumnDeleteFailure::Unconfirmed(Error::HttpStatus {
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
            .execute_list_column_delete(request(address), || true)
            .await;
        assert!(matches!(
            result,
            Err(ListColumnDeleteFailure::Unconfirmed(_))
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
        .execute_list_column_delete(request(address), || live.load(Ordering::SeqCst))
        .await
        .unwrap();
    assert_eq!(result, serde_json::json!({"ok":true}));
    assert!(!live.load(Ordering::SeqCst));
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_forms_and_foreign_origins_fail_before_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let mut malformed = Vec::new();
    for index in 0..4 {
        for replacement in ["", " ", "wrong", &"x".repeat(16 * 1024)] {
            if replacement == "wrong" && index < 3 {
                continue;
            }
            if index == 0 && replacement == " " {
                continue;
            }
            let mut request = request(address);
            let RequestBody::Form(fields) = &mut request.body else {
                unreachable!()
            };
            fields[index].1 = replacement.into();
            malformed.push(request);
        }
        let mut missing = request(address);
        let RequestBody::Form(fields) = &mut missing.body else {
            unreachable!()
        };
        fields.remove(index);
        malformed.push(missing);
    }
    for extra in [
        ("key", "other"),
        ("id", "C1"),
        ("deleted", "true"),
        ("options", "{}"),
        ("cells", "[]"),
    ] {
        let mut request = request(address);
        let RequestBody::Form(fields) = &mut request.body else {
            unreachable!()
        };
        fields.push((extra.0.into(), extra.1.into()));
        malformed.push(request);
    }
    let mut swapped = request(address);
    let RequestBody::Form(fields) = &mut swapped.body else {
        unreachable!()
    };
    fields.swap(1, 2);
    malformed.push(swapped);
    for url in [
        "https://example.slack.com.evil.test/api/lists.columns.delete",
        "http://example.slack.com/api/lists.columns.delete",
        "https://user@example.slack.com/api/lists.columns.delete",
        "https://example.slack.com:444/api/lists.columns.delete",
        "https://example.slack.com/api/lists.columns.delete#fragment",
        "https://example.slack.com/api/lists.columns.delete?token=foreign",
    ] {
        let mut request = request(address);
        request.url = url.into();
        malformed.push(request);
    }
    for request in malformed {
        assert!(matches!(
            transport.execute_list_column_delete(request, || true).await,
            Err(ListColumnDeleteFailure::NotRequested(_))
        ));
    }
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
async fn wire_is_key_only_and_ack_denial_or_redirect_never_causes_replay() {
    for (status, body) in [
        (200, "{\"ok\":true}"),
        (
            200,
            "{\"ok\":false,\"error\":\"cant_delete_primary_column\"}",
        ),
        (302, "{\"ok\":true}"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some((header, form)) = text.split_once("\r\n\r\n") {
                    let content_length: usize = header
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if form.len() < content_length {
                        continue;
                    }
                    assert!(header.starts_with("POST /api/lists.columns.delete HTTP/1.1"));
                    let actual: Vec<_> = url::form_urlencoded::parse(form.as_bytes())
                        .into_owned()
                        .collect();
                    assert_eq!(
                        actual,
                        vec![
                            ("token".into(), "fixture-token".into()),
                            ("list_id".into(), "F1".into()),
                            ("key".into(), "title".into()),
                            ("_x_reason".into(), "delete-column".into())
                        ]
                    );
                    break;
                }
            }
            stream.write_all(format!("HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nLocation: http://{address}/api/lists.columns.delete\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_column_delete(request(address), || true)
            .await;
        if status == 200 {
            assert_eq!(
                result.unwrap(),
                serde_json::from_str::<Value>(body).unwrap()
            );
        } else {
            assert!(matches!(
                result,
                Err(ListColumnDeleteFailure::Unconfirmed(Error::HttpStatus {
                    status: 302,
                    ..
                }))
            ));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn delete_policy_reader_accepts_only_exact_authenticated_experiments_form() {
    fn read_request(address: std::net::SocketAddr) -> PreparedRequest {
        PreparedRequest {
            method: "POST",
            url: format!("http://{address}/api/experiments.getByUser"),
            headers: vec![],
            body: RequestBody::Form(vec![
                ("token".into(), "fixture-token".into()),
                ("ssb_platform".into(), "windows".into()),
                ("ssb_release_channel".into(), "prod".into()),
            ]),
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        transport
            .execute_list_column_delete_read(request(address), || true)
            .await,
        Err(ListColumnDeleteFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_column_delete_read(read_request(address), || false)
            .await,
        Err(ListColumnDeleteFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_column_rename_read(read_request(address), || true)
            .await,
        Err(ListColumnRenameReadFailure::NotRequested(_))
    ));
    for (key, value) in [
        ("ssb_platform", "guessed"),
        ("ssb_release_channel", "dev"),
        ("token", ""),
        ("_x_reason", "delete-column"),
    ] {
        let mut req = read_request(address);
        let RequestBody::Form(fields) = &mut req.body else {
            unreachable!()
        };
        if let Some(pair) = fields.iter_mut().find(|(k, _)| k == key) {
            pair.1 = value.into();
        } else {
            fields.push((key.into(), value.into()));
        }
        assert!(matches!(
            transport
                .execute_list_column_delete_read(req, || true)
                .await,
            Err(ListColumnDeleteFailure::NotRequested(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let count = stream.read(&mut bytes).await.unwrap();
        assert!(
            String::from_utf8_lossy(&bytes[..count])
                .starts_with("POST /api/experiments.getByUser HTTP/1.1")
        );
        let body = r#"{"ok":true,"assignments":{"lists_schema_owner_only":{"group":"on"}}}"#;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    });
    let raw = transport
        .execute_list_column_delete_read(read_request(address), || true)
        .await
        .unwrap();
    assert!(
        crate::slack::api::parse_list_experiments(&raw)
            .unwrap()
            .schema_owner_only
    );
    server.await.unwrap();
}
