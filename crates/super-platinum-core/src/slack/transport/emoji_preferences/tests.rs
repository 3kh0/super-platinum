use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr, write: bool) -> PreparedRequest {
    let mut fields = vec![("token".into(), "fixture-only-token".into())];
    if write {
        fields.push(("name".into(), "frecency".into()));
        fields.push((
            "value".into(),
            r#"{"z +&":{"id":"Ez +&","count":1,"visits":[1700000000000]}}"#.into(),
        ));
    } else {
        fields.push((
            "prefs".into(),
            "preferred_skin_tone,emoji_mode,emoji_use,frecency".into(),
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
async fn read_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..at]);
            let len = headers
                .lines()
                .find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            if bytes.len() >= at + 4 + len {
                return bytes;
            }
        }
    }
}
#[test]
fn only_exact_emoji_read_and_named_frecency_write_are_admitted() {
    let address = "127.0.0.1:1".parse().unwrap();
    assert!(allowed(&request(address, false)));
    assert!(allowed(&request(address, true)));
    for (key, value) in [
        ("name", "emoji_use"),
        ("user", "U_OTHER"),
        ("token", "second-token"),
        ("prefs", "all_notifications_prefs"),
    ] {
        let mut bad = request(address, true);
        let RequestBody::Form(ref mut fields) = bad.body else {
            unreachable!()
        };
        fields.push((key.into(), value.into()));
        assert!(!allowed(&bad));
    }
    let mut bad = request(address, false);
    bad.url.push_str("?user=OTHER");
    assert!(!allowed(&bad));
    let mut bad = request(address, true);
    let RequestBody::Form(ref mut fields) = bad.body else {
        unreachable!()
    };
    fields[1].1 = "emoji_use".into();
    assert!(!allowed(&bad));
    let session = crate::config::WorkspaceSession {
        team_id: "T_SYNTHETIC".into(),
        user_id: "U_SYNTHETIC".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
    };
    let client = crate::slack::SlackClient::default();
    assert!(allowed(&crate::composer_emoji::api::preferences_request(
        &client, &session
    )));
    assert!(allowed(
        &crate::composer_emoji::api::frecency_request(&client, &session, "{}".into()).unwrap()
    ));
}
#[tokio::test]
async fn offline_pref_calls_never_wait_or_reach_the_isolated_listener() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture-only-cookie").unwrap();
    transport.set_health(Health::Offline);
    for write in [false, true] {
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute_emoji_preferences(request(listener.local_addr().unwrap(), write))
            )
            .await
            .unwrap(),
            Err(Error::Offline(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn actual_http_encoding_retry_after_and_lost_response_are_single_attempt() {
    for write in [false, true] {
        for status in [
            "200 OK",
            "429 Too Many Requests",
            "503 Service Unavailable",
            "lost",
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let bytes = read_request(&mut socket).await;
                if status != "lost" {
                    let body = r#"{"ok":true,"prefs":{}}"#;
                    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nRetry-After: 125\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }
                drop(socket);
                assert!(
                    tokio::time::timeout(Duration::from_millis(80), listener.accept())
                        .await
                        .is_err(),
                    "unexpected retry"
                );
                bytes
            });
            let transport = Transport::new("fixture-only-cookie").unwrap();
            let prepared = request(address, write);
            let RequestBody::Form(expected) = &prepared.body else {
                unreachable!()
            };
            let expected = expected.clone();
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                transport.execute_emoji_preferences(prepared),
            )
            .await
            .unwrap();
            match status {
                "200 OK" => assert!(result.is_ok()),
                "429 Too Many Requests" => assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(125)
                    })
                )),
                "503 Service Unavailable" => {
                    assert!(matches!(result, Err(Error::HttpStatus { status: 503, .. })))
                }
                "lost" => assert!(matches!(result, Err(Error::Transport(_)))),
                _ => unreachable!(),
            }
            let bytes = server.await.unwrap();
            let at = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
            let actual: Vec<(String, String)> = url::form_urlencoded::parse(&bytes[at + 4..])
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            assert_eq!(actual, expected);
        }
    }
}
