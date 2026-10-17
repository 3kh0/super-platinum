use super::*;
use serde_json::json;
#[tokio::test]
async fn actual_ordered_capture_permit_and_loopback_keep_vector_order_and_clear_once() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{Document, PersonalTaskLists, ViewSort, ViewSortAttempt, ViewSortChoice},
    };
    for desired in [
        json!([{"key":"a","ascending":true},{"key":"b","ascending":false}]),
        json!([{"key":"b","ascending":false},{"key":"a","ascending":true}]),
        json!([{"key":"b","ascending":false}]),
        json!([]),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let old = if desired.as_array().unwrap().is_empty() {
            json!([{"key":"a","ascending":true},{"key":"b","ascending":false}])
        } else {
            json!([])
        };
        let doc=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[{"id":"A","key":"a","name":"A","type":"text"},{"id":"B","key":"b","name":"B","type":"number","options":{"precision":0}}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[],"sorts":old}]}})).unwrap();
        let personal = PersonalTaskLists::from_response(
            &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap();
        let choices = desired
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                ViewSort::choice(
                    doc.column(if r["key"] == "a" { "A" } else { "B" }).unwrap(),
                    r["ascending"].as_bool().unwrap(),
                )
                .unwrap()
            })
            .collect();
        let intent = ViewSort::capture(
            &doc,
            "V1",
            ViewSortChoice::from_rules(choices),
            Some("full"),
            Some(&personal),
        )
        .unwrap();
        let mut attempt = ViewSortAttempt::new(intent);
        let permit = attempt.admit(&doc, Some("full"), Some(&personal)).unwrap();
        assert!(attempt.admit(&doc, Some("full"), Some(&personal)).is_err());
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: format!("http://{address}"),
            token: "fixture-token".into(),
        };
        let request = api::list_view_sort(&SlackClient::default(), &session, permit);
        let RequestBody::Form(fields) = &request.body else {
            panic!()
        };
        assert_eq!(
            fields,
            &vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F1".into()),
                ("id".into(), "V1".into()),
                ("sorts".into(), desired.to_string()),
                ("_x_reason".into(), "update-view".into())
            ]
        );
        let expected = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut all = Vec::new();
            let body = loop {
                let mut bytes = [0; 2048];
                let n = socket.read(&mut bytes).await.unwrap();
                assert!(n > 0);
                all.extend_from_slice(&bytes[..n]);
                if let Some(i) = all.windows(4).position(|b| b == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8(all[..body].to_vec()).unwrap();
            assert!(header.starts_with("POST /api/lists.views.update"));
            let length = header
                .lines()
                .find_map(|s| {
                    s.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            while all.len() < body + length {
                let mut bytes = [0; 2048];
                let n = socket.read(&mut bytes).await.unwrap();
                assert!(n > 0);
                all.extend_from_slice(&bytes[..n]);
            }
            assert_eq!(&all[body..body + length], expected.as_bytes());
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
                .execute_list_view_sort(request, || true)
                .await
                .unwrap(),
            serde_json::json!({"ok":true})
        );
        server.await.unwrap();
    }
}
