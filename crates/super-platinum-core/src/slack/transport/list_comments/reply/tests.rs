use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        Document, Record,
        comments::{Page, ReplyPolicy, Target},
    },
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn intent() -> ReplyIntent {
    let d = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let r = Record::parse(json!({"id":"Rec1","fields":[]})).unwrap();
    let p = Page::parse(
        &Target::capture(&d, &r).unwrap(),
        json!({"ok":true,"messages":[{
        "type":"message","subtype":"list_record_comment","ts":"1700000000.000001",
        "slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}}]}),
    )
    .unwrap();
    ReplyIntent::capture(
        "TTEST",
        "UTEST",
        p.target(),
        p.root().unwrap(),
        &ReplyPolicy::observed(json!({"id":"C01234567"}), false, true),
        "Literal *text* \u{4e8c}\nnext",
        "11111111-2222-4333-8444-555555555555",
        "1700000001.000001",
    )
    .unwrap()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST".into(),
        user_id: "UTEST".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn request() -> PreparedRequest {
    api::list_record_comment_reply(&SlackClient::default(), &workspace(), &intent()).unwrap()
}
fn ack() -> Value {
    let i = intent();
    let r = i.record();
    json!({"ok":true,"channel":r.target().channel_id(),"ts":"1700000002.000001",
    "message":{"ts":"1700000002.000001","thread_ts":r.target().root_ts(),"user":r.user(),"client_msg_id":r.client_msg_id(),"blocks":r.blocks()}})
}
fn set(r: &mut PreparedRequest, key: &str, value: &str) {
    let RequestBody::Form(f) = &mut r.body else {
        panic!()
    };
    if let Some((_, v)) = f.iter_mut().find(|(k, _)| k == key) {
        *v = value.into()
    } else {
        f.push((key.into(), value.into()));
    }
}
#[test]
fn real_factory_has_exact_normal_blocks_only_contract_and_owner() {
    let req = request();
    assert!(allowed(&req));
    assert!(matches_intent(&req, &intent()));
    let f = fields(&req, "/api/chat.postMessage").unwrap();
    assert_eq!(f.len(), 10);
    assert_eq!(f["type"], "message");
    assert_eq!(f["ts"], "1700000001.000001");
    assert_eq!(f["thread_ts"], "1700000000.000001");
    assert_eq!(f["reply_broadcast"], "false");
    assert_eq!(f["include_channel_perm_error"], "true");
    assert_eq!(f["_x_reason"], "webapp_message_send");
    assert!(!f.contains_key("text") && !f.contains_key("skip_dlp_user_warning"));
    assert_eq!(
        serde_json::from_str::<Value>(f["blocks"]).unwrap(),
        json!(intent().record().blocks())
    );
    let mut w = workspace();
    w.user_id = "UOTHER".into();
    assert!(api::list_record_comment_reply(&SlackClient::default(), &w, &intent()).is_err());
}
#[test]
fn strict_write_scope_rejects_synthetic_thread_mutation_extra_and_foreign_request() {
    for (k, v) in [
        ("thread_ts", "F01234567-Rec1"),
        ("reply_broadcast", "true"),
        ("blocks", "[]"),
        ("text", "fallback"),
        ("skip_dlp_user_warning", "true"),
        ("include_channel_perm_error", "false"),
        ("_x_reason", "other"),
    ] {
        let mut r = request();
        set(&mut r, k, v);
        assert!(!allowed(&r), "{k}");
    }
    for (k, v) in [
        ("channel", "C76543210"),
        ("thread_ts", "1700000009.000001"),
        ("ts", "1700000009.000001"),
        ("client_msg_id", "99999999-2222-4333-8444-555555555555"),
    ] {
        let mut r = request();
        set(&mut r, k, v);
        assert!(!matches_intent(&r, &intent()), "{k}");
    }
    for url in [
        "https://foreign.example/api/chat.postMessage",
        "https://fixture.slack.com/api/chat.update",
        "https://fixture.slack.com/api/chat.postMessage?unknown=1",
    ] {
        let mut r = request();
        r.url = url.into();
        assert!(!allowed(&r));
    }
    let mut r = request();
    r.method = "GET";
    assert!(!allowed(&r));
    let mut r = request();
    let RequestBody::Form(f) = &mut r.body else {
        panic!()
    };
    f.push(("token".into(), "extra".into()));
    assert!(!allowed(&r));
}
#[test]
fn explicit_negative_is_rejected_but_malformed_or_mismatched_ack_keeps_raw_unknown() {
    let raw = json!({"ok":false,"error":"restricted_action","opaque":7});
    assert!(
        matches!(settle(intent(),raw.clone()),Err(ListCommentReplyFailure::Rejected(v)) if v==raw)
    );
    for raw in [
        json!({"ok":true}),
        json!({"ok":false}),
        json!({"ok":false,"error":null}),
    ] {
        assert!(
            matches!(settle(intent(),raw.clone()),Err(ListCommentReplyFailure::Unconfirmed{receipt:Some(v),..}) if v==raw)
        );
    }
    assert!(settle(intent(), ack()).is_ok());
}
#[tokio::test]
async fn initial_and_last_owner_fence_or_offline_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let local = || {
        let mut r = request();
        r.url = format!("http://{addr}/api/chat.postMessage");
        r
    };
    assert!(matches!(
        transport
            .execute_list_comment_reply(local(), intent(), || false)
            .await,
        Err(ListCommentReplyFailure::NotRequested(_))
    ));
    let mut checks = 0;
    assert!(matches!(
        transport
            .execute_list_comment_reply(local(), intent(), || {
                checks += 1;
                checks == 1
            })
            .await,
        Err(ListCommentReplyFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_comment_reply(local(), intent(), || true)
            .await,
        Err(ListCommentReplyFailure::NotRequested(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
}
async fn read_request(stream: &mut tokio::net::TcpStream) {
    let mut data = Vec::new();
    loop {
        let mut b = [0u8; 2048];
        let n = stream.read(&mut b).await.unwrap();
        assert!(n > 0);
        data.extend_from_slice(&b[..n]);
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&data[..i]).to_ascii_lowercase();
            let size: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if data.len() >= i + 4 + size {
                return;
            }
        }
    }
}
#[tokio::test]
async fn dispatched_response_survives_retirement_and_is_not_replayed() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for body in [
        Some(ack()),
        Some(json!({"ok":false,"error":"restricted_action"})),
        Some(json!({"ok":true})),
        None,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let current = Arc::new(AtomicBool::new(true));
        let owned = current.clone();
        let expected = body.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await;
            owned.store(false, Ordering::SeqCst);
            if let Some(raw) = body {
                let bytes = serde_json::to_vec(&raw).unwrap();
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
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(60), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut r = request();
        r.url = format!("http://{addr}/api/chat.postMessage");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_comment_reply(r, intent(), || current.load(Ordering::SeqCst))
            .await;
        assert!(!current.load(Ordering::SeqCst));
        match expected {
            Some(raw) if raw.get("message").is_some() => assert!(result.is_ok()),
            Some(raw) if raw["ok"] == false => {
                assert!(matches!(result,Err(ListCommentReplyFailure::Rejected(v)) if v==raw))
            }
            Some(raw) => assert!(
                matches!(result,Err(ListCommentReplyFailure::Unconfirmed{receipt:Some(v),..}) if v==raw)
            ),
            None => assert!(matches!(
                result,
                Err(ListCommentReplyFailure::Unconfirmed { receipt: None, .. })
            )),
        }
        server.await.unwrap();
    }
}
