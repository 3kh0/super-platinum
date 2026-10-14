use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr, write: bool) -> PreparedRequest {
    let mut fields = vec![("token".into(), "synthetic".into())];
    fields.push((
        "prefs".into(),
        if write {
            r#"{"mark_msgs_read_immediately":false,"start_scroll_at_oldest":false}"#.into()
        } else {
            "mark_msgs_read_immediately,start_scroll_at_oldest".into()
        },
    ));
    if write {
        fields.push((
            "_x_reason".into(),
            "prefs-api/setMultipleUserPrefsByApi".into(),
        ));
    }
    PreparedRequest {
        method: "POST",
        url: format!(
            "http://{address}/api/users.prefs.{}",
            if write { "set" } else { "get" }
        ),
        headers: vec![],
        body: RequestBody::Form(fields),
    }
}

#[test]
fn only_exact_two_key_get_and_boolean_multi_patch_are_admitted() {
    let address = "127.0.0.1:1".parse().unwrap();
    assert!(allowed(&request(address, false)));
    assert!(allowed(&request(address, true)));
    for patch in [
        r#"{"mark_msgs_read_immediately":true}"#,
        r#"{"mark_msgs_read_immediately":true,"start_scroll_at_oldest":"false"}"#,
        r#"{"mark_msgs_read_immediately":true,"start_scroll_at_oldest":false,"time24":true}"#,
        r#"{"all_notifications_prefs":{},"start_scroll_at_oldest":false}"#,
        r#"[]"#,
    ] {
        let mut bad = request(address, true);
        let RequestBody::Form(ref mut fields) = bad.body else {
            unreachable!()
        };
        fields[1].1 = patch.into();
        assert!(!allowed(&bad));
    }
    for (key, value) in [
        ("user", "U_OTHER"),
        ("token", "other"),
        ("_x_reason", "other"),
    ] {
        let mut bad = request(address, true);
        let RequestBody::Form(ref mut fields) = bad.body else {
            unreachable!()
        };
        fields.push((key.into(), value.into()));
        assert!(!allowed(&bad));
    }
    let mut bad = request(address, false);
    bad.url.push_str("?user=U_OTHER");
    assert!(!allowed(&bad));
    let mut bad = request(address, false);
    let RequestBody::Form(ref mut fields) = bad.body else {
        unreachable!()
    };
    fields[1].1.push_str(",all_notifications_prefs");
    assert!(!allowed(&bad));
}

#[tokio::test]
async fn offline_get_and_set_return_without_dispatch_or_waiting_for_reconnect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture").unwrap();
    transport.set_health(Health::Offline);
    for write in [false, true] {
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute_read_preferences(request(listener.local_addr().unwrap(), write))
            )
            .await
            .unwrap(),
            Err(Error::Offline(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn read_preference_requests_do_not_retry_lost_ack_redirect_or_rate_limit() {
    for write in [false, true] {
        for (status, body) in [
            ("200 OK", r#"{"ok":true,"prefs":{}}"#),
            ("200 OK", r#"{"ok":false,"error":"invalid_auth"}"#),
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
                    stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{address}/api/users.prefs.set\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
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
                transport.execute_read_preferences(request(address, write)),
            )
            .await
            .unwrap();
            match status {
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

#[test]
fn only_clean_known_denials_release_write_uncertainty() {
    use serde_json::json;
    for code in [
        "invalid_auth",
        "not_authed",
        "token_revoked",
        "token_expired",
        "account_inactive",
        "missing_scope",
        "invalid_arguments",
        "invalid_arg_name",
    ] {
        assert!(
            matches!(write_success(&json!({"ok":false,"error":code})), Err(Error::Api(actual)) if actual == code)
        );
    }
    assert!(
        matches!(write_success(&json!({"ok":false,"error":"missing_scope","needed":"read","provided":"write","response_metadata":{"warnings":["warning"],"messages":[]}})), Err(Error::Api(code)) if code == "missing_scope")
    );
    for response in [
        json!({"ok":false,"error":"invalid_auth","prefs":{"mark_msgs_read_immediately":true,"start_scroll_at_oldest":false}}),
        json!({"ok":false,"error":"invalid_auth","receipt":"mutation"}),
        json!({"ok":false,"error":"invalid_auth","updated":true}),
        json!({"ok":false,"error":"invalid_auth","needed":{}}),
        json!({"ok":false,"error":"invalid_auth","provided":null}),
        json!({"ok":false,"error":"invalid_auth","response_metadata":{"receipt":"mutation"}}),
        json!({"ok":false,"error":"invalid_auth","response_metadata":{"warnings":[{}]}}),
        json!({"ok":false,"error":"invalid_auth","response_metadata":null}),
        json!({"ok":true,"error":"invalid_auth"}),
        json!({"ok":"false","error":"invalid_auth"}),
        json!({"ok":false,"error":"fatal_error"}),
        json!({"ok":false,"error":"new_unknown_error"}),
        json!({"ok":false}),
        json!({"error":"invalid_auth"}),
    ] {
        assert!(
            matches!(write_success(&response), Err(Error::Transport(_))),
            "{response}"
        );
    }
}

#[tokio::test]
async fn contradictory_set_ack_is_unknown_after_one_actual_request() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        let body = r#"{"ok":false,"error":"invalid_auth","prefs":{"mark_msgs_read_immediately":false,"start_scroll_at_oldest":false}}"#;
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
    let transport = Transport::new("fixture").unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        transport.execute_read_preferences(request(address, true)),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(Error::Transport(_))));
    server.await.unwrap();
}
