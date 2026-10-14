use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn request(url: String) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url,
        headers: Vec::new(),
        body: RequestBody::Form(Vec::new()),
    }
}

async fn respond(listener: TcpListener, status: &str, extra: &str, body: &str) {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut input = vec![0; 8192];
    socket.read(&mut input).await.unwrap();
    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",body.len()).as_bytes()).await.unwrap();
    drop(socket);
    assert!(
        tokio::time::timeout(Duration::from_millis(350), listener.accept())
            .await
            .is_err(),
        "request unexpectedly retried"
    );
}

#[test]
fn structured_response_allowlist_uses_exact_path_and_post() {
    for endpoint in [
        "blocks.actions",
        "blocks.suggestions",
        "views.get",
        "views.submit",
        "views.close",
    ] {
        assert!(is_block_interaction_request(&request(format!(
            "https://fixture.slack.com/api/{endpoint}?reason=test"
        ))));
    }
    for url in [
        "https://fixture.slack.com/api/chat.postMessage?endpoint=/api/views.submit",
        "https://fixture.slack.com/api/views.submit/extra",
        "not a URL",
    ] {
        assert!(!is_block_interaction_request(&request(url.into())));
    }
    let mut get = request("https://fixture.slack.com/api/views.get".into());
    get.method = "GET";
    assert!(!is_block_interaction_request(&get));
}

#[tokio::test]
async fn structured_feedback_survives_while_generic_api_errors_remain_errors() {
    for structured in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            respond(listener,"200 OK","",r#"{"ok":false,"error":"validation_failed","errors":{"note":"Fix this"},"view_error":"Keep form"}"#).await
        });
        let transport = Transport::new("fixture-cookie").unwrap();
        let req = request(format!("http://{address}/api/views.submit"));
        let result = if structured {
            transport.execute_block_interaction(req).await
        } else {
            transport.execute(req).await
        };
        if structured {
            let raw = result.unwrap();
            assert_eq!(raw["errors"]["note"], "Fix this");
            assert_eq!(raw["view_error"], "Keep form");
        } else {
            assert!(matches!(result,Err(Error::Api(code)) if code=="validation_failed"));
        }
        assert_eq!(transport.health(), Health::Online);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn structured_rate_limits_server_errors_and_bad_json_do_not_retry() {
    for (status, extra, body, kind) in [
        (
            "429 Too Many Requests",
            "Retry-After: 7\r\n",
            "private body",
            "rate",
        ),
        ("503 Unavailable", "", "private body", "http"),
        ("200 OK", "", "private invalid JSON", "json"),
        ("200 OK", "", "[]", "json"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { respond(listener, status, extra, body).await });
        let transport = Transport::new("fixture-cookie").unwrap();
        let error = transport
            .execute_block_interaction(request(format!("http://{address}/api/views.close")))
            .await
            .unwrap_err();
        match kind {
            "rate" => assert!(matches!(
                error,
                Error::RateLimited {
                    retry_after_secs: Some(7)
                }
            )),
            "http" => assert!(matches!(error, Error::HttpStatus { status: 503, .. })),
            _ => assert!(matches!(error, Error::Transport(_))),
        }
        assert!(!error.to_string().contains("private"));
        assert_eq!(transport.health(), Health::Online);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn unsupported_structured_endpoint_never_sends_and_refused_connect_marks_offline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let error = transport
        .execute_block_interaction(request(format!("http://{address}/api/chat.postMessage")))
        .await
        .unwrap_err();
    assert!(matches!(error,Error::Api(code) if code=="unsupported_block_interaction_request"));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    drop(listener);
    let error = transport
        .execute_block_interaction(request(format!(
            "http://{address}/api/blocks.actions?token=private"
        )))
        .await
        .unwrap_err();
    assert!(error.is_offline());
    assert_eq!(transport.health(), Health::Offline);
    assert!(!error.to_string().contains("private"));
}

#[tokio::test]
async fn structured_read_waits_for_confirmed_link_recovery() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    let call = tokio::spawn({
        let transport = transport.clone();
        async move {
            transport
                .execute_block_interaction(request(format!("http://{address}/api/views.get")))
                .await
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    assert!(!call.is_finished());
    let server = tokio::spawn(async move {
        respond(listener, "200 OK", "", r#"{"ok":true,"view":{"id":"V1"}}"#).await
    });
    transport.set_health(Health::Unknown);
    let result = tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["view"]["id"], "V1");
    assert_eq!(transport.health(), Health::Online);
    server.await.unwrap();
}

#[tokio::test]
async fn offline_mutations_are_rejected_without_queuing_for_recovery() {
    for endpoint in ["blocks.actions", "views.submit", "views.close"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let transport = Transport::new("fixture-cookie").unwrap();
        transport.set_health(Health::Offline);
        let result = tokio::time::timeout(
            Duration::from_millis(250),
            transport
                .execute_block_interaction(request(format!("http://{address}/api/{endpoint}"))),
        )
        .await
        .expect("offline mutation must not wait for link recovery");
        assert!(
            matches!(result, Err(Error::Api(code)) if code == "block_interaction_not_sent_offline")
        );
        assert_eq!(transport.health(), Health::Offline);
        transport.set_health(Health::Online);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err(),
            "an unsent mutation must not be replayed when the link recovers"
        );
    }
}

#[tokio::test]
async fn action_and_submit_server_errors_are_single_attempts() {
    for endpoint in ["blocks.actions", "views.submit"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server =
            tokio::spawn(
                async move { respond(listener, "503 Unavailable", "", "private body").await },
            );
        let transport = Transport::new("fixture-cookie").unwrap();
        let result = transport
            .execute_block_interaction(request(format!("http://{address}/api/{endpoint}")))
            .await;
        assert!(matches!(result, Err(Error::HttpStatus { status: 503, .. })));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn suggestions_rate_limit_is_single_attempt_and_retains_cooldown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        respond(
            listener,
            "429 Too Many Requests",
            "Retry-After: 9\r\n",
            "private response",
        )
        .await
    });
    let error = Transport::new("fixture-cookie")
        .unwrap()
        .execute_block_interaction(request(format!("http://{address}/api/blocks.suggestions")))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::RateLimited {
            retry_after_secs: Some(9)
        }
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn suggestions_reject_oversize_before_decode_with_or_without_declared_length() {
    let limit = crate::block_actions::external::MAX_SUGGESTIONS_BYTES;
    for declared in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut input = [0; 8192];
            socket.read(&mut input).await.unwrap();
            if declared {
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",limit+1).as_bytes()).await.unwrap();
            } else {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
                let chunk = vec![b' '; 65536];
                for _ in 0..=(limit / 65536) {
                    if socket.write_all(b"10000\r\n").await.is_err()
                        || socket.write_all(&chunk).await.is_err()
                        || socket.write_all(b"\r\n").await.is_err()
                    {
                        break;
                    }
                }
                let _ = socket.write_all(b"0\r\n\r\n").await;
            }
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(350), listener.accept())
                    .await
                    .is_err()
            );
        });
        let error = Transport::new("fixture-cookie")
            .unwrap()
            .execute_block_interaction(request(format!("http://{address}/api/blocks.suggestions")))
            .await
            .unwrap_err();
        assert!(
            matches!(error,Error::Transport(ref text) if text=="invalid or excessive suggestions response")
        );
        server.await.unwrap();
    }
}
