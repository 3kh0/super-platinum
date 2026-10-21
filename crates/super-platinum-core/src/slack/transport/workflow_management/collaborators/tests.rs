use super::*;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn request(adding: bool) -> PreparedRequest {
    use crate::block_actions::workflow_management::{Command, EditorLoad, MutationPlan, ReadKind};
    let workspace = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let mut load = EditorLoad::new("WF1").unwrap();
    for (kind, raw) in [
        (
            ReadKind::Latest,
            json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[]}}),
        ),
        (
            ReadKind::Triggers,
            json!({"ok":true,"triggers":[{"id":"TR1"}]}),
        ),
        (
            ReadKind::TriggerInfo,
            json!({"ok":true,"trigger":{"id":"TR1","workflow":{"workflow_id":"WF1","app_id":"A1"}}}),
        ),
        (
            ReadKind::Preferences,
            json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]}}),
        ),
        (
            ReadKind::CopyPermissions,
            json!({"ok":true,"permission_type":"everyone"}),
        ),
        (
            ReadKind::RunPermissions,
            json!({"ok":true,"permission_type":"app_collaborators"}),
        ),
    ] {
        load.accept(kind, raw).unwrap();
    }
    let plan = MutationPlan::new(
        &load.editor().unwrap(),
        if adding {
            Command::AddCollaborators(vec!["U1".into(), "U2".into()])
        } else {
            Command::RemoveCollaborators(vec!["U1".into(), "U2".into()])
        },
    )
    .unwrap();
    plan.slots()[0]
        .plan()
        .request(&crate::slack::SlackClient::default(), &workspace)
}

#[test]
fn closed_routes_forms_and_original_command_factories() {
    for adding in [true, false] {
        let good = request(adding);
        assert!(allowed(&good));
        assert!(!super::super::allowed(&good));
        for url in [
            "https://fixture.slack.com.evil.test/api/functions.workflows.collaborators.add",
            "https://fixture.slack.com/api/functions.workflows.collaborators.add?workflow_id=OTHER",
            "https://fixture.slack.com/api/functions.workflows.collaborators.add?_x_num_retries=1",
            "https://fixture.slack.com/api/functions.workflows.update",
            "https://user@fixture.slack.com/api/functions.workflows.collaborators.add",
            "https://fixture.slack.com:444/api/functions.workflows.collaborators.add",
            "https://fixture.slack.com/api/functions.workflows.collaborators.add#fragment",
        ] {
            let mut bad = good.clone();
            bad.url = url.into();
            assert!(!allowed(&bad));
        }
        for (key, value) in [
            ("collaborator_ids", ""),
            ("collaborator_ids", "U1,,U2"),
            ("collaborator_ids", "[\"U1\"]"),
            ("workflow_id", "WF1,WF2"),
            ("_x_reason", "other"),
            ("token", ""),
        ] {
            let mut bad = good.clone();
            let RequestBody::Form(fields) = &mut bad.body else {
                unreachable!()
            };
            fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = value.into();
            assert!(!allowed(&bad), "{key}/{value}");
        }
        for (key, value) in [("workflow_id", "WF1"), ("unexpected", "true")] {
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
}
#[tokio::test]
async fn collaborator_receipts_are_raw_and_redirects_rate_limits_losses_never_replay() {
    for adding in [true, false] {
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
                    r#"{"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[],"collaborators":["U1","U2"]},"available_data":{"opaque":8},"expression_data":{},"opaque":{"receipt":8}}"#
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
                    stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 7\r\nLocation: http://{address}/api/functions.workflows.collaborators.add\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }
                drop(stream);
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), listener.accept())
                        .await
                        .is_err()
                );
            });
            let mut req = request(adding);
            req.url = format!(
                "http://{address}/api/functions.workflows.collaborators.{}",
                if adding { "add" } else { "remove" }
            );
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                Transport::new("synthetic")
                    .unwrap()
                    .execute_workflow_collaborators(req),
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
async fn known_offline_and_invalid_collaborator_plans_never_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    let mut req = request(true);
    req.url = format!("http://{address}/api/functions.workflows.collaborators.add");
    let mut wrong = req.clone();
    wrong.method = "GET";
    assert!(matches!(
        transport.execute_workflow_collaborators(wrong).await,
        Err(Error::Api(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport.execute_workflow_collaborators(req).await,
        Err(Error::Offline(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
