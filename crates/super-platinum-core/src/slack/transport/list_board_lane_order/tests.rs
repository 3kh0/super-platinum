use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        BoardLaneOrder, BoardLaneOrderAttempt, BoardLaneOrderPhase, BoardLanePlacement, Document,
        PersonalTaskLists,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn document() -> Document {
    Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[{"id":"C1","key":"done","name":"Done","type":"checkbox"}],"views":[{"id":"V1","type":"board","name":"Board","columns":[],"grouping":{"group_by":"done","group_by_column_id":"C1","opaque":{"keep":7}}}]}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn request(address: std::net::SocketAddr) -> (PreparedRequest, BoardLaneOrderAttempt) {
    let doc = document();
    let intent = BoardLaneOrder::capture(
        &doc,
        "V1",
        "true",
        "",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    let mut attempt = BoardLaneOrderAttempt::new(intent);
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
        api::list_board_lane_order(&SlackClient::default(), &session, permit),
        attempt,
    )
}
#[tokio::test]
async fn real_permit_builder_loopback_exact_form_once_ack_stays_unconfirmed() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request, mut attempt) = request(address);
    let RequestBody::Form(fields) = &request.body else {
        panic!()
    };
    assert_eq!(
        fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        vec!["token", "list_id", "id", "grouping", "_x_reason"]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&fields[3].1).unwrap(),
        serde_json::json!({"group_by":"done","group_by_column_id":"C1","opaque":{"keep":7},"order":[{"checkbox":true},{"checkbox":false}]})
    );
    let expected = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut all = Vec::new();
        let body_start = loop {
            let mut bytes = [0; 2048];
            let n = socket.read(&mut bytes).await.unwrap();
            assert!(n > 0);
            all.extend_from_slice(&bytes[..n]);
            if let Some(at) = all.windows(4).position(|s| s == b"\r\n\r\n") {
                break at + 4;
            }
        };
        let header = String::from_utf8(all[..body_start].to_vec()).unwrap();
        assert!(header.starts_with("POST /api/lists.views.update"));
        let length = header
            .lines()
            .find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
            })
            .unwrap();
        while all.len() < body_start + length {
            let mut bytes = [0; 2048];
            let n = socket.read(&mut bytes).await.unwrap();
            assert!(n > 0);
            all.extend_from_slice(&bytes[..n]);
        }
        assert_eq!(&all[body_start..body_start + length], expected.as_bytes());
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .await
            .unwrap();
        drop(socket);
        assert!(
            tokio::time::timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    });
    assert_eq!(
        Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_board_lane_order(request, || true)
            .await
            .unwrap(),
        serde_json::json!({"ok":true})
    );
    server.await.unwrap();

    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), BoardLaneOrderPhase::Unconfirmed);
    assert!(!attempt.observe(&document()));
}
#[tokio::test]
async fn bad_shapes_and_retired_guard_never_connect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    for invalid in [
        serde_json::json!({"group_by":"done","order":[{"checkbox":true}]}),
        serde_json::json!({"group_by":"done","group_by_column_id":"C1","order":[{"checkbox":"true"}]}),
        serde_json::json!({"group_by":"done","group_by_column_id":"C1","order":[{"checkbox":true,"opaque":1}]}),
    ] {
        let (mut r, _) = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[3].1 = invalid.to_string();
        assert!(!allowed(&r));
        assert!(matches!(
            Transport::new("fixture-cookie")
                .unwrap()
                .execute_list_board_lane_order(r, || true)
                .await,
            Err(ListColumnRenameFailure::NotRequested(_))
        ));
    }
    let (r, _) = request(address);
    assert!(matches!(
        Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_board_lane_order(r, || false)
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
