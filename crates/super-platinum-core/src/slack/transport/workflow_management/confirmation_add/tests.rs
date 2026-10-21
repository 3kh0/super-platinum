use super::*;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn request() -> PreparedRequest {
    use crate::block_actions::{
        posting_policy::Flags,
        workflow_management::confirmation_add::{Add, Bookmarks, Shortcut},
    };
    let workspace = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let shortcut=Shortcut::capture(&json!({"id":"TR1","type":"shortcut","name":" Original ","shortcut_url":"https://slack.com/shortcuts/ORIGINAL"})).unwrap();
    Add::new(
        &shortcut,
        &json!({"id":"C1","is_channel":true,"is_member":true,"fromAnotherTeam":false}),
        Flags {
            read_only: false,
            thread_only: false,
        },
        &Bookmarks::decode("C1", json!({"ok":true,"bookmarks":[]})).unwrap(),
        "T1",
        "T1",
    )
    .unwrap()
    .request(&crate::slack::SlackClient::default(), &workspace)
}
#[test]
fn exact_factory_is_closed_and_generic_metadata_transport_stays_closed() {
    let good = request();
    assert!(allowed(&good));
    assert!(!super::super::allowed(&good));
    for path in [
        "https://fixture.slack.com.evil.test/api/bookmarks.add",
        "https://fixture.slack.com/api/bookmarks.add?channel=OTHER",
        "https://fixture.slack.com/api/bookmarks.add?_x_num_retries=1",
        "https://fixture.slack.com/api/bookmarks.edit",
        "https://user@fixture.slack.com/api/bookmarks.add",
        "https://fixture.slack.com:444/api/bookmarks.add",
        "https://fixture.slack.com/api/bookmarks.add#fragment",
    ] {
        let mut bad = good.clone();
        bad.url = path.into();
        assert!(!allowed(&bad));
    }
    for (key, value) in [
        ("channel", "D1"),
        ("shortcut_id", "TR1,TR2"),
        ("type", "link"),
        ("_x_reason", "bookmarks-add-api"),
        ("token", ""),
    ] {
        let mut bad = good.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            unreachable!()
        };
        fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = value.into();
        assert!(!allowed(&bad));
    }
    for (key, value) in [("channel", "C1"), ("unexpected", "true")] {
        let mut bad = good.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            unreachable!()
        };
        fields.push((key.into(), value.into()));
        assert!(!allowed(&bad));
    }
    let mut bad = good.clone();
    bad.method = "GET";
    assert!(!allowed(&bad));
    let mut bad = good;
    bad.body = RequestBody::Json(json!({}));
    assert!(!allowed(&bad));
}
#[tokio::test]
async fn shortcut_receipts_are_raw_and_redirects_rate_limits_losses_never_replay() {
    for case in [
        "ack",
        "rejected",
        "ambiguous",
        "307",
        "308",
        "429",
        "503",
        "lost",
        "invalid-json",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let body = match case {
            "ack" => {
                r#"{"ok":true,"bookmark":{"id":"Bk1","channel_id":"C1","shortcut_id":"TR1","type":"shortcut","opaque":[1,true]},"opaque":{"receipt":8}}"#
            }
            "rejected" => r#"{"ok":false,"error":"denied"}"#,
            "ambiguous" => r#"{"ok":true,"error":"contradictory"}"#,
            "invalid-json" => "not json",
            _ => "",
        };
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            if case != "lost" {
                let status = match case {
                    "307" => "307 Temporary Redirect",
                    "308" => "308 Permanent Redirect",
                    "429" => "429 Too Many Requests",
                    "503" => "503 Service Unavailable",
                    _ => "200 OK",
                };
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 7\r\nLocation: http://{address}/api/bookmarks.add\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut req = request();
        req.url = format!("http://{address}/api/bookmarks.add");
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            Transport::new("synthetic")
                .unwrap()
                .execute_workflow_confirmation_add(req),
        )
        .await
        .unwrap();
        match case {
            "ack" | "rejected" | "ambiguous" => assert_eq!(
                result.unwrap(),
                serde_json::from_str::<Value>(body).unwrap()
            ),
            "429" => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            )),
            "lost" | "invalid-json" => assert!(matches!(result, Err(Error::Transport(_)))),
            _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn known_offline_and_invalid_shortcut_plans_never_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    let mut req = request();
    req.url = format!("http://{address}/api/bookmarks.add");
    let mut wrong = req.clone();
    wrong.method = "GET";
    assert!(matches!(
        transport.execute_workflow_confirmation_add(wrong).await,
        Err(Error::Api(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport.execute_workflow_confirmation_add(req).await,
        Err(Error::Offline(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
