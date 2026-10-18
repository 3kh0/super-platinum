use super::*;
use crate::{
    block_actions::home::{
        Home, View,
        calendar::Entry,
        navigation::{Tab, Target},
    },
    config::WorkspaceSession,
};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn request(connect: bool) -> PreparedRequest {
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let client = super::super::SlackClient::default();
    if !connect {
        return Target::new("A_HOME".into(), "T1".into(), Tab::Home)
            .unwrap()
            .prepare(&client, &w)
            .unwrap();
    }
    let home = Home {
        channel: "D1".into(),
        app: "A_HOME".into(),
        installed_team: "T1".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    let view = View::decode(&home, json!({"id":"V1","type":"home","app_id":"A_HOME","app_installed_team_id":"T1","blocks":[{"type":"actions","block_id":"b","elements":[{"type":"button","action_id":"calendar_hermes_connect","text":{"type":"plain_text","text":"Connect"},"value":"{\"app_id\":\"A_HOME\",\"provider_key\":\"gcal\"}"}]}]})).unwrap();
    Entry::capture(&view, "b", "calendar_hermes_connect", "U1", "2026-10-17")
        .unwrap()
        .connect_request(&client, &w)
        .unwrap()
}

#[tokio::test]
async fn home_open_and_calendar_connect_are_once_only_and_preserve_retry_after() {
    for connect in [false, true] {
        for rate in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let endpoint = if connect {
                "apps.auth.external.start"
            } else {
                "apps.home.open"
            };
            let server = tokio::spawn(async move {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut data = vec![0; 16384];
                let n = sock.read(&mut data).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&data[..n])
                        .starts_with(&format!("POST /api/{endpoint} "))
                );
                let (status, extra, body) = if rate {
                    ("429 Too Many Requests", "Retry-After: 19\r\n", "ignored")
                } else {
                    ("200 OK", "", "{\"ok\":false,\"error\":\"dispatch_failed\"}")
                };
                sock.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",body.len()).as_bytes()).await.unwrap();
                drop(sock);
                assert!(
                    tokio::time::timeout(Duration::from_millis(350), listener.accept())
                        .await
                        .is_err()
                );
            });
            let mut req = request(connect);
            req.url = format!("http://{address}/api/{endpoint}");
            let result = Transport::new("synthetic")
                .unwrap()
                .execute_block_interaction(req)
                .await;
            if rate {
                assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(19)
                    })
                ));
            } else {
                assert_eq!(
                    result.unwrap(),
                    json!({"ok":false,"error":"dispatch_failed"})
                );
            }
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn offline_home_open_and_connect_are_known_unsent() {
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    for connect in [false, true] {
        assert!(
            matches!(transport.execute_block_interaction(request(connect)).await, Err(Error::Api(s)) if s == "block_interaction_not_sent_offline")
        );
    }
}
