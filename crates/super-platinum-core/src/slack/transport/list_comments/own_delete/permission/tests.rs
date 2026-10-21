use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn query() -> OwnDeletePermissionQuery {
    OwnDeletePermissionQuery::capture("T1", "U1", "C01234567", false).unwrap()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn request() -> PreparedRequest {
    api::list_comment_delete_permissions(&SlackClient::default(), &workspace(), &query()).unwrap()
}
fn grant() -> Value {
    json!({"ok":true,"allowed":["DELETE_OWN_MESSAGE"],"opaque":{"original":true}})
}
#[test]
fn exact_edge_factory_is_query_and_owner_bound() {
    let r = request();
    assert!(allowed(&r));
    assert!(matches_query(&r, &query()));
    assert_eq!(
        url::Url::parse(&r.url).unwrap().path(),
        "/cache/T1/permissions/info"
    );
    assert_eq!(
        r.body,
        RequestBody::Json(
            json!({"token":"fixture-only","user_id":"U1","channel_id":"C01234567","permissions":["DELETE_OWN_MESSAGE"]})
        )
    );
    for (team, user) in [("TOTHER", "U1"), ("T1", "UOTHER")] {
        let mut owner = workspace();
        owner.team_id = team.into();
        owner.user_id = user.into();
        assert!(
            api::list_comment_delete_permissions(&SlackClient::default(), &owner, &query())
                .is_err()
        );
    }
    let admin = OwnDeletePermissionQuery::capture("T1", "U1", "C01234567", true).unwrap();
    let request =
        api::list_comment_delete_permissions(&SlackClient::default(), &workspace(), &admin)
            .unwrap();
    assert!(matches_query(&request, &admin));
    assert!(!matches_query(&request, &query()));
}
#[test]
fn permission_query_rejects_foreign_scope_user_channel_permission_admin_and_form() {
    for url in [
        "https://foreign.example/cache/T1/permissions/info",
        "https://edgeapi.slack.com/cache/TOTHER/permissions/info",
        "https://edgeapi.slack.com/cache/T1/permissions/other",
        "https://user@edgeapi.slack.com/cache/T1/permissions/info",
        "https://edgeapi.slack.com/cache/T1/permissions/info?unexpected=1",
        "https://edgeapi.slack.com/cache/T1/permissions/info#fragment",
    ] {
        let mut r = request();
        r.url = url.into();
        assert!(!matches_query(&r, &query()), "{url}");
    }
    for (key, value) in [
        ("user_id", json!("UOTHER")),
        ("channel_id", json!("C76543210")),
        ("permissions", json!(["DELETE_ANY_MESSAGE"])),
        (
            "permissions",
            json!(["DELETE_OWN_MESSAGE", "DELETE_ANY_MESSAGE"]),
        ),
        ("as_admin", json!(true)),
        ("as_admin", json!(false)),
        ("admin", json!(true)),
        ("enterprise_id", json!("EOTHER")),
        ("enterprise_token", json!("other-fixture")),
        ("channel", json!("C76543210")),
        ("token", json!("")),
    ] {
        let mut r = request();
        let RequestBody::Json(raw) = &mut r.body else {
            panic!()
        };
        raw[key] = value;
        assert!(!matches_query(&r, &query()), "{key}");
    }
    let mut r = request();
    r.body = RequestBody::Form(vec![("token".into(), "fixture-only".into())]);
    assert!(!allowed(&r));
    r = request();
    r.method = "GET";
    assert!(!allowed(&r));
}
#[test]
fn enterprise_scope_cannot_be_promoted_removed_or_substituted_by_request_fields() {
    let client = SlackClient::default();
    let enterprise = query().with_enterprise_scope(Some("E1")).unwrap();
    let mut owner = workspace();
    owner.enterprise_id = Some("E1".into());
    let r = api::list_comment_delete_permissions(&client, &owner, &enterprise).unwrap();
    assert!(allowed(&r));
    assert!(matches_query(&r, &enterprise));
    assert_eq!(
        url::Url::parse(&r.url).unwrap().path(),
        "/cache/E1/permissions/info"
    );
    assert!(
        !matches_query(&r, &query()),
        "ordinary query must not acquire enterprise scope"
    );
    assert!(
        !matches_query(&request(), &enterprise),
        "enterprise query must not lose enterprise scope"
    );
    assert!(api::list_comment_delete_permissions(&client, &owner, &query()).is_err());
    assert!(api::list_comment_delete_permissions(&client, &workspace(), &enterprise).is_err());
    owner.enterprise_id = Some("EOTHER".into());
    assert!(api::list_comment_delete_permissions(&client, &owner, &enterprise).is_err());
    let mut changed = r.clone();
    changed.url = changed.url.replace("/cache/E1/", "/cache/EOTHER/");
    if let RequestBody::Json(raw) = &mut changed.body {
        raw["enterprise_id"] = json!("EOTHER");
    }
    assert!(!matches_query(&changed, &enterprise));
    let mut promoted = request();
    promoted.url = promoted.url.replace("/cache/T1/", "/cache/EOTHER/");
    if let RequestBody::Json(raw) = &mut promoted.body {
        raw["enterprise_id"] = json!("EOTHER");
        raw["enterprise_token"] = json!("fixture-only");
    }
    assert!(!matches_query(&promoted, &query()));
}
async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
    let mut data = Vec::new();
    loop {
        let mut buf = [0u8; 2048];
        let n = stream.read(&mut buf).await.unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8(data[..i].to_vec()).unwrap();
            let size: usize = headers
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(str::trim)
                        .map(str::to_owned)
                })
                .unwrap()
                .parse()
                .unwrap();
            if data.len() >= i + 4 + size {
                return (headers, data[i + 4..i + 4 + size].to_vec());
            }
        }
    }
}
#[tokio::test]
async fn permission_posts_actual_json_once_and_preserves_denial_and_raw_grant() {
    for raw in [
        grant(),
        json!({"ok":true,"allowed":[]}),
        json!({"ok":true,"allowed":["DELETE_ANY_MESSAGE"]}),
        json!({"ok":true}),
        json!({"ok":false,"allowed":["DELETE_OWN_MESSAGE"],"error":"internal_error"}),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let bytes = serde_json::to_vec(&raw).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (headers, body) = read_request(&mut stream).await;
            assert!(headers.starts_with("POST /cache/T1/permissions/info HTTP/1.1"));
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-type: application/json")
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                json!({"token":"fixture-only","user_id":"U1","channel_id":"C01234567","permissions":["DELETE_OWN_MESSAGE"]})
            );
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            stream.write_all(&bytes).await.unwrap();
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(60), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut r = request();
        r.url = format!("http://{addr}/cache/T1/permissions/info");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_comment_delete_permission(r, query(), || true)
            .await;
        if raw["ok"] == true && raw["allowed"].is_array() {
            let p = result.unwrap();
            assert_eq!(p.raw(), &raw);
            assert_eq!(p.query(), &query());
            assert_eq!(p.allowed(), raw == grant());
        } else {
            assert!(result.is_err());
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn permission_stale_offline_and_bad_request_have_no_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let local = || {
        let mut r = request();
        r.url = format!("http://{addr}/cache/T1/permissions/info");
        r
    };
    assert!(
        transport
            .execute_list_comment_delete_permission(local(), query(), || false)
            .await
            .is_err()
    );
    let mut r = local();
    if let RequestBody::Json(raw) = &mut r.body {
        raw["as_admin"] = json!(true)
    };
    assert!(
        transport
            .execute_list_comment_delete_permission(r, query(), || true)
            .await
            .is_err()
    );
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_comment_delete_permission(local(), query(), || true)
            .await,
        Err(Error::Offline(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn late_permission_response_never_grants_to_retired_owner() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let current = Arc::new(AtomicBool::new(true));
    let owner = current.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_request(&mut stream).await;
        owner.store(false, Ordering::SeqCst);
        let bytes = serde_json::to_vec(&grant()).unwrap();
        let _ = stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .as_bytes(),
            )
            .await;
        let _ = stream.write_all(&bytes).await;
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(60), listener.accept())
                .await
                .is_err()
        );
    });
    let mut r = request();
    r.url = format!("http://{addr}/cache/T1/permissions/info");
    assert!(
        matches!(Transport::new("fixture-cookie").unwrap().execute_list_comment_delete_permission(r,query(),||current.load(Ordering::SeqCst)).await,Err(Error::Api(e)) if e=="list_comment_owner_changed")
    );
    server.await.unwrap();
}
#[tokio::test]
async fn cancelled_pending_permission_has_no_grant_or_replay() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_request(&mut stream).await;
        seen_tx.send(()).unwrap();
        release_rx.await.unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(60), listener.accept())
                .await
                .is_err()
        );
    });
    let mut r = request();
    r.url = format!("http://{addr}/cache/T1/permissions/info");
    let task = tokio::spawn(async move {
        Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_comment_delete_permission(r, query(), || true)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), seen_rx)
        .await
        .unwrap()
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release_tx.send(()).unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn permission_malformed_lost_and_rate_limited_reads_do_not_grant_or_retry() {
    for case in 0..3 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await;
            if case != 1 {
                let bytes = b"not-json";
                let status = if case == 2 {
                    "429 Too Many Requests"
                } else {
                    "200 OK"
                };
                let retry = if case == 2 { "Retry-After: 9\r\n" } else { "" };
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n",bytes.len()).as_bytes()).await.unwrap();
                stream.write_all(bytes).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(60), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut r = request();
        r.url = format!("http://{addr}/cache/T1/permissions/info");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_comment_delete_permission(r, query(), || true)
            .await;
        if case == 2 {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(9)
                })
            ));
        } else {
            assert!(result.is_err());
        }
        server.await.unwrap();
    }
}
