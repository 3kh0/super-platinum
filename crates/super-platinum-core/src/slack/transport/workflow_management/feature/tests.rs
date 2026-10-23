use super::*;
use crate::{
    block_actions::posting_policy::Flags,
    channel_workflows::{Previews, feature_permission::Permission},
    config::WorkspaceSession,
    slack::SlackClient,
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> Request {
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "synthetic-body-token".into(),
    };
    let user = json!({"id":"U1","is_restricted":false});
    feature::Add::new(
        &json!({"id":"FtNew01","type":"shortcut"}),
        &json!({"id":"CADDDEST01","is_member":true}),
        &Previews::decode(&[], json!({"ok":true,"triggers":[]})).unwrap(),
        feature::Facts {
            team_id: "T1",
            top_level_team_id: "T1",
            self_member: &user,
            posting: Flags {
                read_only: false,
                thread_only: false,
            },
            can_post: true,
            manage: Permission::Allowed,
        },
    )
    .unwrap()
    .request(&SlackClient::default(), &session)
    .unwrap()
}
async fn read(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut b = [0; 4096];
        let n = stream.read(&mut b).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&b[..n]);
        assert!(bytes.len() < 1024 * 1024);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let len: usize = headers
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
#[test]
fn admission_rejects_foreign_origins_redirect_sources_wrong_methods_and_field_injection() {
    let good = request("127.0.0.1:1".parse().unwrap());
    assert!(allowed(&good));
    for url in [
        "https://example.test/api/workflows.featured.add",
        "https://fixture.slack.com.evil.test/api/workflows.featured.add",
        "http://fixture.slack.com/api/workflows.featured.add",
        "https://fixture.slack.com:444/api/workflows.featured.add",
        "https://u@fixture.slack.com/api/workflows.featured.add",
        "https://fixture.slack.com/api/workflows.featured.remove",
    ] {
        let mut bad = good.clone();
        bad.request.url = url.into();
        assert!(!allowed(&bad));
    }
    for suffix in [
        "&token=secret",
        "&trigger_ids=Other",
        "&_x_num_retries=1",
        "#fragment",
    ] {
        let mut bad = good.clone();
        bad.request.url.push_str(suffix);
        assert!(!allowed(&bad));
    }
    let mut bad = good.clone();
    bad.request.method = "GET";
    assert!(!allowed(&bad));
    for (index, value) in [
        (1, "[\"FtNew01\"]"),
        (1, "FtNew01,Other"),
        (2, "wrong/channel"),
        (3, "wrong-reason"),
    ] {
        let mut bad = good.clone();
        let RequestBody::Form(fields) = &mut bad.request.body else {
            panic!()
        };
        fields[index].1 = value.into();
        assert!(!allowed(&bad));
    }
    let mut bad = good.clone();
    let RequestBody::Form(fields) = &mut bad.request.body else {
        panic!()
    };
    fields.push(("token".into(), "extra".into()));
    assert!(!allowed(&bad));
    for header in [
        "Cookie",
        "Authorization",
        "Host",
        "Content-Type",
        "Content-Length",
    ] {
        let mut bad = good.clone();
        bad.request.headers.push((header.into(), "bad".into()));
        assert!(!allowed(&bad));
    }
}
#[tokio::test]
async fn real_factory_sends_token_first_multipart_csv_and_retains_opaque_ack() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let wire = read(&mut s).await;
        let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-type: multipart/form-data; boundary=")
        );
        assert!(
            !headers
                .lines()
                .next()
                .unwrap()
                .contains("synthetic-body-token")
        );
        assert!(headers.lines().next().unwrap().contains("_x_num_retries=0"));
        let token = body.find("name=\"token\"").unwrap();
        let target = body.find("name=\"trigger_ids\"").unwrap();
        assert!(token < target);
        assert!(body.contains("name=\"trigger_ids\"\r\n\r\nFtNew01\r\n"));
        assert!(body.contains("name=\"channel_id\"\r\n\r\nCADDDEST01\r\n"));
        assert!(!body.contains("[\"FtNew01\"]"));
        reply(
            &mut s,
            "200 OK",
            r#"{"ok":true,"opaque":{"keep":[1,"actual"]}}"#,
            "",
        )
        .await;
    });
    let mut attempts = Vec::new();
    let raw = Transport::new("synthetic-cookie")
        .unwrap()
        .execute_workflow_feature_current(&request(addr), || true, |a| attempts.push(a))
        .await
        .unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, Outcome::Acknowledged);
    assert_eq!(attempts[0].raw.as_ref(), Some(&raw));
    server.await.unwrap();
}
#[test]
fn classification_and_source_backoff_keep_rate_limit_adaptation_explicit() {
    for status in [301, 307, 400, 403, 429, 500, 502, 503, 504] {
        assert_eq!(
            classify(1, status, None, vec![]).retry,
            matches!(status, 429 | 503 | 504)
        );
    }
    for code in RATE_CODES {
        let s = classify(
            1,
            200,
            Some(12),
            serde_json::to_vec(&json!({"ok":false,"error":code})).unwrap(),
        );
        assert!(!s.retry);
        assert_eq!(s.attempt.outcome, Outcome::RateLimited);
        assert!(matches!(
            s.result,
            Err(Error::RateLimited {
                retry_after_secs: Some(12)
            })
        ));
    }
    assert!(
        classify(
            1,
            200,
            None,
            br#"{"ok":false,"error":"internal_error"}"#.to_vec()
        )
        .retry
    );
    assert!(
        !classify(
            1,
            200,
            None,
            br#"{"ok":false,"error":"not_allowed"}"#.to_vec()
        )
        .retry
    );
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
        Duration::from_secs(999)
    ); // never shorten server deadline
}
#[tokio::test]
async fn retry_retains_three_actual_attempts_and_honors_deadline_without_fourth() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut last = None;
        for number in 0..3 {
            let (mut s, _) = listener.accept().await.unwrap();
            if let Some(last) = last {
                assert!(std::time::Instant::now().duration_since(last) >= Duration::from_secs(1));
            }
            let wire = read(&mut s).await;
            assert!(wire.contains(&format!("_x_num_retries={number}")));
            last = Some(std::time::Instant::now());
            reply(
                &mut s,
                "503 Service Unavailable",
                &format!("{{\"actualAttempt\":{number}}}"),
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
        .execute_workflow_feature_current(&request(addr), || true, |a| attempts.push(a))
        .await;
    assert!(matches!(result, Err(Error::HttpStatus { status: 503, .. })));
    assert_eq!(attempts.len(), 3);
    for (i, a) in attempts.iter().enumerate() {
        assert_eq!(a.number, i as u8 + 1);
        assert_eq!(a.raw, Some(json!({"actualAttempt":i})));
        assert_eq!(a.outcome, Outcome::Unknown);
        assert!(a.may_have_applied);
    }
    assert_eq!(attempts[0].retry_after_millis, Some(1000));
    server.await.unwrap();
}
#[tokio::test]
async fn redirects_api_rate_limits_and_malformed_replies_do_not_replay() {
    for (status, body, outcome) in [
        ("307 Temporary Redirect", "", Outcome::Unknown),
        (
            "200 OK",
            r#"{"ok":false,"error":"ratelimited"}"#,
            Outcome::RateLimited,
        ),
        (
            "200 OK",
            r#"{"ok":true,"error":"contradiction"}"#,
            Outcome::Unknown,
        ),
        ("200 OK", "bad JSON", Outcome::Unknown),
        (
            "200 OK",
            r#"{"ok":false,"error":"not_allowed"}"#,
            Outcome::Rejected,
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            read(&mut s).await;
            reply(
                &mut s,
                status,
                body,
                &format!("Location: http://{addr}/redirected\r\nRetry-After: 12\r\n"),
            )
            .await;
            drop(s);
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
                .execute_workflow_feature_current(&request(addr), || true, |a| attempts.push(a))
                .await
                .is_err()
        );
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].outcome, outcome);
        assert_eq!(attempts[0].response_text.as_deref(), Some(body));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn retirement_after_unknown_receipt_prevents_retry_without_erasing_receipt() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read(&mut s).await;
        reply(
            &mut s,
            "503 Service Unavailable",
            r#"{"opaque":"original"}"#,
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
            .execute_workflow_feature_current(
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
    assert_eq!(attempts[0].raw, Some(json!({"opaque":"original"})));
    assert!(attempts[0].may_have_applied);
    server.await.unwrap();
}
#[tokio::test]
async fn predispatch_retirement_has_no_network_or_attempt_and_inflight_retirement_is_unknown() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut attempts = Vec::new();
    assert!(
        Transport::new("synthetic")
            .unwrap()
            .execute_workflow_feature_current(&request(addr), || false, |a| attempts.push(a))
            .await
            .is_err()
    );
    assert!(attempts.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    let current = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let server_current = current.clone();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read(&mut s).await;
        server_current.store(false, std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(150)).await;
    });
    assert!(
        Transport::new("synthetic")
            .unwrap()
            .execute_workflow_feature_current(
                &request(addr),
                || current.load(std::sync::atomic::Ordering::SeqCst),
                |a| attempts.push(a)
            )
            .await
            .is_err()
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].outcome, Outcome::Retired);
    assert!(attempts[0].may_have_applied);
    server.await.unwrap();
}
