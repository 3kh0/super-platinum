use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        Document, Record,
        comments::{OwnEditPolicy, OwnEditSeed, Page, Target},
    },
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn intent() -> OwnEditIntent {
    let document = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let row = Record::parse(json!({"id":"Rec1","fields":[]})).unwrap();
    let page=Page::parse(&Target::capture(&document,&row).unwrap(),json!({"ok":true,"messages":[
        {"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
        {"ts":"1700000001.000001","thread_ts":"1700000000.000001","user":"U1","text":"Original",
        "blocks":crate::canvas::comments::content::plain("Original")}
    ]})).unwrap();
    let policy = OwnEditPolicy::observed(
        "T1",
        "U1",
        json!({"id":"C01234567"}),
        true,
        false,
        vec![-1],
        false,
        false,
    )
    .unwrap();
    let known = &page.messages()[1];
    let seed = OwnEditSeed::capture("T1", "U1", &page, known, &policy, 1700000002000).unwrap();
    OwnEditIntent::capture(
        &seed,
        known,
        &policy,
        crate::canvas::comments::content::plain("Changed"),
        1700000002000,
    )
    .unwrap()
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
    api::list_record_comment_update(&SlackClient::default(), &workspace(), &intent()).unwrap()
}
fn ack() -> Value {
    json!({"ok":true,"channel":"C01234567","ts":"1700000001.000001","message":{"blocks":intent().record().blocks(),"text":"Changed"}})
}
fn set(request: &mut PreparedRequest, key: &str, value: &str) {
    let RequestBody::Form(fields) = &mut request.body else {
        panic!()
    };
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = value.into()
    } else {
        fields.push((key.into(), value.into()));
    }
}

#[test]
fn exact_rich_factory_is_owner_bound_and_has_no_text_or_delete_fields() {
    let request = request();
    assert!(allowed(&request));
    assert!(matches_intent(&request, &intent()));
    let fields = fields(&request, "/api/chat.update").unwrap();
    assert_eq!(fields.len(), 6);
    assert_eq!(fields["skip_dlp_user_warning"], "false");
    assert_eq!(fields["_x_reason"], "saveMessageEdit");
    assert_eq!(
        serde_json::from_str::<Value>(fields["blocks"]).unwrap(),
        json!(intent().record().blocks())
    );
    assert!(!fields.contains_key("text"));
    assert!(!fields.contains_key("thread_ts"));
    let mut other = workspace();
    other.user_id = "UOTHER".into();
    assert!(api::list_record_comment_update(&SlackClient::default(), &other, &intent()).is_err());
}
#[test]
fn exact_update_allowlist_refuses_extra_fallback_warning_bypass_and_foreign_identity() {
    for (key, value) in [
        ("text", "fallback"),
        ("blocks", "[]"),
        ("markdown_text", "text"),
        ("skip_dlp_user_warning", "true"),
        ("_x_reason", "other"),
        ("file_ids", "F1"),
    ] {
        let mut r = request();
        set(&mut r, key, value);
        assert!(!allowed(&r), "{key}");
    }
    for (key, value) in [
        ("channel", "C76543210"),
        ("ts", "1700000009.000001"),
        (
            "blocks",
            "[{\"type\":\"rich_text\",\"elements\":[{\"type\":\"rich_text_section\",\"elements\":[{\"type\":\"text\",\"text\":\"Other\"}]}]}]",
        ),
    ] {
        let mut r = request();
        set(&mut r, key, value);
        assert!(!matches_intent(&r, &intent()));
    }
    for url in [
        "https://fixture.slack.com/api/chat.delete",
        "https://foreign.example/api/chat.update",
        "https://fixture.slack.com/api/chat.update?unknown=1",
    ] {
        let mut r = request();
        r.url = url.into();
        assert!(!allowed(&r));
    }
    let mut r = request();
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields.push(("token".into(), "duplicate".into()));
    assert!(!allowed(&r));
}
#[test]
fn explicit_rejection_and_uncertainty_are_distinct_without_replay() {
    for error in [
        "cant_update_message",
        "edit_window_closed",
        "message_not_found",
        "ratelimited",
    ] {
        let raw = json!({"ok":false,"error":error});
        assert!(
            matches!(settle(intent(),raw.clone()),Err(ListCommentUpdateFailure::Rejected(v)) if v==raw)
        );
    }
    for raw in [
        json!({"ok":true}),
        json!({"ok":false}),
        json!({"ok":false,"error":"internal_error"}),
        json!({"ok":false,"error":"fatal_error"}),
        json!({"ok":false,"error":"unknown_code"}),
    ] {
        assert!(
            matches!(settle(intent(),raw.clone()),Err(ListCommentUpdateFailure::Unconfirmed{receipt:Some(v),..}) if v==raw)
        );
    }
    assert!(settle(intent(), ack()).is_ok());
}
#[tokio::test]
async fn stale_original_or_offline_does_not_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let local = || {
        let mut r = request();
        r.url = format!("http://{addr}/api/chat.update");
        r
    };
    assert!(matches!(
        transport
            .execute_list_comment_update(local(), intent(), || false)
            .await,
        Err(ListCommentUpdateFailure::NotRequested(_))
    ));
    let mut checks = 0;
    assert!(matches!(
        transport
            .execute_list_comment_update(local(), intent(), || {
                checks += 1;
                checks == 1
            })
            .await,
        Err(ListCommentUpdateFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_comment_update(local(), intent(), || true)
            .await,
        Err(ListCommentUpdateFailure::NotRequested(Error::Offline(_)))
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
        let mut buffer = [0u8; 2048];
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buffer[..n]);
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
async fn original_dispatched_response_survives_retirement_and_never_retries() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for body in [
        Some(ack()),
        Some(json!({"ok":false,"error":"message_not_found"})),
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
            if let Some(body) = body {
                let bytes = serde_json::to_vec(&body).unwrap();
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
        r.url = format!("http://{addr}/api/chat.update");
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_comment_update(r, intent(), || current.load(Ordering::SeqCst))
            .await;
        assert!(!current.load(Ordering::SeqCst));
        match expected {
            Some(raw) if raw.get("message").is_some() => assert!(result.is_ok()),
            Some(raw) if raw["ok"] == false => {
                assert!(matches!(result,Err(ListCommentUpdateFailure::Rejected(v)) if v==raw))
            }
            Some(raw) => assert!(
                matches!(result,Err(ListCommentUpdateFailure::Unconfirmed{receipt:Some(v),..}) if v==raw)
            ),
            None => assert!(matches!(
                result,
                Err(ListCommentUpdateFailure::Unconfirmed { receipt: None, .. })
            )),
        }
        server.await.unwrap();
    }
}
