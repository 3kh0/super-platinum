use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
fn request() -> PreparedRequest {
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
            json!({"ok":true,"decorated_workflow":{"id":"FIXTURE1","app_id":"A1","steps":[],"is_published":false}}),
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
    let plan = MutationPlan::new(&load.editor().unwrap(), Command::Published(true)).unwrap();
    assert_eq!(plan.slots().len(), 1);
    assert_eq!(plan.slots()[0].prerequisite(), None);
    plan.slots()[0]
        .plan()
        .request(&crate::slack::SlackClient::default(), &workspace)
}
#[test]
fn publish_original_plan_has_exact_primary_wire() {
    let req = request();
    assert_eq!(req.method, "POST");
    let url = url::Url::parse(&req.url).unwrap();
    assert_eq!(url.path(), "/api/functions.workflows.publish");
    assert!(metadata_query(&url));
    let RequestBody::Form(mut fields) = req.body else {
        panic!("source plan must use form fields");
    };
    fields.sort();
    assert_eq!(
        fields,
        vec![
            ("_x_reason".into(), "publish-workflow".into()),
            ("token".into(), "synthetic".into()),
            ("workflow_id".into(), "FIXTURE1".into()),
        ]
    );
}
#[test]
fn publish_admission_is_separate_exact_method_origin_body_and_reason() {
    let good = request();
    assert!(allowed(&good));
    assert!(!super::super::allowed(&good));
    for url in [
        "https://example.test/api/functions.workflows.publish",
        "https://fixture.slack.com.evil.test/api/functions.workflows.publish",
        "http://fixture.slack.com/api/functions.workflows.publish",
        "https://fixture.slack.com:444/api/functions.workflows.publish",
        "https://user@fixture.slack.com/api/functions.workflows.publish",
        "https://fixture.slack.com/api/functions.workflows.publish#fragment",
        "https://fixture.slack.com/api/functions.workflows.update",
        "https://fixture.slack.com/api/functions.workflows.publish.extra",
        "https://fixture.slack.com/api/functions.workflows.unpublish",
        "https://fixture.slack.com/api/functions.workflows.publish?token=synthetic",
        "https://fixture.slack.com/api/functions.workflows.publish?workflow_id=OTHER",
        "https://fixture.slack.com/api/functions.workflows.publish?_x_reason=publish-workflow",
        "https://fixture.slack.com/api/functions.workflows.publish?_x_num_retries=1",
        "https://fixture.slack.com/api/functions.workflows.publish?_x_id=one&_x_id=two",
        "https://fixture.slack.com/api/functions.workflows.publish?_x_id=",
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
#[tokio::test]
async fn publish_preserves_raw_receipts_and_never_retries_or_follows_redirects() {
    for case in [
        "ack",
        "rejected",
        "ambiguous",
        "307",
        "308",
        "429",
        "503",
        "403",
        "lost",
        "invalid-json",
        "non-object",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let body = match case {
            "ack" => r#"{"ok":true,"retained":{"receipt":7}}"#,
            "rejected" => r#"{"ok":false,"error":"denied"}"#,
            "ambiguous" => r#"{"ok":true,"error":"contradictory"}"#,
            "invalid-json" => "not json",
            "non-object" => "[]",
            _ => "",
        };
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let body_start = loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0, "request ended before headers");
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8(bytes[..body_start].to_vec()).unwrap();
            assert!(headers.starts_with("POST /api/functions.workflows.publish HTTP/1.1\r\n"));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < body_start + length {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0, "request ended before body");
                bytes.extend_from_slice(&chunk[..count]);
            }
            let mut fields: Vec<_> =
                url::form_urlencoded::parse(&bytes[body_start..body_start + length])
                    .into_owned()
                    .collect();
            fields.sort();
            assert_eq!(
                fields,
                vec![
                    ("_x_reason".into(), "publish-workflow".into()),
                    ("token".into(), "synthetic".into()),
                    ("workflow_id".into(), "FIXTURE1".into()),
                ]
            );
            if case != "lost" {
                let status = match case {
                    "307" => "307 Temporary Redirect",
                    "308" => "308 Permanent Redirect",
                    "429" => "429 Too Many Requests",
                    "503" => "503 Service Unavailable",
                    "403" => "403 Forbidden",
                    _ => "200 OK",
                };
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nRetry-After: 7\r\nLocation: http://{address}/api/functions.workflows.publish\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut req = request();
        let endpoint = "functions.workflows.publish";
        req.url = format!("http://{address}/api/{endpoint}");
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            Transport::new("synthetic")
                .unwrap()
                .execute_workflow_publish(req),
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
            "lost" | "invalid-json" | "non-object" => {
                assert!(matches!(result, Err(Error::Transport(_))))
            }
            _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn publish_malformed_arguments_never_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    for variant in 0..7 {
        let mut req = request();
        req.url = format!("http://{address}/api/functions.workflows.publish");
        let RequestBody::Form(fields) = &mut req.body else {
            unreachable!()
        };
        match variant {
            0 => {
                fields.retain(|(key, _)| key != "workflow_id");
            }
            1 => fields.push(("workflow_id".into(), "OTHER".into())),
            2 => fields.push(("published".into(), "true".into())),
            3 => {
                fields
                    .iter_mut()
                    .find(|(key, _)| key == "_x_reason")
                    .unwrap()
                    .1 = "unpublish-workflow".into()
            }
            4 => fields
                .iter_mut()
                .find(|(key, _)| key == "token")
                .unwrap()
                .1
                .clear(),
            5 => req.url.push_str("?workflow_id=OTHER"),
            6 => req.url = format!("http://{address}/api/functions.workflows.unpublish"),
            _ => unreachable!(),
        }
        let result = transport.execute_workflow_publish(req).await;
        assert!(
            matches!(result, Err(Error::Api(ref error)) if error == "workflow_management_not_sent_publish_method")
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn publish_known_offline_never_dispatches() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    {
        let mut req = request();
        req.url = format!("http://{address}/api/{}", "functions.workflows.publish");
        assert!(matches!(
            tokio::time::timeout(
                Duration::from_millis(100),
                transport.execute_workflow_publish(req)
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
