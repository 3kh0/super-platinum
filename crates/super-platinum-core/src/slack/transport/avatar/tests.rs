use super::*;
use crate::config::WorkspaceSession;
use crate::slack::SlackClient;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn workspace(address: std::net::SocketAddr) -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TAVATAR".into(),
        enterprise_id: None,
        user_id: "UAVATAR".into(),
        name: "Synthetic".into(),
        url: format!("http://{address}"),
        token: "synthetic-body-token".into(),
    }
}
fn crop(address: std::net::SocketAddr) -> AvatarRequest {
    avatar::crop(
        &SlackClient::default(),
        &workspace(address),
        "AV-uploaded",
        avatar::Crop::new(1.5, 2., 128., 256, 256).unwrap(),
    )
    .unwrap()
}
fn upload(address: std::net::SocketAddr) -> AvatarRequest {
    use ::image::ImageEncoder;
    let mut bytes = Vec::new();
    ::image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &vec![42; 128 * 128 * 4],
            128,
            128,
            ::image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    avatar::upload(
        &SlackClient::default(),
        &workspace(address),
        avatar::Image::parse("Original \u{e9}.png".into(), "image/png", bytes).unwrap(),
    )
}
async fn read(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut b = [0u8; 4096];
        let n = stream.read(&mut b).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&b[..n]);
        assert!(bytes.len() < 3 * 1024 * 1024);
        if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            let h = std::str::from_utf8(&bytes[..end]).unwrap();
            let len: usize = h
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() == end + 4 + len {
                return bytes;
            }
        }
    }
}
async fn reply(stream: &mut tokio::net::TcpStream, status: &str, body: &str, extra: &str) {
    stream
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn both_real_factories_send_token_first_multipart_with_exact_original_file_or_crop() {
    for is_upload in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let req = if is_upload { upload(addr) } else { crop(addr) };
        let expected = req.image.as_ref().map(|i| i.bytes());
        assert!(allowed(&req));
        assert!(!format!("{req:?}").contains("synthetic-body-token"));
        assert!(!req.inspect().to_string().contains("synthetic-body-token"));
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let wire = read(&mut stream).await;
            let end = wire.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
            let headers = std::str::from_utf8(&wire[..end])
                .unwrap()
                .to_ascii_lowercase();
            assert!(headers.starts_with(if is_upload {
                "post /api/avatars.upload?"
            } else {
                "post /api/avatars.crop?"
            }));
            assert!(headers.contains("content-type: multipart/form-data; boundary="));
            assert!(headers.contains("cookie: d=synthetic-cookie"));
            assert!(!headers.contains("authorization:"));
            let body = &wire[end + 4..];
            let text = String::from_utf8_lossy(body);
            assert_eq!(text.matches("name=\"token\"").count(), 1);
            assert!(
                text.find("name=\"token\"").unwrap() < text.find("name=\"_x_reason\"").unwrap()
            );
            if let Some(expected) = expected {
                assert!(text.contains("filename=\"Original \u{e9}.png\""));
                assert!(text.contains("Content-Type: image/png"));
                assert!(body.windows(expected.len()).any(|v| v == &*expected));
                assert!(text.contains(avatar::UPLOAD_REASON));
                assert!(
                    text.find("name=\"token\"").unwrap() < text.find("name=\"image\"").unwrap()
                );
            } else {
                assert!(text.contains("name=\"avatar\"\r\n\r\nAV-uploaded"));
                assert!(text.contains("name=\"cropbox\"\r\n\r\n2,2,128"));
                assert!(text.contains(avatar::CROP_REASON));
                assert!(!text.contains("filename="));
            }
            reply(&mut stream,"200 OK",r#"{"ok":true,"id":"AV-uploaded","url":"https://files.slack.com/avatar","opaque":{"original":[1,2]}}"#,"").await;
        });
        let mut attempts = Vec::new();
        let raw = Transport::new("synthetic-cookie")
            .unwrap()
            .execute_workflow_avatar_current(&req, || true, |a| attempts.push(a))
            .await
            .unwrap();
        assert_eq!(raw["opaque"], json!({"original":[1,2]}));
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].outcome, Outcome::Acknowledged);
        assert_eq!(attempts[0].raw.as_ref(), Some(&raw));
        server.await.unwrap();
    }
}
#[test]
fn closed_admission_rejects_wrong_origin_body_headers_and_query_without_sending() {
    let base = crop("127.0.0.1:1".parse().unwrap());
    for url in [
        "https://evil.example/api/avatars.crop?_x_num_retries=0",
        "https://x.slack.com/api/avatars.upload?_x_num_retries=0",
        "https://user:pw@x.slack.com/api/avatars.crop?_x_num_retries=0",
        "https://x.slack.com/api/avatars.crop?_x_num_retries=0&token=secret",
        "https://x.slack.com/api/avatars.crop?_x_num_retries=0&_x_num_retries=0",
    ] {
        let mut r = base.clone();
        r.request.url = url.into();
        assert!(!allowed(&r));
    }
    let mut r = base.clone();
    r.request
        .headers
        .push(("Authorization".into(), "secret".into()));
    assert!(!allowed(&r));
    let mut r = base.clone();
    let RequestBody::Form(f) = &mut r.request.body else {
        unreachable!()
    };
    f.push(("avatar".into(), "other".into()));
    assert!(!allowed(&r));
    let mut r = base;
    let RequestBody::Form(f) = &mut r.request.body else {
        unreachable!()
    };
    f[3].1 = "other-reason".into();
    assert!(!allowed(&r));
}
#[test]
fn exact_source_retry_statuses_and_backoff_are_bounded() {
    let r = crop("127.0.0.1:1".parse().unwrap());
    for status in [301, 302, 307, 400, 401, 403, 429, 500, 502, 503, 504] {
        assert_eq!(
            classify(&r, 1, status, None, vec![]).retry,
            matches!(status, 429 | 503 | 504)
        );
    }
    for code in RATE_CODES
        .into_iter()
        .chain(["internal_error", "invalid_auth"])
    {
        let step = classify(
            &r,
            1,
            200,
            None,
            serde_json::to_vec(&json!({"ok":false,"error":code})).unwrap(),
        );
        assert_eq!(step.retry, code != "invalid_auth");
    }
    assert_eq!(
        retry_delay(1, None, false, None, 0.),
        Duration::from_secs(1)
    );
    assert_eq!(
        retry_delay(1, None, false, None, 0.999),
        Duration::from_secs(9)
    );
    assert_eq!(
        retry_delay(2, Some(503), false, None, 0.999),
        Duration::from_secs(78)
    );
    assert_eq!(
        retry_delay(1, Some(504), false, None, 0.),
        Duration::from_secs(15)
    );
    assert_eq!(
        retry_delay(2, Some(429), true, Some(999), 0.),
        Duration::from_secs(180)
    );
}
#[tokio::test]
async fn retryable_response_retains_every_raw_attempt_and_stops_at_three() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for n in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let bytes = read(&mut stream).await;
            let text = String::from_utf8_lossy(&bytes);
            assert!(text.contains(&format!("_x_num_retries={n}")));
            reply(
                &mut stream,
                "503 Service Unavailable",
                &format!("{{\"attempt\":{n}}}"),
                "Retry-After: 1\r\n",
            )
            .await;
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let mut attempts = Vec::new();
    let result = Transport::new("synthetic")
        .unwrap()
        .execute_workflow_avatar_current(&crop(addr), || true, |a| attempts.push(a))
        .await;
    assert!(matches!(result, Err(Error::HttpStatus { status: 503, .. })));
    assert_eq!(attempts.len(), 3);
    for (i, a) in attempts.iter().enumerate() {
        assert_eq!(a.number, 1 + i as u8);
        assert_eq!(a.raw, Some(json!({"attempt":i})));
        assert!(a.may_have_applied);
        assert_eq!(a.outcome, Outcome::Unknown);
    }
    assert_eq!(attempts[0].retry_after_millis, Some(1000));
    assert_eq!(attempts[2].retry_after_millis, None);
    server.await.unwrap();
}
#[tokio::test]
async fn later_ack_does_not_erase_an_earlier_unknown_attempt() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        read(&mut first).await;
        reply(
            &mut first,
            "503 Service Unavailable",
            r#"{"opaque":"first unknown"}"#,
            "Retry-After: 1\r\n",
        )
        .await;
        drop(first);
        let (mut second, _) = listener.accept().await.unwrap();
        assert!(String::from_utf8_lossy(&read(&mut second).await).contains("_x_num_retries=1"));
        reply(&mut second,"200 OK",r#"{"ok":true,"url":"https://files.slack.com/actual-crop","opaque":"second actual ACK"}"#,"").await;
    });
    let mut attempts = Vec::new();
    let raw = Transport::new("synthetic")
        .unwrap()
        .execute_workflow_avatar_current(&crop(addr), || true, |a| attempts.push(a))
        .await
        .unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].outcome, Outcome::Unknown);
    assert_eq!(attempts[0].raw, Some(json!({"opaque":"first unknown"})));
    assert_eq!(attempts[1].outcome, Outcome::Acknowledged);
    assert_eq!(attempts[1].raw.as_ref(), Some(&raw));
    server.await.unwrap();
}

#[tokio::test]
async fn redirects_rejections_and_malformed_success_do_not_replay_or_forward_cookie() {
    for (status, body, outcome) in [
        ("302 Found", "", Outcome::Unknown),
        ("403 Forbidden", "", Outcome::Rejected),
        (
            "200 OK",
            r#"{"ok":false,"error":"invalid_auth"}"#,
            Outcome::Rejected,
        ),
        (
            "200 OK",
            r#"{"ok":true,"opaque":"missing URL"}"#,
            Outcome::Unknown,
        ),
        ("200 OK", "not json", Outcome::Unknown),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read(&mut stream).await;
            reply(
                &mut stream,
                status,
                body,
                &format!("Location: http://{addr}/redirected\r\n"),
            )
            .await;
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut attempts = Vec::new();
        let result = Transport::new("synthetic")
            .unwrap()
            .execute_workflow_avatar_current(&crop(addr), || true, |a| attempts.push(a))
            .await;
        assert!(result.is_err());
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].outcome, outcome);
        assert_eq!(attempts[0].response_text.as_deref(), Some(body));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn lost_reply_is_unknown_and_retirement_prevents_source_retry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read(&mut stream).await;
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), listener.accept())
                .await
                .is_err()
        );
    });
    let current = std::cell::Cell::new(true);
    let mut attempts = Vec::new();
    let result = Transport::new("synthetic")
        .unwrap()
        .execute_workflow_avatar_current(
            &crop(addr),
            || current.get(),
            |a| {
                attempts.push(a);
                current.set(false)
            },
        )
        .await;
    assert!(matches!(result, Err(Error::Transport(_))));
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, Outcome::Unknown);
    assert!(attempts[0].may_have_applied);
    server.await.unwrap();
}
#[tokio::test]
async fn held_dispatched_request_retirement_keeps_unknown_application_and_drops_receiver() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let current = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let server_current = current.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read(&mut stream).await;
        server_current.store(false, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let mut attempts = Vec::new();
    let result = Transport::new("synthetic")
        .unwrap()
        .execute_workflow_avatar_current(
            &crop(addr),
            || current.load(Ordering::SeqCst),
            |a| attempts.push(a),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, Outcome::Retired);
    assert!(attempts[0].may_have_applied);
    server.await.unwrap();
}
#[tokio::test]
async fn offline_or_already_retired_owner_never_dispatches() {
    let transport = Transport::new("synthetic").unwrap();
    let mut attempts = Vec::new();
    let r = crop("127.0.0.1:1".parse().unwrap());
    assert!(matches!(
        transport
            .execute_workflow_avatar_current(&r, || false, |a| attempts.push(a))
            .await,
        Err(Error::Api(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_workflow_avatar_current(&r, || true, |a| attempts.push(a))
            .await,
        Err(Error::Offline(_))
    ));
    assert!(attempts.is_empty());
}
