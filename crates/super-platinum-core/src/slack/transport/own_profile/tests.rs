use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/users.profile.set"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "synthetic".into()),
            ("profile".into(), r#"{"display_name":"Example"}"#.into()),
            ("reason".into(), "EditProfileFormPane".into()),
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
                stream.write_all(format!("HTTP/1.1 {status}\r\nRetry-After: 7\r\nLocation: http://{address}/api/users.profile.set\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
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
            transport.execute_own_profile_text(request(address)),
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
            transport.execute_own_profile_text(request(address))
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
        assert!(transport.execute_own_profile_text(req).await.is_err());
    }
    for url in [
        "https://evil.invalid/api/users.profile.set",
        "https://slack.com.evil.invalid/api/users.profile.set",
        "https://fixture.slack.com:444/api/users.profile.set",
        "https://user@fixture.slack.com/api/users.profile.set",
        "https://fixture.slack.com/api/users.profile.set#fragment",
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
async fn production_profile_builder_accepts_metadata_but_rejects_query_user_override() {
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
    let request = api::own_profile_text_request(
        &SlackClient::default(),
        &workspace,
        serde_json::json!({"display_name":"Example"}),
    )
    .unwrap();
    assert!(url::Url::parse(&request.url).unwrap().query().is_some());
    assert!(allowed(&request));
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport.execute_own_profile_text(request.clone()).await,
        Err(Error::Offline(_))
    ));
    for override_key in ["user", "token", "profile", "status_text"] {
        let mut bad = request.clone();
        bad.url.push_str(&format!("&{override_key}=different"));
        assert!(!allowed(&bad));
    }
}
