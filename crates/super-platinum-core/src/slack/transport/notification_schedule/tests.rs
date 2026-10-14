use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/users.prefs.set"),
        headers: vec![],
        body: RequestBody::Form(vec![
            (
                "prefs".into(),
                r#"{"dnd_enabled":true,"dnd_days":"custom"}"#.into(),
            ),
            (
                "_x_reason".into(),
                "prefs-api/setMultipleUserPrefsByApi".into(),
            ),
        ]),
    }
}
#[tokio::test]
async fn offline_schedule_never_sends_and_other_prefs_are_rejected() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let t = Transport::new("fixture").unwrap();
    t.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(100),
            t.execute_notification_schedule(request(addr))
        )
        .await
        .unwrap(),
        Err(Error::Offline(_))
    ));
    let mut bad = request(addr);
    let RequestBody::Form(ref mut fields) = bad.body else {
        unreachable!()
    };
    fields[0].1 = r#"{"dnd_enabled":true,"all_notifications_prefs":{}}"#.into();
    assert!(!allowed(&bad));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn schedule_never_retries_lost_ack_redirect_or_rate_limit() {
    for (status, body) in [
        ("200 OK", r#"{"ok":true}"#),
        ("200 OK", r#"{"ok":false,"error":"not_allowed"}"#),
        ("429 Too Many Requests", ""),
        ("302 Found", ""),
        ("lost", ""),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            if status != "lost" {
                stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{addr}/api/users.prefs.set\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let t = Transport::new("fixture").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            t.execute_notification_schedule(request(addr)),
        )
        .await
        .unwrap();
        match status {
            "200 OK" if body.contains("not_allowed") => {
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
#[test]
fn strict_schedule_request_does_not_admit_timezone_global_or_user_override() {
    let mut req = request("127.0.0.1:1".parse().unwrap());
    assert!(allowed(&req));
    for patch in [
        r#"{"dnd_enabled":true,"tz":"UTC"}"#,
        r#"{"dnd_enabled":true,"time24":true}"#,
        r#"{"dnd_enabled":false,"dnd_days":"custom"}"#,
        r#"[{"name":"dnd_days","value":"custom"}]"#,
    ] {
        let RequestBody::Form(ref mut fields) = req.body else {
            unreachable!()
        };
        fields[0].1 = patch.into();
        assert!(!allowed(&req));
    }
    let mut req = request("127.0.0.1:1".parse().unwrap());
    req.url.push_str("?user=U_OTHER");
    assert!(!allowed(&req));
}
