use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(addr: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{addr}/api/users.prefs.setNotifications"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("global".into(), "true".into()),
            ("name".into(), "desktop_sound".into()),
            ("value".into(), "b2.mp3".into()),
            ("sync".into(), "false".into()),
            ("_x_reason".into(), "".into()),
        ]),
    }
}
#[tokio::test]
async fn presentation_mutation_known_offline_never_sends() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let t = Transport::new("synthetic").unwrap();
    t.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(100),
            t.execute_presentation_notifications(request(listener.local_addr().unwrap()))
        )
        .await
        .unwrap(),
        Err(Error::Offline(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn presentation_mutation_single_attempt_retains_rate_limit_and_uncertainty() {
    for (status, body) in [
        ("200 OK", r#"{"ok":true}"#),
        ("200 OK", r#"{"ok":false,"error":"not_allowed"}"#),
        ("200 OK", r#"{"ok":false}"#),
        ("429 Too Many Requests", ""),
        ("302 Found", ""),
        ("lost", ""),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(stream.read(&mut buf).await.unwrap() > 0);
            if status != "lost" {
                stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 8\r\nLocation: http://{addr}/api/users.prefs.setNotifications\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let t = Transport::new("synthetic").unwrap();
        let result = t.execute_presentation_notifications(request(addr)).await;
        match (status, body) {
            ("200 OK", r#"{"ok":true}"#) => assert!(result.is_ok()),
            ("200 OK", r#"{"ok":false,"error":"not_allowed"}"#) => {
                assert!(matches!(result, Err(Error::Api(_))))
            }
            ("429 Too Many Requests", _) => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(8)
                })
            )),
            ("302 Found", _) => {
                assert!(matches!(result, Err(Error::HttpStatus { status: 302, .. })))
            }
            _ => assert!(matches!(result, Err(Error::Transport(_)))),
        }
        server.await.unwrap();
    }
}
#[test]
fn admission_separates_endpoints_and_rejects_arbitrary_prefs_or_owner_override() {
    let req = request("127.0.0.1:1".parse().unwrap());
    assert!(allowed(&req));
    for (key, value) in [
        ("name", "mute_sounds"),
        ("value", "../sound.mp3"),
        ("sync", "true"),
        ("user", "U_OTHER"),
        ("channel_id", "C1"),
    ] {
        let mut bad = req.clone();
        let RequestBody::Form(ref mut fields) = bad.body else {
            unreachable!()
        };
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
            *v = value.into();
        } else {
            fields.push((key.into(), value.into()));
        }
        assert!(!allowed(&bad));
    }
    let mut bad = req.clone();
    bad.url.push_str("?user=U_OTHER");
    assert!(!allowed(&bad));
    let mut user = req.clone();
    user.url = user.url.replace("setNotifications", "set");
    user.body = RequestBody::Form(vec![
        ("name".into(), "mute_sounds".into()),
        ("value".into(), "false".into()),
        ("_x_reason".into(), "prefs-api/setUserPrefByApi".into()),
    ]);
    assert!(allowed(&user));
    let RequestBody::Form(ref mut fields) = user.body else {
        unreachable!()
    };
    fields.push(("global".into(), "true".into()));
    assert!(!allowed(&user));
    let mut read = req;
    read.url = read.url.replace("setNotifications", "get");
    read.body = RequestBody::Form(vec![(
        "prefs".into(),
        "all_notifications_prefs,mute_sounds,mac_ssb_bounce".into(),
    )]);
    assert!(allowed(&read));
}

#[test]
fn bounce_admission_only_accepts_exact_user_scalar_values() {
    let mut req = request("127.0.0.1:1".parse().unwrap());
    req.url = req.url.replace("setNotifications", "set");
    for value in ["", "short", "long", "critical", "false", "future"] {
        req.body = RequestBody::Form(vec![
            ("name".into(), "mac_ssb_bounce".into()),
            ("value".into(), value.into()),
            ("_x_reason".into(), "prefs-api/setUserPrefByApi".into()),
        ]);
        assert_eq!(allowed(&req), matches!(value, "" | "short" | "long"));
    }
    req.body = RequestBody::Form(vec![
        ("name".into(), "mac_ssb_bounce".into()),
        ("value".into(), "long".into()),
        ("_x_reason".into(), "prefs-api/setUserPrefByApi".into()),
        ("user".into(), "other".into()),
    ]);
    assert!(!allowed(&req));
}
