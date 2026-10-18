use super::*;
use crate::{block_actions::shortcuts::calendar as cal, config::WorkspaceSession};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn event() -> PreparedRequest {
    cal::create(
        &super::super::SlackClient::default(),
        &session(),
        cal::Provider::Google,
        &cal::Account::Modern,
        &cal::Event {
            start: 1792332000,
            end: 1792333800,
            title: "Fixture".into(),
            description: String::new(),
            attendees: vec![],
            calendar: String::new(),
            meeting_url: String::new(),
            share_channel: String::new(),
            share_thread: String::new(),
        },
    )
    .unwrap()
}
#[tokio::test]
async fn actual_calendar_mutations_send_once_and_preserve_429_or_ambiguous_reply() {
    for room in [false, true] {
        for rate in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut data = vec![0; 16384];
                let n = sock.read(&mut data).await.unwrap();
                assert!(n > 0);
                let text = String::from_utf8_lossy(&data[..n]);
                assert!(text.starts_with(if room {
                    "POST /api/rooms.request"
                } else {
                    "POST /api/calendar.event.create"
                }));
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
            let mut req = if room {
                cal::room(
                    &super::super::SlackClient::default(),
                    &session(),
                    "A1",
                    "C1",
                )
                .unwrap()
            } else {
                event()
            };
            req.url = format!(
                "http://{address}/api/{}",
                if room {
                    "rooms.request"
                } else {
                    "calendar.event.create"
                }
            );
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
                    cal::event_result(&result.unwrap()),
                    cal::EventResult::Unknown
                );
            }
            server.await.unwrap();
        }
    }
}
#[tokio::test]
async fn offline_calendar_write_is_known_unsent_and_preference_authority_is_closed() {
    let t = Transport::new("synthetic").unwrap();
    t.set_health(Health::Offline);
    assert!(
        matches!(t.execute_block_interaction(event()).await,Err(Error::Api(s)) if s=="block_interaction_not_sent_offline")
    );
    let c = super::super::SlackClient::default();
    let good = cal::set_share_preference(&c, &session(), true);
    assert!(is_block_interaction_request(&good));
    for variant in 0..4 {
        let mut req = good.clone();
        let RequestBody::Form(fields) = &mut req.body else {
            panic!()
        };
        match variant {
            0 => fields.push(("other".into(), "true".into())),
            1 => fields.iter_mut().find(|(k, _)| k == "name").unwrap().1 = "mute_sounds".into(),
            2 => fields.iter_mut().find(|(k, _)| k == "value").unwrap().1 = "1".into(),
            _ => fields.push(("name".into(), "calendar_share_event_in_channel".into())),
        };
        assert!(!is_block_interaction_request(&req));
    }
}
