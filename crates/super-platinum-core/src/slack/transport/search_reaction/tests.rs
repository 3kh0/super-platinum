use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
};
use std::sync::atomic::AtomicBool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const C: &str = "C01234567";
const T: &str = "1700000001.000001";
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST".into(),
        user_id: "UTEST".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn request(adding: bool) -> PreparedRequest {
    let w = workspace();
    let c = SlackClient::default();
    if adding {
        api::reactions_add(&c, &w, C.into(), T.into(), "wave".into())
    } else {
        api::reactions_remove(&c, &w, C.into(), T.into(), "wave".into())
    }
}
async fn consume(s: &mut tokio::net::TcpStream) -> String {
    let mut data = Vec::new();
    loop {
        let mut b = [0; 2048];
        let n = s.read(&mut b).await.unwrap();
        assert!(n > 0);
        data.extend_from_slice(&b[..n]);
        if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
            let h = String::from_utf8_lossy(&data[..i]).to_ascii_lowercase();
            let len: usize = h
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if data.len() >= i + 4 + len {
                return String::from_utf8(data).unwrap();
            }
        }
    }
}
#[tokio::test]
async fn search_reaction_exact_wire_retains_late_response_without_redirect_retry_or_duplicate_json()
{
    for adding in [true, false] {
        let r = request(adding);
        assert!(valid_write(&r));
        let mut extra = r.clone();
        let RequestBody::Form(f) = &mut extra.body else {
            panic!()
        };
        f.push(("name".into(), "wave".into()));
        assert!(!valid_write(&extra));
        let mut foreign = r.clone();
        foreign.url = "https://foreign.example/api/reactions.add".into();
        assert!(!valid_write(&foreign));
        let mut header = r;
        header.headers.push(("Cookie".into(), "foreign".into()));
        assert!(!valid_write(&header));
    }
    let (w, c) = (workspace(), SlackClient::default());
    for r in [
        api::conversations_history(
            &c,
            &w,
            api::HistoryArgs {
                channel: C.into(),
                latest: Some(T.into()),
                limit: Some(1),
                inclusive: true,
                ..Default::default()
            },
        ),
        api::conversations_info(&c, &w, C.into()),
        api::reactions_get(&c, &w, C.into(), T.into()),
    ] {
        assert!(valid_read(&r));
    }
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    let mut r = request(true);
    r.url = format!("http://{a}/api/reactions.add");
    let mut checks = 0;
    assert!(matches!(
        Transport::new("fixture-cookie")
            .unwrap()
            .execute_search_reaction_once(r, C, T, "wave", true, || {
                checks += 1;
                checks == 1
            })
            .await,
        Err(SearchReactionFailure::NotRequested(_))
    ));
    assert_eq!(checks, 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), l.accept())
            .await
            .is_err()
    );
    for (status, headers, body, success) in [
        ("200 OK", "", "{\"ok\":true}", true),
        (
            "200 OK",
            "",
            "{\"ok\":false,\"error\":\"already_reacted\"}",
            true,
        ),
        ("200 OK", "", "{\"ok\":true,\"ok\":false}", false),
        ("302 Found", "Location: /api/reactions.add\r\n", "{}", false),
        ("429 Too Many Requests", "Retry-After: 2\r\n", "{}", false),
        (
            "429 Too Many Requests",
            "Retry-After: 18446744073709551615\r\n",
            "{}",
            false,
        ),
    ] {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        let current = Arc::new(AtomicBool::new(true));
        let retire = current.clone();
        let server = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let wire = consume(&mut s).await;
            assert!(
                wire.starts_with("POST /api/reactions.add ")
                    && wire.contains("channel=C01234567")
                    && wire.contains("timestamp=1700000001.000001")
                    && wire.contains("name=wave")
            );
            retire.store(false, Ordering::Release);
            s.write_all(format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            drop(s);
            assert!(
                tokio::time::timeout(Duration::from_millis(40), l.accept())
                    .await
                    .is_err()
            );
        });
        let mut r = request(true);
        r.url = format!("http://{a}/api/reactions.add");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_search_reaction_once(r, C, T, "wave", true, || current.load(Ordering::Acquire))
            .await;
        assert_eq!(result.is_ok(), success);
        if !success {
            assert!(matches!(&result, Err(SearchReactionFailure::Unknown(_))));
            if status.starts_with("429") {
                let expected = if headers.contains("18446744073709551615") {
                    u64::MAX
                } else {
                    2
                };
                assert!(
                    matches!(result, Err(SearchReactionFailure::Unknown(Error::RateLimited { retry_after_secs: Some(value) })) if value == expected)
                );
            }
        }
        assert!(!current.load(Ordering::Acquire));
        server.await.unwrap();
    }
}
