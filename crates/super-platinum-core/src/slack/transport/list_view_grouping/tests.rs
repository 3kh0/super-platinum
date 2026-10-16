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
            ("grouping".into(), r#"{"group_by":"checked_✓"}"#.into()),
            ("_x_reason".into(), "update-view".into()),
        ]),
    }
}
#[tokio::test]
async fn actual_capture_permit_builder_and_transport_send_set_and_clear_once() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{
            Document, PersonalTaskLists, ViewGrouping, ViewGroupingAttempt, ViewGroupingChoice,
            ViewGroupingPhase,
        },
    };
    for kind in ["table", "board"] {
        for key in ["checked_✓", "status", ""] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","created":1,"user":"U1","list_metadata":{"schema":[
            {"id":"C_CHECK","key":"checked_✓","name":"Verified","type":"checkbox"},
            {"id":"C_SELECT","key":"status","name":"Status","type":"select","options":{"choices":[{"value":"a","label":"Alpha"}]}}],
            "views":[{"id":"V1","name":"Custom","type":kind,"columns":[],"grouping":if key.is_empty() {serde_json::json!({"group_by":"status"})} else {serde_json::json!({})}}]}})).unwrap();
            let personal = PersonalTaskLists::from_response(
                &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
            )
            .unwrap();
            let desired = if key.is_empty() {
                ViewGroupingChoice::None
            } else {
                ViewGroupingChoice::Column {
                    column_id: if key == "status" {
                        "C_SELECT"
                    } else {
                        "C_CHECK"
                    }
                    .into(),
                }
            };
            let intent =
                ViewGrouping::capture(&document, "V1", desired, Some("full"), Some(&personal))
                    .unwrap();
            let mut attempt = ViewGroupingAttempt::new(intent);
            let permit = attempt
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
            let request = api::list_view_grouping(&SlackClient::default(), &session, permit);
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
                        "grouping".into(),
                        serde_json::json!({"group_by":key}).to_string()
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
                    .execute_list_view_grouping(request, || true)
                    .await
                    .unwrap(),
                serde_json::json!({"ok":true})
            );
            server.await.unwrap();
            assert!(attempt.finish(false));
            assert_eq!(attempt.phase(), ViewGroupingPhase::Unconfirmed);
            assert!(!attempt.observe(&document));
            let mut canonical = document.raw.clone();
            canonical["list_metadata"]["views"][0]["grouping"] =
                serde_json::json!({"group_by":key});
            assert!(attempt.observe(&Document::parse(canonical).unwrap()));
            assert!(
                attempt
                    .admit(&document, Some("full"), Some(&personal))
                    .is_err()
            );
        }
    }
}
#[tokio::test]
async fn malformed_grouping_or_wrong_wrapper_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for value in [
        r#"null"#,
        r#"{}"#,
        r#"[]"#,
        r#"{"group_by":null}"#,
        r#"{"group_by":"checked_✓","group_by":"status"}"#,
        r#"{"group_by":"status","order":[]}"#,
        r#"{"group_by":"status","group_by_column_id":"C_SELECT"}"#,
        r#"{"group_by":"status","hide_empty_groups":false}"#,
        r#"{"group_by":" "}"#,
        r#"{"group_by":"bad\nkey"}"#,
    ] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[3].1 = value.into();
        assert!(matches!(
            transport.execute_list_view_grouping(r, || true).await,
            Err(ListViewGroupingFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_view_grouping(request(address), || false)
            .await,
        Err(ListViewGroupingFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_view_rename(request(address), || true)
            .await,
        Err(ListViewGroupingFailure::NotRequested(_))
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
            .execute_list_view_grouping(request(address), || true)
            .await;
        if body.is_some() {
            assert!(matches!(
                result,
                Err(ListViewGroupingFailure::Unconfirmed(Error::RateLimited {
                    retry_after_secs: Some(2)
                }))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ListViewGroupingFailure::Unconfirmed(_))
            ));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn original_response_is_retained_after_current_owner_retires() {
    use std::sync::atomic::AtomicBool;
    let live = Arc::new(AtomicBool::new(true));
    let retired = live.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        stream.read(&mut bytes).await.unwrap();
        retired.store(false, Ordering::SeqCst);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .await
            .unwrap();
    });
    let result = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_view_grouping(request(address), || live.load(Ordering::SeqCst))
        .await
        .unwrap();
    assert_eq!(result, serde_json::json!({"ok":true}));
    assert!(!live.load(Ordering::SeqCst));
    server.await.unwrap();
}

#[tokio::test]
async fn redirects_declared_and_streamed_bounds_and_nested_duplicates_never_retry() {
    for (headers,body) in [
        ("HTTP/1.1 302 Found\r\nLocation: /api/files.info\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),Vec::new()),
        ("HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n".to_owned(),Vec::new()),
        ("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_owned(),vec![b' ';1048577]),
        ("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_owned(),br#"{"ok":true,"file":{"id":"F1","id":"F2"}}"#.to_vec()),
    ]{
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();
        let server=tokio::spawn(async move{
            let(mut stream,_)=listener.accept().await.unwrap();let mut bytes=[0;4096];stream.read(&mut bytes).await.unwrap();
            let _=stream.write_all(headers.as_bytes()).await;let _=stream.write_all(&body).await;drop(stream);
            assert!(tokio::time::timeout(Duration::from_millis(80),listener.accept()).await.is_err());
        });
        let result=Transport::new("fixture-cookie").unwrap().execute_list_view_grouping(request(addr),||true).await;
        assert!(matches!(result,Err(ListViewGroupingFailure::Unconfirmed(_))));server.await.unwrap();
    }
}

#[tokio::test]
async fn second_owner_fence_offline_and_cross_form_refuse_without_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let mut checks = 0;
    assert!(matches!(
        transport
            .execute_list_view_grouping(request(address), || {
                checks += 1;
                checks == 1
            })
            .await,
        Err(ListViewGroupingFailure::NotRequested(_))
    ));
    assert_eq!(checks, 2);
    let mut foreign = request(address);
    let RequestBody::Form(fields) = &mut foreign.body else {
        panic!()
    };
    fields[3] = (
        "columns".into(),
        r#"[{"key":"notes","id":"C_TEXT","visible":false}]"#.into(),
    );
    assert!(matches!(
        transport.execute_list_view_grouping(foreign, || true).await,
        Err(ListViewGroupingFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_field_visibility(request(address), || true)
            .await,
        Err(ListViewGroupingFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_view_grouping(request(address), || true)
            .await,
        Err(ListViewGroupingFailure::NotRequested(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn grouping_wrapper_requires_exact_five_fields_and_bounded_identity() {
    let address = "127.0.0.1:1".parse().unwrap();
    assert!(allowed(&request(address)));
    for branch in 0..9 {
        let mut bad = request(address);
        match branch {
            0 => bad.method = "GET",
            1 => bad.url = "https://example.test/api/lists.views.update".into(),
            2 => bad.url = "https://fixture.slack.com/api/lists.views.update?group_by=other".into(),
            3 => bad.url = "https://fixture.slack.com/api/lists.columns.update".into(),
            _ => {
                let RequestBody::Form(fields) = &mut bad.body else {
                    panic!()
                };
                match branch {
                    4 => fields.push(("name".into(), "Changed".into())),
                    5 => fields.swap(1, 2),
                    6 => fields[0].1.clear(),
                    7 => fields[1].1 = "x".repeat(1025),
                    _ => fields[3].1 = serde_json::json!({"group_by":"x".repeat(1025)}).to_string(),
                }
            }
        }
        assert!(!allowed(&bad), "branch {branch}");
    }
}
#[tokio::test]
async fn other_view_mutation_wrappers_refuse_grouping_form_without_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        transport
            .execute_list_view_sort(request(address), || true)
            .await,
        Err(ListViewGroupingFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_view_filter(request(address), || true)
            .await,
        Err(ListViewGroupingFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
