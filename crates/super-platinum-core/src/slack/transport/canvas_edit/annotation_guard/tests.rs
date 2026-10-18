use super::super::*;
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        api::{RepliesArgs, conversations_replies},
        client::SlackClient,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request() -> PreparedRequest {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let control = "temp:C:xyz0123456789012345678901234";
    let mut request = conversations_replies(
        &SlackClient::default(),
        &workspace,
        RepliesArgs {
            channel: "Cfixture".into(),
            ts: format!("Qpc:{}", control.replacen("temp:", "t:", 1)),
            latest: Some("1720000000.001".into()),
            limit: Some(28),
            inclusive: true,
            ..Default::default()
        },
    );
    let RequestBody::Form(fields) = &mut request.body else {
        panic!()
    };
    fields.push(("content_snippet".into(),serde_json::json!({"control_id":control,"is_archived":false,"mrkdwn":"Paragraph\n","rich_text":serde_json::json!({"blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Paragraph"}]}]}]}).to_string(),"section_users":[{"quip_user_id":"quip-user","slack_user_id":"U1","is_author":true,"is_recent_editor":false,"is_most_recent_editor":true,"is_mentioned":false}]}).to_string()));
    fields.push(("cached_latest_updates".into(), "{}".into()));
    request
}
#[test]
fn actual_rest_builder_query_is_accepted_but_duplicate_and_foreign_semantics_refuse() {
    let req = request();
    assert!(bootstrap_allowed(&req));
    for suffix in ["&token=foreign", "&_x_num_retries=1", "&channel=Cforeign"] {
        let mut bad = req.clone();
        bad.url.push_str(suffix);
        assert!(!bootstrap_allowed(&bad));
    }
    let mut bad = req.clone();
    bad.url = bad.url.split('?').next().unwrap().into();
    assert!(!bootstrap_allowed(&bad));
    for key in ["control_id", "rich_text", "section_users", "is_archived"] {
        let mut bad = req.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            panic!()
        };
        let (_, raw) = fields
            .iter_mut()
            .find(|(k, _)| k == "content_snippet")
            .unwrap();
        let mut value: Value = serde_json::from_str(raw).unwrap();
        value[key] = Value::Null;
        *raw = value.to_string();
        assert!(!bootstrap_allowed(&bad), "{key}");
    }
}
#[tokio::test]
async fn bootstrap_is_one_attempt_and_preserves_retry_after_after_dispatch() {
    for status in [200, 307, 429, 503] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut req = request();
        let mut url = url::Url::parse(&req.url).unwrap();
        url.set_scheme("http").unwrap();
        url.set_host(Some("127.0.0.1")).unwrap();
        url.set_port(Some(address.port())).unwrap();
        req.url = url.into();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 8192];
            let count = stream.read(&mut bytes).await.unwrap();
            assert!(
                String::from_utf8_lossy(&bytes[..count])
                    .starts_with("POST /api/conversations.replies?")
            );
            stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: 2\r\nRetry-After: 9\r\nLocation: http://{address}/api/conversations.replies\r\nConnection: close\r\n\r\n{{}}").as_bytes()).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("synthetic-cookie")
            .unwrap()
            .execute_canvas_comment_bootstrap(req)
            .await;
        match status {
            200 => assert_eq!(result.unwrap(), b"{}"),
            429 => assert!(matches!(
                result,
                Err(CanvasEditFailure::Uncertain(Error::RateLimited {
                    retry_after_secs: Some(9)
                }))
            )),
            _ => assert!(matches!(result, Err(CanvasEditFailure::Uncertain(_)))),
        }
        server.await.unwrap();
    }
}
