use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
fn request(trigger: bool) -> PreparedRequest {
    let workspace = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let (endpoint, key, reason) = if trigger {
        (
            "workflows.triggers.delete",
            "trigger_id",
            "delete-hermes-workflow",
        )
    } else {
        (
            "functions.workflows.delete",
            "workflow_id",
            "delete-workflow",
        )
    };
    crate::slack::SlackClient::default().rest_form(
        &workspace,
        endpoint,
        vec![(key, "FIXTURE1".into()), ("_x_reason", reason.into())],
    )
}
#[test]
fn delete_admission_is_separate_exact_method_origin_body_and_reason() {
    for trigger in [false, true] {
        let good = request(trigger);
        assert!(allowed(&good));
        assert!(!super::super::allowed(&good));
        for url in [
            "https://example.test/api/functions.workflows.delete",
            "https://fixture.slack.com.evil.test/api/functions.workflows.delete",
            "http://fixture.slack.com/api/functions.workflows.delete",
            "https://fixture.slack.com:444/api/functions.workflows.delete",
            "https://user@fixture.slack.com/api/functions.workflows.delete",
            "https://fixture.slack.com/api/functions.workflows.delete#fragment",
            "https://fixture.slack.com/api/functions.workflows.update",
            "https://fixture.slack.com/api/functions.workflows.delete.extra",
        ] {
            let mut bad = good.clone();
            bad.url = url.into();
            assert!(!allowed(&bad), "{url}");
        }
        let mut bad = good.clone();
        bad.method = "GET";
        assert!(!allowed(&bad));
        bad = good.clone();
        bad.body = RequestBody::Json(serde_json::json!({"workflow_id":"FIXTURE1"}));
        assert!(!allowed(&bad));
        for (key, value) in [
            ("_x_reason", "update-hermes-workflow-metadata"),
            ("token", ""),
            (if trigger { "trigger_id" } else { "workflow_id" }, ""),
        ] {
            let mut bad = good.clone();
            let RequestBody::Form(fields) = &mut bad.body else {
                unreachable!()
            };
            fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = value.into();
            assert!(!allowed(&bad));
        }
        let mut bad = good;
        let RequestBody::Form(fields) = &mut bad.body else {
            unreachable!()
        };
        fields.push(fields[1].clone());
        assert!(!allowed(&bad));
    }
}
#[tokio::test]
async fn delete_preserves_raw_receipts_and_never_retries_or_follows_redirects() {
    for trigger in [false, true] {
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
                "ack" => r#"{"ok":true,"retained":{"receipt":7}}"#,
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
                    stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 7\r\nLocation: http://{address}/api/functions.workflows.delete\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            });
            let mut req = request(trigger);
            let endpoint = if trigger {
                "workflows.triggers.delete"
            } else {
                "functions.workflows.delete"
            };
            req.url = format!("http://{address}/api/{endpoint}");
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                Transport::new("synthetic")
                    .unwrap()
                    .execute_workflow_delete(req),
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
}
#[tokio::test]
async fn delete_known_offline_never_dispatches() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    for trigger in [false, true] {
        let mut req = request(trigger);
        req.url = format!(
            "http://{address}/api/{}",
            if trigger {
                "workflows.triggers.delete"
            } else {
                "functions.workflows.delete"
            }
        );
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute_workflow_delete(req)
            )
            .await
            .unwrap(),
            Err(Error::Offline(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
