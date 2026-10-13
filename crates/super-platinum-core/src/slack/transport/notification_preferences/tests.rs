use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/users.prefs.setNotifications"),
        headers: vec![],
        body: RequestBody::Form(vec![("prefs".into(), "synthetic".into())]),
    }
}
#[tokio::test]
async fn known_offline_and_unrelated_endpoint_never_send_or_wait() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let t = Transport::new("synthetic").unwrap();
    t.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(100),
            t.execute_notification_preferences(request(address))
        )
        .await
        .unwrap(),
        Err(Error::Offline(_))
    ));
    t.set_health(Health::Online);
    let mut bad = request(address);
    bad.url = format!("http://{address}/api/users.prefs.set");
    assert!(t.execute_notification_preferences(bad).await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn notification_mutation_never_replays_and_keeps_rejection_distinct_from_lost_ack() {
    for (status, body) in [
        ("200 OK", r#"{"ok":true}"#),
        ("200 OK", r#"{"ok":false,"error":"not_allowed"}"#),
        ("200 OK", r#"{"ok":false}"#),
        ("200 OK", r#"{"ok":true,"error":"bad"}"#),
        ("429 Too Many Requests", ""),
        ("503 Service Unavailable", ""),
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
                stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{address}/api/users.prefs.setNotifications\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let t = Transport::new("synthetic").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            t.execute_notification_preferences(request(address)),
        )
        .await
        .unwrap();
        match (status, body) {
            ("200 OK", r#"{"ok":true}"#) => assert!(result.is_ok()),
            ("200 OK", r#"{"ok":false,"error":"not_allowed"}"#) => {
                assert!(matches!(result,Err(Error::Api(code)) if code=="not_allowed"))
            }
            ("200 OK", _) | ("lost", _) => assert!(matches!(result, Err(Error::Transport(_)))),
            ("429 Too Many Requests", _) => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            )),
            _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
        }
        server.await.unwrap();
    }
}
#[test]
fn origin_and_path_are_strict() {
    let mut req = request("127.0.0.1:1".parse().unwrap());
    for url in [
        "https://evil.invalid/api/users.prefs.setNotifications",
        "https://slack.com.evil.invalid/api/users.prefs.setNotifications",
        "https://fixture.slack.com:444/api/users.prefs.setNotifications",
        "https://user@fixture.slack.com/api/users.prefs.setNotifications",
        "https://fixture.slack.com/api/users.prefs.setNotifications/",
        "https://fixture.slack.com/api/users.prefs.setNotifications#fragment",
    ] {
        req.url = url.into();
        assert!(!allowed(&req));
    }
    req.url = "https://fixture.slack.com/api/users.prefs.setNotifications".into();
    assert!(allowed(&req));
    req.method = "GET";
    assert!(!allowed(&req));
}
