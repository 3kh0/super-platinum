use super::*;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

// Exercise the public admission path with a test-only loopback origin. Request
// paths, bodies, headers, decoding, and the actual HTTP client remain unchanged.
async fn serve(
    ordered: bool,
    status: &str,
    headers: &str,
    body: &str,
    retire: Option<Arc<AtomicBool>>,
) -> (PreparedRequest, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut input = Vec::new();
        loop {
            let mut bytes = [0; 4096];
            let read = socket.read(&mut bytes).await.unwrap();
            assert!(read > 0, "request ended before its complete body");
            input.extend_from_slice(&bytes[..read]);
            assert!(input.len() < 65536);
            if let Some(end) = input.windows(4).position(|v| v == b"\r\n\r\n") {
                let head = std::str::from_utf8(&input[..end]).unwrap();
                let length: usize = head
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if input.len() >= end + 4 + length {
                    assert_eq!(input.len(), end + 4 + length);
                    break;
                }
            }
        }
        if let Some(owner) = retire {
            owner.store(false, Ordering::Release);
        }
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
        // Completion must return without backoff/replay. Keep the listener
        // alive to catch an immediate lower-level retry or redirect too.
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err(),
            "group read dispatched a second request"
        );
        String::from_utf8(input).unwrap()
    });
    let mut req = request(ordered);
    let path = if ordered {
        "/cache/T00000001/usergroups/info"
    } else {
        "/api/subteams.info"
    };
    req.url = format!("http://{address}{path}");
    req.headers
        .push(("X-Group-Witness".into(), "original".into()));
    assert!(allowed(&req, ordered));
    assert!(!req.retry_safe());
    (req, server)
}

async fn execute(
    transport: &Transport,
    req: PreparedRequest,
    ordered: bool,
    current: impl Fn() -> bool,
) -> Result<Value, Error> {
    tokio::time::timeout(Duration::from_secs(3), async {
        if ordered {
            transport
                .execute_directory_usergroup_info_current(req, current)
                .await
                .map(|response| response.raw)
        } else {
            transport
                .execute_usergroup_fallback_current(req, current)
                .await
        }
    })
    .await
    .expect("single group attempt must not wait for a Retry-After delay")
}

fn wire(input: &str, ordered: bool) {
    let (headers, body) = input.split_once("\r\n\r\n").unwrap();
    let path = if ordered {
        "/cache/T00000001/usergroups/info"
    } else {
        "/api/subteams.info"
    };
    assert_eq!(
        headers.lines().next().unwrap(),
        format!("POST {path} HTTP/1.1")
    );
    let headers = headers.to_ascii_lowercase();
    assert!(headers.contains("\r\ncookie: d=synthetic"));
    assert!(headers.contains("\r\nx-group-witness: original"));
    match request(ordered).body {
        RequestBody::Json(expected) => {
            assert!(headers.contains("content-type: application/json"));
            assert_eq!(serde_json::from_str::<Value>(body).unwrap(), expected);
        }
        RequestBody::Form(expected) => {
            assert!(headers.contains("content-type: application/x-www-form-urlencoded"));
            let actual: Vec<(String, String)> = url::form_urlencoded::parse(body.as_bytes())
                .into_owned()
                .collect();
            assert_eq!(actual, expected);
        }
    }
}

#[tokio::test]
async fn both_routes_preserve_full_header_for_http_and_api_rate_limits_once() {
    for ordered in [false, true] {
        for status in ["429 Too Many Requests", "200 OK"] {
            // Above the old UI delay cap and u32; no truncation or clamping.
            let (req, server) = serve(
                ordered,
                status,
                "Retry-After: 4294967301\r\n",
                r#"{"ok":false,"error":"ratelimited"}"#,
                None,
            )
            .await;
            let transport = Transport::new("synthetic").unwrap();
            assert!(matches!(
                execute(&transport, req, ordered, || true).await,
                Err(Error::RateLimited {
                    retry_after_secs: Some(4294967301)
                })
            ));
            wire(&server.await.unwrap(), ordered);
        }
    }
}

#[tokio::test]
async fn missing_invalid_and_zero_headers_are_not_invented_or_clamped() {
    for ordered in [false, true] {
        for (header, expected) in [
            ("", None),
            ("Retry-After: tomorrow\r\n", None),
            ("Retry-After: 0\r\n", Some(0)),
        ] {
            let (req, server) = serve(
                ordered,
                "200 OK",
                header,
                r#"{"ok":false,"error":"ratelimited"}"#,
                None,
            )
            .await;
            let transport = Transport::new("synthetic").unwrap();
            match execute(&transport, req, ordered, || true).await {
                Err(Error::RateLimited { retry_after_secs }) => {
                    assert_eq!(retry_after_secs, expected)
                }
                other => panic!("rate-limit category lost: {other:?}"),
            }
            wire(&server.await.unwrap(), ordered);
        }
    }
}

#[tokio::test]
async fn issued_rate_limit_survives_owner_retirement_without_authorizing_publication() {
    for ordered in [false, true] {
        let owner = Arc::new(AtomicBool::new(true));
        let (req, server) = serve(
            ordered,
            "200 OK",
            "Retry-After: 91\r\n",
            r#"{"ok":false,"error":"ratelimited"}"#,
            Some(owner.clone()),
        )
        .await;
        let transport = Transport::new("synthetic").unwrap();
        assert!(matches!(
            execute(&transport, req, ordered, || owner.load(Ordering::Acquire)).await,
            Err(Error::RateLimited {
                retry_after_secs: Some(91)
            })
        ));
        assert!(!owner.load(Ordering::Acquire));
        wire(&server.await.unwrap(), ordered);
        // This error is endpoint cooldown evidence. The original owner is
        // still retired; returning an issued error does not revive it.
    }
}

#[tokio::test]
async fn retired_public_owner_never_dispatches() {
    for ordered in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut req = request(ordered);
        let path = url::Url::parse(&req.url).unwrap().path().to_owned();
        req.url = format!("http://{}{path}", listener.local_addr().unwrap());
        let transport = Transport::new("synthetic").unwrap();
        assert!(matches!(
            execute(&transport, req, ordered, || false).await,
            Err(Error::Offline(message)) if message.contains("owner retired")
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn generic_execution_keeps_existing_api_error_for_same_group_routes() {
    for ordered in [false, true] {
        let (req, server) = serve(
            ordered,
            "200 OK",
            "Retry-After: 120\r\n",
            r#"{"ok":false,"error":"ratelimited"}"#,
            None,
        )
        .await;
        let transport = Transport::new("synthetic").unwrap();
        let result = tokio::time::timeout(Duration::from_secs(3), transport.execute(req))
            .await
            .unwrap();
        assert!(matches!(result, Err(Error::Api(code)) if code == "ratelimited"));
        wire(&server.await.unwrap(), ordered);
    }
}

#[tokio::test]
async fn other_api_errors_and_retryable_http_failures_do_not_replay_group_reads() {
    for ordered in [false, true] {
        for (status, body) in [
            ("200 OK", r#"{"ok":false,"error":"invalid_auth"}"#),
            ("503 Service Unavailable", "{}"),
            ("302 Found", "{}"),
        ] {
            let (req, server) = serve(
                ordered,
                status,
                "Retry-After: 99\r\nLocation: /unexpected\r\n",
                body,
                None,
            )
            .await;
            let transport = Transport::new("synthetic").unwrap();
            let result = execute(&transport, req, ordered, || true).await;
            if status.starts_with("200") {
                assert!(matches!(result, Err(Error::Api(code)) if code == "invalid_auth"));
            } else {
                let expected = if status.starts_with("503") { 503 } else { 302 };
                assert!(
                    matches!(result, Err(Error::HttpStatus {status, retry_after_secs:Some(99)}) if status==expected)
                );
            }
            wire(&server.await.unwrap(), ordered);
        }
    }
}

#[test]
fn header_opt_in_requires_exact_route_body_and_rate_envelope() {
    let raw = json!({"ok":false,"error":"ratelimited"});
    for ordered in [false, true] {
        let req = request(ordered);
        assert!(rate_limited(&req, &raw));
        for wrong in [
            json!({"ok":true,"error":"ratelimited"}),
            json!({"ok":false,"error":"invalid_auth"}),
            json!({"error":"ratelimited"}),
            json!({"ok":0,"error":"ratelimited"}),
        ] {
            assert!(!rate_limited(&req, &wrong));
        }
        for url in [
            "https://evil.invalid/api/subteams.info",
            "https://fixture.slack.com/api/subteams.update",
            "https://edgeapi.slack.com/cache/T00000001/usergroups/search",
            "https://edgeapi.slack.com/cache/T00000001/users/info",
        ] {
            let mut wrong = request(ordered);
            wrong.url = url.into();
            assert!(!rate_limited(&wrong, &raw));
        }
        let mut wrong = request(ordered);
        wrong.method = "GET";
        assert!(!rate_limited(&wrong, &raw));
        match &mut wrong.body {
            RequestBody::Json(body) => body["ids"] = json!([]),
            RequestBody::Form(body) => body.push(("unexpected".into(), "true".into())),
        }
        wrong.method = "POST";
        assert!(!rate_limited(&wrong, &raw));
    }
}
