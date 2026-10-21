use super::*;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn request(mode: &str, selected: bool) -> PreparedRequest {
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
        Command::RunPermission {
            permission_type: mode.into(),
            user_ids: selected.then(|| vec!["U1".into(), "U2".into()]),
            channel_ids: selected.then(|| vec!["C1".into()]),
            team_ids: selected.then(|| vec![]),
            org_ids: selected.then(|| vec!["E1".into()]),
        },
    )
    .unwrap();
    plan.slots()[0]
        .plan()
        .request(&crate::slack::SlackClient::default(), &workspace)
}

#[test]
fn exact_run_plan_accepts_source_modes_and_csv_including_present_empty_arrays() {
    for (mode, selected) in [
        ("everyone", false),
        ("app_collaborators", false),
        ("named_entities", true),
        ("everyone", true),
    ] {
        let good = request(mode, selected);
        assert!(allowed(&good), "{mode}/{selected}");
        assert!(!super::super::allowed(&good));
    }
    let RequestBody::Form(fields) = request("named_entities", true).body else {
        unreachable!()
    };
    assert!(fields.contains(&("user_ids".into(), "U1,U2".into())));
    assert!(fields.contains(&("team_ids".into(), "".into())));
}

#[test]
fn run_admission_refuses_other_operations_duplicate_or_hidden_arguments() {
    let good = request("named_entities", true);
    for url in [
        "https://example.test/api/workflows.triggers.permissions.set",
        "https://fixture.slack.com.evil.test/api/workflows.triggers.permissions.set",
        "http://fixture.slack.com/api/workflows.triggers.permissions.set",
        "https://fixture.slack.com:444/api/workflows.triggers.permissions.set",
        "https://user@fixture.slack.com/api/workflows.triggers.permissions.set",
        "https://fixture.slack.com/api/workflows.triggers.permissions.set#fragment",
        "https://fixture.slack.com/api/workflows.triggers.permissions.set.extra",
        "https://fixture.slack.com/api/functions.workflows.permissions.copy.set",
        "https://fixture.slack.com/api/workflows.triggers.permissions.set?token=synthetic",
        "https://fixture.slack.com/api/workflows.triggers.permissions.set?trigger_id=OTHER",
        "https://fixture.slack.com/api/workflows.triggers.permissions.set?_x_reason=other",
        "https://fixture.slack.com/api/workflows.triggers.permissions.set?_x_num_retries=1",
        "https://fixture.slack.com/api/workflows.triggers.permissions.set?_x_id=one&_x_id=two",
    ] {
        let mut bad = good.clone();
        bad.url = url.into();
        assert!(!allowed(&bad), "{url}");
    }
    for (key, value) in [
        ("token", ""),
        ("trigger_id", ""),
        ("trigger_id", "TR1,TR2"),
        ("_x_reason", "load-workflow-editor"),
        ("permission_type", "collaborators"),
        ("user_ids", "U1,,U2"),
        ("user_ids", "[\"U1\"]"),
        ("channel_ids", "C1 C2"),
        ("org_ids", "E1&other=value"),
    ] {
        let mut bad = good.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            unreachable!()
        };
        fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = value.into();
        assert!(!allowed(&bad), "{key}");
    }
    for field in [
        ("token", "second"),
        ("workflow_id", "WF1"),
        ("unexpected", "true"),
    ] {
        let mut bad = good.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            unreachable!()
        };
        fields.push((field.0.into(), field.1.into()));
        assert!(!allowed(&bad));
    }
    let mut bad = request("everyone", false);
    let RequestBody::Form(fields) = &mut bad.body else {
        unreachable!()
    };
    fields.push(("user_ids".into(), "U1".into()));
    assert!(!allowed(&bad));
    let mut bad = good.clone();
    bad.method = "GET";
    assert!(!allowed(&bad));
    let mut bad = good;
    bad.body = RequestBody::Json(json!({"trigger_id":"TR1"}));
    assert!(!allowed(&bad));
}

#[tokio::test]
async fn run_receipts_are_raw_and_redirects_rate_limits_losses_never_replay() {
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
            "ack" => r#"{"ok":true,"permission_type":"named_entities","opaque":{"receipt":8}}"#,
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
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 7\r\nLocation: http://{address}/api/workflows.triggers.permissions.set\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut req = request("named_entities", true);
        req.url = format!("http://{address}/api/workflows.triggers.permissions.set");
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            Transport::new("synthetic")
                .unwrap()
                .execute_workflow_run_permission(req),
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
async fn known_offline_and_invalid_run_plans_never_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    let mut req = request("everyone", false);
    req.url = format!("http://{address}/api/workflows.triggers.permissions.set");
    let mut wrong = req.clone();
    wrong.method = "GET";
    assert!(matches!(
        transport.execute_workflow_run_permission(wrong).await,
        Err(Error::Api(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport.execute_workflow_run_permission(req).await,
        Err(Error::Offline(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
