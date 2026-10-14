use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr, undo: bool) -> PreparedRequest {
    let mut fields = vec![
        ("token".into(), "synthetic".into()),
        (
            "_x_reason".into(),
            if undo {
                "UndoShiftEscapeMarkAllRead"
            } else {
                "ShiftEscapeConfirmDialog"
            }
            .into(),
        ),
    ];
    if undo {
        fields.push(("undo_ts".into(), "1791884254.000001".into()));
    }
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/users.markAllRead"),
        headers: vec![],
        body: RequestBody::Form(fields),
    }
}

#[test]
fn transport_rejects_scope_expansion_duplicates_and_malformed_undo() {
    let address = "127.0.0.1:1".parse().unwrap();
    assert!(allowed(&request(address, false)));
    assert!(allowed(&request(address, true)));
    for (key, value) in [
        ("team_id", "T_OTHER"),
        ("channels", "C1"),
        ("token", "another"),
        ("undo_ts", "NaN"),
    ] {
        let mut bad = request(address, false);
        let RequestBody::Form(ref mut fields) = bad.body else {
            unreachable!()
        };
        fields.push((key.into(), value.into()));
        assert!(!allowed(&bad));
    }
    let mut bad = request(address, true);
    bad.url.push_str("?team_id=T_OTHER");
    assert!(!allowed(&bad));
}

#[tokio::test]
async fn offline_shortcut_never_dispatches_or_waits_for_link_recovery() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture").unwrap();
    transport.set_health(Health::Offline);
    for undo in [false, true] {
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute_read_shortcuts(request(listener.local_addr().unwrap(), undo))
            )
            .await
            .unwrap(),
            Err(Error::Offline(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn confirmation_adapter_accepts_only_targeted_get_or_independent_false_set() {
    let address = "127.0.0.1:1".parse().unwrap();
    let mut get = request(address, false);
    get.url = format!("http://{address}/api/users.prefs.get");
    get.body = RequestBody::Form(vec![
        ("token".into(), "synthetic".into()),
        ("prefs".into(), "confirm_clear_all_unreads".into()),
    ]);
    assert!(allowed(&get));
    let mut set = request(address, false);
    set.url = format!("http://{address}/api/users.prefs.set");
    set.body = RequestBody::Form(vec![
        ("token".into(), "synthetic".into()),
        ("name".into(), "confirm_clear_all_unreads".into()),
        ("value".into(), "false".into()),
        ("_x_reason".into(), "prefs-api/setUserPrefByApi".into()),
    ]);
    assert!(allowed(&set));
    let RequestBody::Form(ref mut fields) = set.body else {
        unreachable!()
    };
    fields[2].1 = "true".into();
    assert!(!allowed(&set));
    let RequestBody::Form(ref mut fields) = get.body else {
        unreachable!()
    };
    fields[1].1.push_str(",mark_msgs_read_immediately");
    assert!(!allowed(&get));
}

fn cursor_request(address: std::net::SocketAddr, thread: bool) -> PreparedRequest {
    let mut fields = vec![
        ("token".into(), "synthetic".into()),
        ("channel".into(), "C_FIXTURE".into()),
        ("ts".into(), "1791884254.000002".into()),
    ];
    if thread {
        fields.extend([
            ("thread_ts".into(), "1791884254.000001".into()),
            ("read".into(), "1".into()),
        ]);
    }
    PreparedRequest {
        method: "POST",
        url: format!(
            "http://{address}/api/{}",
            if thread {
                "subscriptions.thread.mark"
            } else {
                "conversations.mark"
            }
        ),
        headers: vec![],
        body: RequestBody::Form(fields),
    }
}

#[test]
fn explicit_cursor_adapter_rejects_unread_or_missing_or_duplicate_coordinates() {
    let address = "127.0.0.1:1".parse().unwrap();
    assert!(allowed(&cursor_request(address, false)));
    assert!(allowed(&cursor_request(address, true)));
    for (key, value) in [
        ("read", "0"),
        ("thread_ts", "pending"),
        ("ts", "NaN"),
        ("channel", ""),
    ] {
        let mut request = cursor_request(address, true);
        let RequestBody::Form(ref mut fields) = request.body else {
            unreachable!()
        };
        fields.iter_mut().find(|(name, _)| name == key).unwrap().1 = value.into();
        assert!(!allowed(&request));
    }
    let mut request = cursor_request(address, false);
    let RequestBody::Form(ref mut fields) = request.body else {
        unreachable!()
    };
    fields.push(("channel".into(), "C_OTHER".into()));
    assert!(!allowed(&request));
}

#[tokio::test]
async fn explicit_cursor_requests_are_single_attempt_even_when_ack_is_lost() {
    for thread in [false, true] {
        for body in [
            Some(r#"{"ok":true}"#),
            Some(r#"{"ok":false,"error":"invalid_auth","undo_ts":"1791884254.000001"}"#),
            None,
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 4096];
                assert!(stream.read(&mut bytes).await.unwrap() > 0);
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
            let transport = Transport::new("fixture").unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                transport.execute_read_shortcuts(cursor_request(address, thread)),
            )
            .await
            .unwrap();
            if body == Some(r#"{"ok":true}"#) {
                assert!(result.is_ok());
            } else {
                assert!(matches!(result, Err(Error::Transport(_))));
            }
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn clear_and_undo_never_retry_or_follow_redirect_and_keep_contradiction_unknown() {
    for undo in [false, true] {
        for (status, body) in [
            ("200 OK", r#"{"ok":true,"undo_ts":"1791884254.000001"}"#),
            ("200 OK", r#"{"ok":false,"error":"invalid_auth"}"#),
            (
                "200 OK",
                r#"{"ok":false,"error":"invalid_auth","undo_ts":"1791884254.000001"}"#,
            ),
            ("429 Too Many Requests", ""),
            ("302 Found", ""),
            ("lost", ""),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 4096];
                assert!(stream.read(&mut bytes).await.unwrap() > 0);
                if status != "lost" {
                    stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{address}/api/users.markAllRead\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(80), listener.accept())
                        .await
                        .is_err()
                );
            });
            let transport = Transport::new("fixture").unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                transport.execute_read_shortcuts(request(address, undo)),
            )
            .await
            .unwrap();
            match status {
                "200 OK" if body.contains("invalid_auth") && body.contains("undo_ts") => {
                    assert!(matches!(result, Err(Error::Transport(_))))
                }
                "200 OK" if body.contains("invalid_auth") => {
                    assert!(matches!(result, Err(Error::Api(_))))
                }
                "200 OK" => assert!(result.is_ok()),
                "429 Too Many Requests" => assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(7)
                    })
                )),
                "lost" => assert!(matches!(result, Err(Error::Transport(_)))),
                _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
            }
            server.await.unwrap();
        }
    }
}
