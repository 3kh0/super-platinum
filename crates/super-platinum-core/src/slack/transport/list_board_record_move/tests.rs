use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        BoardRecordMove, BoardRecordMoveAttempt, BoardRecordMovePhase, Document, PersonalTaskLists,
        RecordsPage, saved_views::ProjectionContext,
    },
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn fixture() -> (Document, RecordsPage, ProjectionContext, PersonalTaskLists) {
    let doc=Document::parse(json!({"id":"F1","mode":"list","access":"write","user":"U1","created":1,"list_metadata":{"schema":[
        {"id":"CN","key":"name","name":"Item","type":"text","is_primary_column":true},
        {"id":"CS","key":"stage","name":"Stage","type":"select","options":{"format":"multi_select","choices":[{"value":"A","label":"Alpha"},{"value":"B","label":"Beta"},{"value":"C","label":"Empty"}]}}
    ],"views":[{"id":"V1","type":"board","name":"Board","columns":[{"key":"name","visible":true},{"key":"stage","visible":true}],"grouping":{"group_by":"stage","group_by_column_id":"CS"},"filters":[],"sorts":[],"opaque":7}],"opaque":{"keep":true}}})).unwrap();
    let rows=(1..=4).map(|i|json!({"id":format!("Rec{i}"),"list_id":"F1","position":format!("{i:010}"),"date_created":i,"opaque":{"keep":[i]},"fields":[{"key":"name","text":format!("Item {i}")},{"key":"stage","column_id":"CS","select":[if i<4{"A"}else{"B"}],"opaque":"cell"}]})).collect::<Vec<_>>();
    let page=RecordsPage::parse(json!({"ok":true,"records":rows,"response_metadata":{"next_cursor":"retained+/=","opaque":true}})).unwrap();
    let context = ProjectionContext {
        editable: true,
        ..Default::default()
    };
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (doc, page, context, personal)
}

#[tokio::test]
async fn actual_permit_builders_and_single_attempt_loopback_cover_all_three_source_paths() {
    for (destination, before, path) in [
        ("A", Some("Rec2"), "/api/lists.records.reorder"),
        ("B", Some("Rec4"), "/api/lists.records.reorder"),
        ("C", None, "/api/lists.cells.update"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (doc, page, context, personal) = fixture();
        let intent = BoardRecordMove::capture(
            &doc,
            &page,
            "V1",
            &context,
            "Rec3",
            "A",
            destination,
            before,
            Some("full"),
            Some(&personal),
        )
        .unwrap();
        let mut attempt = BoardRecordMoveAttempt::new(intent);
        let permit = attempt
            .admit(&doc, &page, &context, Some("full"), Some(&personal))
            .unwrap();
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: format!("http://{address}"),
            token: "fixture-token".into(),
        };
        let request = api::list_board_record_move(&SlackClient::default(), &session, permit);
        let RequestBody::Form(fields) = &request.body else {
            panic!()
        };
        let keys = fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>();
        assert_eq!(
            keys,
            match destination {
                "A" => vec!["token", "list_id", "id", "view_id", "position", "_x_reason"],
                "B" => vec![
                    "token",
                    "list_id",
                    "id",
                    "view_id",
                    "position",
                    "cells",
                    "_x_reason"
                ],
                _ => vec!["token", "list_id", "cells", "_x_reason"],
            }
        );
        if let Some((_, cells)) = fields.iter().find(|(key, _)| key == "cells") {
            assert_eq!(
                serde_json::from_str::<Value>(cells).unwrap(),
                json!([{"row_id":"Rec3","column_id":"CS","select":[destination]}])
            );
        }
        let expected = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish();
        let expected_path = path.to_owned();
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
            assert!(header.starts_with(&format!("POST {expected_path}")));
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
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
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
                .execute_list_board_record_move(request, || true)
                .await
                .unwrap(),
            serde_json::json!({"ok":true})
        );
        server.await.unwrap();

        assert!(attempt.finish(false));
        assert_eq!(attempt.phase(), BoardRecordMovePhase::Unconfirmed);
        assert!(!attempt.observe(&doc, &page));
    }
}
#[test]
fn strict_path_field_identity_and_scalar_validator_refuses_ambiguous_forms() {
    let mut request = PreparedRequest {
        method: "POST",
        url: "https://fixture.slack.com/api/lists.records.reorder".into(),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "fixture".into()),
            ("list_id".into(), "F1".into()),
            ("id".into(), "R1".into()),
            ("view_id".into(), "V1".into()),
            ("position".into(), "0000000001.V".into()),
            (
                "cells".into(),
                json!([{"row_id":"R2","column_id":"CS","select":["A"]}]).to_string(),
            ),
            ("_x_reason".into(), "reorder-record".into()),
        ]),
    };
    assert!(!allowed(&request));
    let RequestBody::Form(fields) = &mut request.body else {
        panic!()
    };
    fields[5].1 = json!([{"row_id":"R1","column_id":"CS","select":["A"]}]).to_string();
    assert!(allowed(&request));
    let RequestBody::Form(fields) = &mut request.body else {
        panic!()
    };
    fields[4].1 = "0000000001.0".into();
    assert!(!allowed(&request));
}
