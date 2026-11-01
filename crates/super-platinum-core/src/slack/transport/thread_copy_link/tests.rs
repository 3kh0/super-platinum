use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
};
use std::sync::atomic::AtomicBool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CHANNEL: &str = "C01234567";
const TS: &str = "1700000001.000001";

fn request(kind: ThreadCopyRead) -> PreparedRequest {
    let workspace = WorkspaceSession {
        team_id: "T01234567".into(),
        user_id: "U01234567".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let client = SlackClient::default();
    match kind {
        ThreadCopyRead::History => api::conversations_history(
            &client,
            &workspace,
            api::HistoryArgs {
                channel: CHANNEL.into(),
                latest: Some(TS.into()),
                inclusive: true,
                limit: Some(1),
                ..Default::default()
            },
        ),
        ThreadCopyRead::Channel => api::conversations_info(&client, &workspace, CHANNEL.into()),
    }
}

async fn consume(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 2048];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..i]).to_ascii_lowercase();
            let length: usize = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= i + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}

#[tokio::test]
async fn thread_copy_exact_reads_bound_dispatch_and_retain_original_late_raw_response() {
    for kind in [ThreadCopyRead::History, ThreadCopyRead::Channel] {
        let original = request(kind);
        assert!(valid_read(&original));
        for (key, value) in [("channel", CHANNEL), ("cursor", "next"), ("token", "other")] {
            let mut duplicate = original.clone();
            let RequestBody::Form(fields) = &mut duplicate.body else {
                panic!()
            };
            fields.push((key.into(), value.into()));
            assert!(!valid_read(&duplicate));
        }
        for url in [
            "https://foreign.example/api/conversations.history",
            "https://fixture.slack.com/api/reactions.get",
            "https://fixture.slack.com/api/conversations.history?_x_id=1&_x_id=2",
            "https://fixture.slack.com/api/conversations.history?_x_num_retries=1",
            "https://fixture.slack.com/api/conversations.history?cursor=x",
            "https://u@fixture.slack.com/api/conversations.history",
            "https://@fixture.slack.com/api/conversations.history",
            "https://fixture.slack.com/api/other/../conversations.history",
            "https://fixture.slack.com/api/conversations.history#x",
        ] {
            let mut malformed = original.clone();
            malformed.url = url.into();
            assert!(!valid_read(&malformed));
        }
        let mut header = original.clone();
        header.headers.push(("Cookie".into(), "other".into()));
        assert!(!valid_read(&header));
        let mut method = original.clone();
        method.method = "GET".into();
        assert!(!valid_read(&method));
        let mut json = original.clone();
        json.body = RequestBody::Json(serde_json::json!({}));
        assert!(!valid_read(&json));
        let mut token = original.clone();
        let RequestBody::Form(fields) = &mut token.body else {
            panic!()
        };
        fields.iter_mut().find(|(key, _)| key == "token").unwrap().1 = " ".into();
        assert!(!valid_read(&token));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut local = original;
        let path = match kind {
            ThreadCopyRead::History => "history",
            ThreadCopyRead::Channel => "info",
        };
        local.url = format!(
            "http://{}/api/conversations.{path}",
            listener.local_addr().unwrap()
        );
        let transport = Transport::new("fixture-cookie").unwrap();
        let mut checks = 0;
        assert!(matches!(
            transport
                .execute_thread_copy_read(local.clone(), kind, CHANNEL, TS, || {
                    checks += 1;
                    checks == 1
                })
                .await,
            Err(ThreadCopyReadFailure::NotRequested(_))
        ));
        assert_eq!(checks, 2);
        assert!(matches!(
            transport
                .execute_thread_copy_read(local.clone(), kind, "CFOREIGN", TS, || true)
                .await,
            Err(ThreadCopyReadFailure::NotRequested(_))
        ));
        assert!(matches!(
            transport
                .execute_thread_copy_read(local, kind, CHANNEL, "1700000001.1", || true)
                .await,
            Err(ThreadCopyReadFailure::NotRequested(_))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(40), listener.accept())
                .await
                .is_err()
        );
    }
    for (kind, status, headers, body, success) in [
        (
            ThreadCopyRead::History,
            "200 OK",
            "",
            "{\"ok\":true,\"messages\":[]}",
            true,
        ),
        (
            ThreadCopyRead::Channel,
            "200 OK",
            "",
            "{\"ok\":false,\"error\":\"not_in_channel\"}",
            true,
        ),
        (
            ThreadCopyRead::History,
            "200 OK",
            "",
            "{\"ok\":true,\"message\":{\"ts\":1,\"ts\":2}}",
            false,
        ),
        (ThreadCopyRead::Channel, "200 OK", "", "[]", false),
        (
            ThreadCopyRead::Channel,
            "302 Found",
            "Location: /api/conversations.info\r\n",
            "{}",
            false,
        ),
        (
            ThreadCopyRead::History,
            "429 Too Many Requests",
            "Retry-After: 2\r\n",
            "{}",
            false,
        ),
        (
            ThreadCopyRead::Channel,
            "429 Too Many Requests",
            "Retry-After: 18446744073709551615\r\n",
            "{}",
            false,
        ),
        (
            ThreadCopyRead::History,
            "200 OK",
            "Retry-After: 3\r\n",
            "{}",
            false,
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let current = Arc::new(AtomicBool::new(true));
        let retire = current.clone();
        let path = match kind {
            ThreadCopyRead::History => "history",
            ThreadCopyRead::Channel => "info",
        };
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let wire = consume(&mut stream).await;
            assert!(wire.starts_with(&format!("POST /api/conversations.{path} ")));
            assert!(wire.contains("channel=C01234567") && wire.contains("token=fixture-only"));
            if matches!(kind, ThreadCopyRead::History) {
                assert!(
                    wire.contains("latest=1700000001.000001")
                        && wire.contains("limit=1")
                        && wire.contains("inclusive=true")
                );
            } else {
                assert!(wire.contains("include_num_members=false"));
            }
            retire.store(false, Ordering::Release);
            stream.write_all(format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(40), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut local = request(kind);
        local.url = format!("http://{address}/api/conversations.{path}");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_thread_copy_read(local, kind, CHANNEL, TS, || current.load(Ordering::Acquire))
            .await;
        assert_eq!(result.is_ok(), success);
        if success {
            assert_eq!(
                result.unwrap(),
                serde_json::from_str::<Value>(body).unwrap()
            );
        } else if status.starts_with("429") {
            let expected = if headers.contains("18446744073709551615") {
                u64::MAX
            } else {
                2
            };
            assert!(
                matches!(result, Err(ThreadCopyReadFailure::Failed(Error::RateLimited { retry_after_secs: Some(seconds) })) if seconds == expected)
            );
        } else if headers.starts_with("Retry-After") {
            assert!(matches!(
                result,
                Err(ThreadCopyReadFailure::Failed(Error::HttpStatus {
                    status: 200,
                    retry_after_secs: Some(3)
                }))
            ));
        } else {
            assert!(matches!(result, Err(ThreadCopyReadFailure::Failed(_))));
        }
        assert!(!current.load(Ordering::Acquire));
        server.await.unwrap();
    }
}
