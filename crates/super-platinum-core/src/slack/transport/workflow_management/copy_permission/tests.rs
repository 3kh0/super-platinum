use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
fn request(_trigger: bool) -> PreparedRequest {
    use crate::block_actions::workflow_management::{Command, EditorLoad, MutationPlan, ReadKind};
    use serde_json::json;
    let workspace = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let mut load = EditorLoad::new("FIXTURE1").unwrap();
    for (kind, raw) in [
        (
            ReadKind::Latest,
            json!({"ok":true,"decorated_workflow":{"id":"FIXTURE1","app_id":"A1","steps":[],"is_published":true}}),
        ),
        (ReadKind::Triggers, json!({"ok":true,"triggers":[]})),
        (
            ReadKind::Preferences,
            json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]}}),
        ),
        (
            ReadKind::CopyPermissions,
            json!({"ok":true,"permission_type":"everyone"}),
        ),
    ] {
        load.accept(kind, raw).unwrap();
    }
    let plan = MutationPlan::new(
        &load.editor().unwrap(),
        Command::CopyPermission("collaborators".into()),
    )
    .unwrap();
    plan.slots()[0]
        .plan()
        .request(&crate::slack::SlackClient::default(), &workspace)
}
#[test]
fn copy_permission_admission_is_separate_exact_method_origin_body_and_reason() {
    for trigger in [false] {
        let good = request(trigger);
        assert!(allowed(&good));
        assert!(!super::super::allowed(&good));
        for url in [
            "https://example.test/api/functions.workflows.permissions.copy.set",
            "https://fixture.slack.com.evil.test/api/functions.workflows.permissions.copy.set",
            "http://fixture.slack.com/api/functions.workflows.permissions.copy.set",
            "https://fixture.slack.com:444/api/functions.workflows.permissions.copy.set",
            "https://user@fixture.slack.com/api/functions.workflows.permissions.copy.set",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set#fragment",
            "https://fixture.slack.com/api/functions.workflows.update",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set.extra",
            "https://fixture.slack.com/api/functions.workflows.publish",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set?token=synthetic",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set?workflow_id=OTHER",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set?_x_reason=publish-workflow",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set?_x_num_retries=1",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set?_x_id=one&_x_id=two",
            "https://fixture.slack.com/api/functions.workflows.permissions.copy.set?_x_id=",
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
            ("workflow_id", ""),
            ("permission_type", "named_entities"),
            ("permission_type", ""),
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
async fn copy_permission_preserves_raw_receipts_and_never_retries_or_follows_redirects() {
    for trigger in [false] {
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
                    stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 7\r\nLocation: http://{address}/api/functions.workflows.permissions.copy.set\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            });
            let mut req = request(trigger);
            let endpoint = "functions.workflows.permissions.copy.set";
            req.url = format!("http://{address}/api/{endpoint}");
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                Transport::new("synthetic")
                    .unwrap()
                    .execute_workflow_copy_permission(req),
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
async fn copy_permission_known_offline_never_dispatches() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    for trigger in [false] {
        let mut req = request(trigger);
        req.url = format!(
            "http://{address}/api/{}",
            "functions.workflows.permissions.copy.set"
        );
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute_workflow_copy_permission(req)
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
