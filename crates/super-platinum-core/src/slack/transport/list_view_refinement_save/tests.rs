use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        Document, PersonalTaskLists, ViewRefinementDestination, ViewRefinementDraft,
        ViewRefinementSave, ViewRefinementSaveAttempt, ViewRefinementSavePhase,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn document() -> Document {
    Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_limits":{"over_view_maximum":false},"list_metadata":{"schema":[{"id":"C1","key":"name","name":"Name","type":"text"}],"views":[{"id":"V1","name":"Delivery","type":"table","columns":[{"id":"C1","key":"name","visible":true}],"filters":[],"sorts":[]}]}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn request(
    address: std::net::SocketAddr,
    new: bool,
) -> (PreparedRequest, ViewRefinementSaveAttempt) {
    let doc = document();
    let draft = ViewRefinementDraft::capture(&doc, "V1")
        .unwrap()
        .with_completed(&doc, true)
        .unwrap();
    let destination = if new {
        ViewRefinementDestination::New {
            name: "新しい ✓".into(),
        }
    } else {
        ViewRefinementDestination::Existing
    };
    let intent = ViewRefinementSave::capture(
        &doc,
        &draft,
        None,
        destination,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    let mut attempt = ViewRefinementSaveAttempt::new(intent);
    let permit = attempt
        .admit(&doc, Some("full"), Some(&personal()))
        .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "fixture-token".into(),
    };
    (
        api::list_view_refinement_save(&SlackClient::default(), &session, permit),
        attempt,
    )
}
#[tokio::test]
async fn actual_capture_permit_and_shared_executor_send_update_create_once_even_lost_reply() {
    for new in [false, true] {
        for lost in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (request, mut attempt) = request(listener.local_addr().unwrap(), new);
            assert!(allowed(&request));
            let RequestBody::Form(fields) = &request.body else {
                panic!()
            };
            let expected = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .finish();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut all = Vec::new();
                let start = loop {
                    let mut b = [0; 2048];
                    let n = socket.read(&mut b).await.unwrap();
                    assert!(n > 0);
                    all.extend_from_slice(&b[..n]);
                    if let Some(at) = all.windows(4).position(|s| s == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let headers = String::from_utf8(all[..start].to_vec()).unwrap();
                assert!(headers.starts_with(if new {
                    "POST /api/lists.views.create"
                } else {
                    "POST /api/lists.views.update"
                }));
                let length: usize = headers
                    .lines()
                    .find_map(|l| {
                        l.split_once(':')
                            .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                            .map(|(_, v)| v.trim().parse().unwrap())
                    })
                    .unwrap();
                while all.len() < start + length {
                    let mut b = [0; 2048];
                    let n = socket.read(&mut b).await.unwrap();
                    assert!(n > 0);
                    all.extend_from_slice(&b[..n]);
                }
                assert_eq!(&all[start..start + length], expected.as_bytes());
                if !lost {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
                }
                drop(socket);
                assert!(
                    tokio::time::timeout(Duration::from_millis(80), listener.accept())
                        .await
                        .is_err()
                );
            });
            let result = Transport::new("fixture-cookie")
                .unwrap()
                .execute_list_view_refinement_save(request, || true)
                .await;
            assert_eq!(result.is_ok(), !lost);
            server.await.unwrap();
            assert!(attempt.finish(false));
            assert_eq!(attempt.phase(), ViewRefinementSavePhase::Unconfirmed);
            assert!(
                attempt
                    .admit(&document(), Some("full"), Some(&personal()))
                    .is_err()
            );
        }
    }
}
#[tokio::test]
async fn stale_guard_stops_before_any_socket_admission() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (request, _) = request(listener.local_addr().unwrap(), false);
    let result = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_view_refinement_save(request, || false)
        .await;
    assert!(matches!(
        result,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
#[test]
fn strict_shape_refuses_duplicate_extra_wrong_method_and_wrong_scalar_fields() {
    let address = "127.0.0.1:9".parse().unwrap();
    let (valid, _) = request(address, false);
    assert!(allowed(&valid));
    for field in ["token", "id", "filters", "_x_reason"] {
        let mut bad = valid.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            panic!()
        };
        let duplicate = fields.iter().find(|(k, _)| k == field).unwrap().clone();
        fields.push(duplicate);
        assert!(!allowed(&bad));
    }
    for (field, value) in [
        (
            "filters",
            r#"[{"key":"name","operator":"is","values":[1]}]"#,
        ),
        ("sorts", r#"[{"key":"name","ascending":"true"}]"#),
        ("info_column_filters", "null"),
        ("show_completed_items", "1"),
        ("_x_reason", "create-view"),
    ] {
        let mut bad = valid.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            panic!()
        };
        fields.iter_mut().find(|(k, _)| k == field).unwrap().1 = value.into();
        assert!(!allowed(&bad));
    }
    let mut bad = valid.clone();
    bad.url = "https://other.example/api/lists.views.update".into();
    assert!(!allowed(&bad));
    let mut bad = valid;
    bad.method = "GET";
    assert!(!allowed(&bad));
}
