use super::*;
use crate::slack::api::{self, AssigneeGrant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    let w = crate::config::WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let mut r = api::list_assignee_share(
        &crate::slack::SlackClient::default(),
        &w,
        "F00000001",
        "D00000002",
        AssigneeGrant::Write,
        None,
    );
    r.url = format!("http://{address}/api/files.share");
    r
}
#[tokio::test]
async fn assignee_share_refuses_retired_owner_offline_and_broadened_grants_without_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let t = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        t.execute_list_assignee(request(address), || false).await,
        Err(ListAssigneeFailure::NotRequested(_))
    ));
    t.set_health(Health::Offline);
    assert!(matches!(
        t.execute_list_assignee(request(address), || true).await,
        Err(ListAssigneeFailure::NotRequested(Error::Offline(_)))
    ));
    t.set_health(Health::Online);
    for case in 0..5 {
        let mut r = request(address);
        let RequestBody::Form(f) = &mut r.body else {
            panic!()
        };
        match case {
            0 => f[4].1 = r#"[{"file_id":"F00000001","grant":"owner"}]"#.into(),
            1 => f[4].1 = r#"[{"file_id":"F00000002","grant":"write"}]"#.into(),
            2 => f[5].1 = "true".into(),
            3 => f.push(("silent_share".into(), "true".into())),
            _ => f[2].1 = "U00000002".into(),
        }
        assert!(matches!(
            t.execute_list_assignee(r, || true).await,
            Err(ListAssigneeFailure::NotRequested(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn permission_share_uses_actual_form_once_and_preserves_rate_limit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0; 2048];
        loop {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let size = header
                    .lines()
                    .find_map(|s| {
                        s.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + size {
                    let f: std::collections::BTreeMap<String, String> =
                        url::form_urlencoded::parse(&bytes[end + 4..end + 4 + size])
                            .into_owned()
                            .collect();
                    assert_eq!(f["files"], "F00000001");
                    assert_eq!(f["channel"], "D00000002");
                    assert_eq!(f["from_share_modal"], "true");
                    assert_eq!(
                        serde_json::from_str::<Value>(&f["permissions"]).unwrap(),
                        serde_json::json!([{"file_id":"F00000001","grant":"write"}])
                    );
                    break;
                }
            }
        }
        stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 11\r\nContent-Type: application/json\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let t = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        t.execute_list_assignee(request(address), || true).await,
        Err(ListAssigneeFailure::Unconfirmed(Error::RateLimited {
            retry_after_secs: Some(7)
        }))
    ));
    server.await.unwrap();
}
