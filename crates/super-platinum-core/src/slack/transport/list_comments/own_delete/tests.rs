use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        Document, Record,
        comments::{OwnDeletePermissionQuery, OwnDeletePolicy, Page, Target},
    },
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn intent() -> OwnDeleteIntent {
    let doc = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let record = Record::parse(json!({"id":"Rec1","fields":[]})).unwrap();
    let page=Page::parse(&Target::capture(&doc,&record).unwrap(),json!({"ok":true,"messages":[
        {"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
        {"ts":"1700000001.000001","thread_ts":"1700000000.000001","user":"U1","text":"Original","opaque":{"retain":true}}
    ]})).unwrap();
    let permission = OwnDeletePermissionQuery::capture("T1", "U1", "C01234567", false)
        .unwrap()
        .accept(json!({"ok":true,"allowed":["DELETE_OWN_MESSAGE"]}))
        .unwrap();
    let policy = OwnDeletePolicy::observed(
        "T1",
        "U1",
        json!({"id":"C01234567"}),
        true,
        false,
        false,
        &permission,
    )
    .unwrap();
    OwnDeleteIntent::capture("T1", "U1", &page, &page.messages()[1], &policy).unwrap()
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
    api::list_record_comment_delete(&SlackClient::default(), &workspace(), &intent()).unwrap()
}
fn ack() -> Value {
    json!({"ok":true,"channel":"C01234567","ts":"1700000001.000001","opaque":{"original":true}})
}
fn set(r: &mut PreparedRequest, key: &str, value: &str) {
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = value.into()
    } else {
        fields.push((key.into(), value.into()))
    }
}
#[test]
fn factory_is_one_exact_own_message_and_refuses_foreign_owner() {
    let r = request();
    assert!(allowed(&r));
    assert!(matches_intent(&r, &intent()));
    let f = fields(&r, "/api/chat.delete").unwrap();
    assert_eq!(
        f,
        BTreeMap::from([
            ("token", "fixture-only"),
            ("channel", "C01234567"),
            ("ts", "1700000001.000001"),
            ("_x_reason", "animateAndDeleteMessageApi")
        ])
    );
    for (team, user) in [("TOTHER", "U1"), ("T1", "UOTHER")] {
        let mut owner = workspace();
        owner.team_id = team.into();
        owner.user_id = user.into();
        assert!(
            api::list_record_comment_delete(&SlackClient::default(), &owner, &intent()).is_err()
        );
    }
}
#[test]
fn exact_allowlist_rejects_broadcast_thread_bulk_json_and_foreign_request() {
    for (key, value) in [
        ("broadcast", "true"),
        ("thread_ts", "1700000000.000001"),
        ("delete_all", "true"),
        ("as_user", "true"),
        ("ts", "bad"),
        ("channel", "not-channel"),
        ("_x_reason", "other"),
        ("text", "replacement"),
    ] {
        let mut r = request();
        set(&mut r, key, value);
        assert!(!allowed(&r), "{key}");
    }
    for (key, value) in [("channel", "C76543210"), ("ts", "1700000009.000001")] {
        let mut r = request();
        set(&mut r, key, value);
        assert!(allowed(&r));
        assert!(!matches_intent(&r, &intent()));
    }
    for url in [
        "https://foreign.example/api/chat.delete",
        "https://fixture.slack.com/api/chat.update",
        "https://fixture.slack.com/api/chat.delete?unknown=1",
        "https://user@fixture.slack.com/api/chat.delete",
        "https://fixture.slack.com/api/chat.delete#fragment",
    ] {
        let mut r = request();
        r.url = url.into();
        assert!(!allowed(&r), "{url}");
    }
    let mut r = request();
    r.method = "GET";
    assert!(!allowed(&r));
    r = request();
    r.body = RequestBody::Json(
        json!({"token":"fixture-only","channel":"C01234567","ts":"1700000001.000001","_x_reason":"animateAndDeleteMessageApi"}),
    );
    assert!(!allowed(&r));
    r = request();
    if let RequestBody::Form(f) = &mut r.body {
        f.push(("token".into(), "duplicate".into()))
    };
    assert!(!allowed(&r));
}
#[test]
fn rejection_is_bounded_and_missing_or_conflicting_ack_identity_is_uncertain() {
    for error in [
        "cant_delete_message",
        "message_not_found",
        "channel_not_found",
        "not_authed",
        "invalid_auth",
        "token_revoked",
        "account_inactive",
        "missing_scope",
        "no_permission",
        "ratelimited",
    ] {
        let raw = json!({"ok":false,"error":error});
        assert!(
            matches!(settle(intent(),raw.clone()),Err(ListCommentDeleteFailure::Rejected(v)) if v==raw)
        );
    }
    for raw in [
        json!({"ok":true}),
        json!({"ok":false}),
        json!({"ok":false,"error":"internal_error"}),
        json!({"ok":false,"error":"fatal_error"}),
        json!({"ok":false,"error":"unknown_code"}),
        json!({"ok":true,"channel":"C76543210","ts":"1700000001.000001"}),
        json!({"ok":true,"channel":"C01234567","ts":"1700000009.000001"}),
        json!({"ok":"true","channel":"C01234567","ts":"1700000001.000001"}),
        json!({"ok":true,"channel":"C01234567","ts":"1700000001.000001","error":"internal_error"}),
    ] {
        assert!(
            matches!(settle(intent(),raw.clone()),Err(ListCommentDeleteFailure::Unconfirmed{receipt:Some(v),..}) if v==raw)
        );
    }
    let accepted = settle(intent(), ack()).unwrap();
    assert_eq!(accepted.raw(), &ack());
    assert_eq!(accepted.record().ts(), "1700000001.000001");
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
async fn stale_offline_or_mismatched_original_never_dispatches() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let local = || {
        let mut r = request();
        r.url = format!("http://{addr}/api/chat.delete");
        r
    };
    assert!(matches!(
        transport
            .execute_list_comment_delete(local(), intent(), || false)
            .await,
        Err(ListCommentDeleteFailure::NotRequested(_))
    ));
    let mut checks = 0;
    assert!(matches!(
        transport
            .execute_list_comment_delete(local(), intent(), || {
                checks += 1;
                checks == 1
            })
            .await,
        Err(ListCommentDeleteFailure::NotRequested(_))
    ));
    let mut wrong = local();
    set(&mut wrong, "ts", "1700000009.000001");
    assert!(matches!(
        transport
            .execute_list_comment_delete(wrong, intent(), || true)
            .await,
        Err(ListCommentDeleteFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_comment_delete(local(), intent(), || true)
            .await,
        Err(ListCommentDeleteFailure::NotRequested(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn one_dispatch_preserves_late_ack_rejection_malformed_loss_and_retry_after() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for case in 0..7 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let current = Arc::new(AtomicBool::new(true));
        let owned = current.clone();
        let raw = match case {
            0 => Some(ack()),
            1 => Some(json!({"ok":false,"error":"message_not_found"})),
            2 => Some(json!({"ok":true})),
            _ => None,
        };
        let expected = raw.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (headers, body) = read_request(&mut stream).await;
            assert!(headers.starts_with("POST /api/chat.delete HTTP/1.1"));
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-type: application/x-www-form-urlencoded")
            );
            let fields = url::form_urlencoded::parse(&body)
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(fields.len(), 4);
            assert_eq!(fields["ts"], "1700000001.000001");
            assert_eq!(fields["channel"], "C01234567");
            assert_eq!(fields["_x_reason"], "animateAndDeleteMessageApi");
            owned.store(false, Ordering::SeqCst);
            if case != 4 {
                let bytes = if case == 3 {
                    b"not-json".to_vec()
                } else if let Some(raw) = raw {
                    serde_json::to_vec(&raw).unwrap()
                } else {
                    b"{}".to_vec()
                };
                let status = if case == 5 {
                    "429 Too Many Requests"
                } else if case == 6 {
                    "500 Internal Server Error"
                } else {
                    "200 OK"
                };
                let retry = if case == 5 { "Retry-After: 7\r\n" } else { "" };
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n",bytes.len()).as_bytes()).await.unwrap();
                stream.write_all(&bytes).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut r = request();
        r.url = format!("http://{addr}/api/chat.delete");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_comment_delete(r, intent(), || current.load(Ordering::SeqCst))
            .await;
        assert!(!current.load(Ordering::SeqCst));
        match case {
            0 => assert_eq!(result.unwrap().raw(), &expected.unwrap()),
            1 => assert!(
                matches!(result,Err(ListCommentDeleteFailure::Rejected(v)) if Some(&v)==expected.as_ref())
            ),
            2 => assert!(
                matches!(result,Err(ListCommentDeleteFailure::Unconfirmed{receipt:Some(v),..}) if Some(&v)==expected.as_ref())
            ),
            5 => assert!(matches!(
                result,
                Err(ListCommentDeleteFailure::Unconfirmed {
                    error: Error::RateLimited {
                        retry_after_secs: Some(7)
                    },
                    receipt: None
                })
            )),
            _ => assert!(matches!(
                result,
                Err(ListCommentDeleteFailure::Unconfirmed { receipt: None, .. })
            )),
        }
        server.await.unwrap();
    }
}
