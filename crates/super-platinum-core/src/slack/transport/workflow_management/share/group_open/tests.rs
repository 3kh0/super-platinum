use super::*;
use crate::{config::WorkspaceSession, slack::SlackClient};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr) -> group::Request {
    let session = WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "synthetic-group-token".into(),
    };
    group::Open::new(
        &[
            json!({"id":"UHUMAN001","team_id":"TWORKFLOW"}),
            json!({"id":"UHUMAN002","team_id":"TWORKFLOW"}),
        ],
        "TWORKFLOW",
        "USELF0001",
        None,
    )
    .unwrap()
    .request(&SlackClient::default(), &session)
    .unwrap()
}
fn ack() -> Value {
    json!({"ok":true,"channel":{"id":"GSHARE001","is_mpim":true,"members":["USELF0001","UHUMAN001","UHUMAN002"]},"opaque":{"retained":true}})
}
async fn read(s: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut b = [0; 4096];
        let n = s.read(&mut b).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&b[..n]);
        assert!(bytes.len() < 1024 * 1024);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
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
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
async fn reply(s: &mut tokio::net::TcpStream, status: &str, body: &str, extra: &str) {
    s.write_all(
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
            body.len()
        )
        .as_bytes(),
    )
    .await
    .unwrap();
}

#[test]
fn group_endpoint_whitelist_cannot_send_message_or_inject_fields() {
    let good = request("127.0.0.1:1".parse().unwrap());
    assert!(allowed(&good));
    for url in [
        "https://fixture.slack.com/api/chat.postMessage",
        "https://fixture.slack.com.evil.test/api/conversations.open",
        "https://u@fixture.slack.com/api/conversations.open",
        "https://fixture.slack.com:444/api/conversations.open",
    ] {
        let mut bad = good.clone();
        bad.request.url = url.into();
        assert!(!allowed(&bad));
    }
    for suffix in ["&token=secret", "&_x_num_retries=2", "#fragment"] {
        let mut bad = good.clone();
        bad.request.url.push_str(suffix);
        assert!(!allowed(&bad));
    }
    for (index, value) in [(1, "0"), (2, "true"), (3, "UOTHER001"), (4, "share-dialog")] {
        let mut bad = good.clone();
        let RequestBody::Form(f) = &mut bad.request.body else {
            panic!()
        };
        f[index].1 = value.into();
        assert!(!allowed(&bad));
    }
    let mut bad = good.clone();
    let RequestBody::Form(f) = &mut bad.request.body else {
        panic!()
    };
    f.push(("channel".into(), "DDMPEER01".into()));
    assert!(!allowed(&bad));
    let mut bad = good;
    bad.request
        .headers
        .push(("Cookie".into(), "foreign".into()));
    assert!(!allowed(&bad));
}
#[tokio::test]
async fn group_actual_multipart_correlated_open_is_not_a_message_ack() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let wire = read(&mut s).await;
        let (h, b) = wire.split_once("\r\n\r\n").unwrap();
        assert!(h.starts_with("POST /api/conversations.open?"));
        assert!(!h.contains("synthetic-group-token"));
        assert!(h.to_ascii_lowercase().contains("multipart/form-data"));
        assert!(b.find("name=\"token\"").unwrap() < b.find("name=\"quiet\"").unwrap());
        for (k, v) in [
            ("quiet", "1"),
            ("return_im", "1"),
            ("users", "USELF0001,UHUMAN001,UHUMAN002"),
            ("_x_reason", "startMpimByMemberIds"),
        ] {
            assert!(b.contains(&format!("name=\"{k}\"\r\n\r\n{v}\r\n")));
        }
        for k in ["channel", "blocks", "client_msg_id"] {
            assert!(!b.contains(&format!("name=\"{k}\"")));
        }
        reply(&mut s, "200 OK", &ack().to_string(), "").await;
    });
    let mut attempts = Vec::new();
    let raw = Transport::new("synthetic")
        .unwrap()
        .execute_workflow_share_group_current(&request(addr), || true, |a| attempts.push(a))
        .await
        .unwrap();
    assert_eq!(raw, ack());
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, Outcome::Acknowledged);
    assert_eq!(attempts[0].raw, Some(raw));
    server.await.unwrap();
}
#[tokio::test]
async fn group_retry_retains_three_attempts_and_server_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut last = None;
        for n in 0..3 {
            let (mut s, _) = listener.accept().await.unwrap();
            if let Some(t) = last {
                assert!(std::time::Instant::now().duration_since(t) >= Duration::from_secs(1));
            }
            let wire = read(&mut s).await;
            assert!(wire.contains(&format!("_x_num_retries={n}")));
            assert!(wire.contains("name=\"users\"\r\n\r\nUSELF0001,UHUMAN001,UHUMAN002"));
            last = Some(std::time::Instant::now());
            reply(
                &mut s,
                "503 Service Unavailable",
                &json!({"attempt":n}).to_string(),
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
    assert!(
        Transport::new("synthetic")
            .unwrap()
            .execute_workflow_share_group_current(&request(addr), || true, |a| attempts.push(a))
            .await
            .is_err()
    );
    assert_eq!(attempts.len(), 3);
    for (i, a) in attempts.iter().enumerate() {
        assert_eq!(a.raw, Some(json!({"attempt":i})));
        assert_eq!(a.number, i as u8 + 1);
    }
    server.await.unwrap();
}
#[tokio::test]
async fn group_wrong_peer_redirect_and_api_rate_limit_never_replay() {
    for (status, raw, outcome) in [
        (
            "200 OK",
            json!({"ok":true,"channel":{"id":"GSHARE001","is_mpim":true,"members":["USELF0001","UHUMAN001","UOTHER001"]}}),
            Outcome::Unknown,
        ),
        (
            "307 Temporary Redirect",
            json!({"keep":1}),
            Outcome::Unknown,
        ),
        (
            "200 OK",
            json!({"ok":false,"error":"ratelimited"}),
            Outcome::RateLimited,
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let body = raw.to_string();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            read(&mut s).await;
            reply(
                &mut s,
                status,
                &body,
                &format!("Location: http://{addr}/other\r\nRetry-After: 12\r\n"),
            )
            .await;
            drop(s);
            assert!(
                tokio::time::timeout(Duration::from_millis(150), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut attempts = Vec::new();
        assert!(
            Transport::new("synthetic")
                .unwrap()
                .execute_workflow_share_group_current(&request(addr), || true, |a| attempts.push(a))
                .await
                .is_err()
        );
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].outcome, outcome);
        assert_eq!(attempts[0].raw, Some(raw));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn group_predispatch_retirement_and_offline_send_nothing() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let t = Transport::new("synthetic").unwrap();
    let mut attempts = Vec::new();
    assert!(
        t.execute_workflow_share_group_current(&request(addr), || false, |a| attempts.push(a))
            .await
            .is_err()
    );
    t.set_health(Health::Offline);
    assert!(matches!(
        t.execute_workflow_share_group_current(&request(addr), || true, |a| attempts.push(a))
            .await,
        Err(Error::Offline(_))
    ));
    assert!(attempts.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn group_owner_retirement_stops_retry_after_retaining_response() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read(&mut s).await;
        reply(
            &mut s,
            "503 Service Unavailable",
            r#"{"original":true}"#,
            "Retry-After: 1\r\n",
        )
        .await;
        drop(s);
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    });
    let current = std::cell::Cell::new(true);
    let mut attempts = Vec::new();
    assert!(
        Transport::new("synthetic")
            .unwrap()
            .execute_workflow_share_group_current(
                &request(addr),
                || current.get(),
                |a| {
                    attempts.push(a);
                    current.set(false)
                }
            )
            .await
            .is_err()
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].raw, Some(json!({"original":true})));
    server.await.unwrap();
}

#[tokio::test]
async fn group_inflight_retirement_records_unknown_application_without_successor_retry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let current = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let server_current = current.clone();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read(&mut s).await;
        server_current.store(false, std::sync::atomic::Ordering::SeqCst);
        // The original request reached the server; cancellation proves neither
        // rollback nor a reusable group ID. No response is invented for the caller.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let mut attempts = Vec::new();
    assert!(
        Transport::new("synthetic")
            .unwrap()
            .execute_workflow_share_group_current(
                &request(addr),
                || current.load(std::sync::atomic::Ordering::SeqCst),
                |a| attempts.push(a),
            )
            .await
            .is_err()
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, Outcome::Retired);
    assert!(attempts[0].may_have_applied);
    assert!(attempts[0].raw.is_none());
    server.await.unwrap();
}
