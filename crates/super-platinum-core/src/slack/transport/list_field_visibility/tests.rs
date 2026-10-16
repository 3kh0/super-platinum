use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/lists.views.update"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("id".into(), "V1".into()),
            (
                "columns".into(),
                r#"[{"key":"notes","visible":false,"id":"C_TEXT"}]"#.into(),
            ),
            ("_x_reason".into(), "update-view".into()),
        ]),
    }
}
#[tokio::test]
async fn actual_capture_permit_builder_and_transport_send_exact_hide_and_show_form_once() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{
            Document, PersonalTaskLists, ViewFieldVisibility, ViewFieldVisibilityAttempt,
        },
    };
    for desired in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[
            {"id":"C_TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true},
            {"id":"C_TEXT","key":"notes","name":"Notes","type":"text","options":{"format":"text"}}],
            "views":[{"id":"V1","name":"Custom","type":"table","columns":[{"key":"notes","id":"C_TEXT","visible":!desired}],"grouping":{}}]}})).unwrap();
        let personal = PersonalTaskLists::from_response(
            &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap();
        let intent = ViewFieldVisibility::capture(
            &document,
            "V1",
            "C_TEXT",
            desired,
            Some("full"),
            Some(&personal),
        )
        .unwrap();
        let permit = ViewFieldVisibilityAttempt::new(intent)
            .admit(&document, Some("full"), Some(&personal))
            .unwrap();
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: format!("http://{address}"),
            token: "fixture-token".into(),
        };
        let request = api::list_field_visibility(&SlackClient::default(), &session, permit);
        let RequestBody::Form(fields) = &request.body else {
            panic!()
        };
        assert_eq!(
            fields,
            &vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F1".into()),
                ("id".into(), "V1".into()),
                (
                    "columns".into(),
                    serde_json::json!([{"key":"notes","visible":desired,"id":"C_TEXT"}])
                        .to_string()
                ),
                ("_x_reason".into(), "update-view".into())
            ]
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
                .execute_list_field_visibility(request, || true)
                .await
                .unwrap(),
            serde_json::json!({"ok":true})
        );
        server.await.unwrap();
    }
}
#[tokio::test]
async fn malformed_columns_or_wrong_wrapper_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for value in [
        r#"[]"#,
        r#"[{"key":"notes","visible":"false","id":"C_TEXT"}]"#,
        r#"[{"key":"notes","visible":false,"id":"C_TEXT","width":20}]"#,
        r#"[{"key":"notes","visible":false,"id":"C_TEXT","id":"C_OTHER"}]"#,
        r#"[{"key":"notes","visible":false}]"#,
        r#"[{"key":"a","visible":false,"id":"A"},{"key":"b","visible":true,"id":"B"}]"#,
    ] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[3].1 = value.into();
        assert!(matches!(
            transport.execute_list_field_visibility(r, || true).await,
            Err(ListFieldVisibilityFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_field_visibility(request(address), || false)
            .await,
        Err(ListFieldVisibilityFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_view_rename(request(address), || true)
            .await,
        Err(ListFieldVisibilityFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn rate_limit_and_lost_response_remain_single_attempt() {
    for body in [
        Some(
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ),
        None,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            socket.read(&mut bytes).await.unwrap();
            if let Some(body) = body {
                socket.write_all(body.as_bytes()).await.unwrap();
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
            .execute_list_field_visibility(request(address), || true)
            .await;
        if body.is_some() {
            assert!(matches!(
                result,
                Err(ListFieldVisibilityFailure::Unconfirmed(
                    Error::RateLimited {
                        retry_after_secs: Some(2)
                    }
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ListFieldVisibilityFailure::Unconfirmed(_))
            ));
        }
        server.await.unwrap();
    }
}
