use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/users.profile.setSections"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "synthetic".into()),
            ("user".into(), "U1".into()),
            ("section".into(), "PsHeader".into()),
            (
                "elements".into(),
                r#"[{"element_id":"PeTitle","text":{"text":"Example"}}]"#.into(),
            ),
            ("_x_reason".into(), "edit-profile-form-pane".into()),
        ]),
    }
}
#[tokio::test]
async fn profile_mutation_never_replays_or_redirects_and_keeps_ambiguity_distinct() {
    for (status, body) in [
        ("200 OK", r#"{"ok":true}"#),
        ("200 OK", r#"{"ok":false,"error":"not_allowed"}"#),
        ("200 OK", r#"{"ok":false}"#),
        ("200 OK", r#"{"ok":true,"error":"bad"}"#),
        ("429 Too Many Requests", ""),
        ("503 Service Unavailable", ""),
        ("302 Found", ""),
        ("lost", ""),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            if status != "lost" {
                stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{address}/api/users.profile.setSections\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let transport = Transport::new("synthetic").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            transport.execute_own_profile_sections(request(address)),
        )
        .await
        .unwrap();
        match (status, body) {
            ("200 OK", r#"{"ok":true}"#) => assert!(result.is_ok()),
            ("200 OK", r#"{"ok":false,"error":"not_allowed"}"#) => {
                assert!(matches!(result,Err(Error::Api(code)) if code=="not_allowed"))
            }
            ("200 OK", _) | ("lost", _) => assert!(matches!(result, Err(Error::Transport(_)))),
            ("429 Too Many Requests", _) => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            )),
            _ => assert!(matches!(result, Err(Error::HttpStatus { .. }))),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn offline_and_nonself_payloads_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_millis(100),
            transport.execute_own_profile_sections(request(address))
        )
        .await
        .unwrap(),
        Err(Error::Offline(_))
    ));
    transport.set_health(Health::Online);
    for extra in ["user", "status_text", "title"] {
        let mut req = request(address);
        let RequestBody::Form(fields) = &mut req.body else {
            unreachable!()
        };
        fields.push((extra.into(), "reject".into()));
        assert!(transport.execute_own_profile_sections(req).await.is_err());
    }
    for url in [
        "https://evil.invalid/api/users.profile.setSections",
        "https://slack.com.evil.invalid/api/users.profile.setSections",
        "https://fixture.slack.com:444/api/users.profile.setSections",
        "https://user@fixture.slack.com/api/users.profile.setSections",
        "https://fixture.slack.com/api/users.profile.setSections#fragment",
        "https://fixture.slack.com/api/users.profile.delete",
    ] {
        let mut req = request(address);
        req.url = url.into();
        assert!(!allowed(&req));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn production_builder_metadata_is_admitted_without_query_target_overrides() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
    };
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let response = serde_json::json!({"ok":true,"result":{"data":{"user":{"id":"U1","profileSections":[{
        "sectionId":"PsHeader","type":"HEADER","profileElements":[{"elementId":"PeTitle","elementKey":"title",
        "__typename":"ProfileTextElement","text":"Original","permissions":{"ui":true}}]}]}}}});
    let snapshot = api::OwnProfileSections::from_response(&workspace, &response).unwrap();
    let intent = api::OwnProfileTitleIntent::capture(&snapshot, "New").unwrap();
    let request =
        api::own_profile_title_request(&SlackClient::default(), &workspace, &intent, &snapshot)
            .unwrap();
    assert!(url::Url::parse(&request.url).unwrap().query().is_some());
    assert!(allowed(&request));
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_own_profile_sections(request.clone())
            .await,
        Err(Error::Offline(_))
    ));
    for override_key in ["user", "section", "token", "elements"] {
        let mut bad = request.clone();
        bad.url.push_str(&format!("&{override_key}=different"));
        assert!(!allowed(&bad));
    }
}
