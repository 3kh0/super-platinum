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
                "filters".into(),
                r#"[{"key":"notes","operator":"is_empty","values":[]}]"#.into(),
            ),
            ("info_column_filters".into(), "[]".into()),
            ("_x_reason".into(), "update-view".into()),
        ]),
    }
}
#[tokio::test]
async fn actual_capture_permit_builder_and_transport_send_both_checkbox_operators_and_clear_once() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{
            Document, PersonalTaskLists, ViewFilter, ViewFilterAttempt, ViewFilterChoice,
        },
    };
    for desired in [Some(false), Some(true), None] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[
            {"id":"C_TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true},
            {"id":"C_TEXT","key":"notes","name":"Notes","type":"checkbox","options":{}}],
            "views":[{"id":"V1","name":"Custom","type":"table","columns":[],"filters":if desired.is_some() { serde_json::json!([]) } else { serde_json::json!([{"key":"notes","operator":"is_not_empty","values":[]}]) },"grouping":{}}]}})).unwrap();
        let personal = PersonalTaskLists::from_response(
            &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap();
        let intent = ViewFilter::capture(
            &document,
            "V1",
            desired.map_or(ViewFilterChoice::None, |checked| {
                ViewFilterChoice::Checkbox {
                    column_id: "C_TEXT".into(),
                    checked,
                }
            }),
            Some("full"),
            Some(&personal),
        )
        .unwrap();
        let permit = ViewFilterAttempt::new(intent)
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
        let request = api::list_view_filter(&SlackClient::default(), &session, permit);
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
                    "filters".into(),
                    desired
                        .map_or(
                            serde_json::json!([]),
                            |checked| serde_json::json!([{"key":"notes","operator":if checked {"is_not_empty"}else{"is_empty"},"values":[]}])
                        )
                        .to_string()
                ),
                ("info_column_filters".into(), "[]".into()),
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
                .execute_list_view_filter(request, || true)
                .await
                .unwrap(),
            serde_json::json!({"ok":true})
        );
        server.await.unwrap();
    }
}
#[tokio::test]
async fn malformed_filters_or_wrong_wrapper_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for value in [
        r#"null"#,
        r#"{}"#,
        r#"[{"key":"notes","operator":"equal","values":[]}]"#,
        r#"[{"key":"notes","operator":"is_empty","values":[false]}]"#,
        r#"[{"key":"notes","operator":"is_empty","values":[],"id":"C_TEXT"}]"#,
        r#"[{"key":"notes","operator":"is_empty","values":[],"operator":"is_not_empty"}]"#,
        r#"[{"key":"notes","operator":"is_empty"}]"#,
        r#"[{"key":"a","operator":"is_empty","values":[]},{"key":"b","operator":"is_not_empty","values":[]}]"#,
    ] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[3].1 = value.into();
        assert!(matches!(
            transport.execute_list_view_filter(r, || true).await,
            Err(ListViewFilterFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_view_filter(request(address), || false)
            .await,
        Err(ListViewFilterFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_view_rename(request(address), || true)
            .await,
        Err(ListViewFilterFailure::NotRequested(_))
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
            .execute_list_view_filter(request(address), || true)
            .await;
        if body.is_some() {
            assert!(matches!(
                result,
                Err(ListViewFilterFailure::Unconfirmed(Error::RateLimited {
                    retry_after_secs: Some(2)
                }))
            ));
        } else {
            assert!(matches!(result, Err(ListViewFilterFailure::Unconfirmed(_))));
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
        .execute_list_view_filter(request(address), || live.load(Ordering::SeqCst))
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
        let result=Transport::new("fixture-cookie").unwrap().execute_list_view_filter(request(addr),||true).await;
        assert!(matches!(result,Err(ListViewFilterFailure::Unconfirmed(_))));server.await.unwrap();
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
            .execute_list_view_filter(request(address), || {
                checks += 1;
                checks == 1
            })
            .await,
        Err(ListViewFilterFailure::NotRequested(_))
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
        transport.execute_list_view_filter(foreign, || true).await,
        Err(ListViewFilterFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_field_visibility(request(address), || true)
            .await,
        Err(ListViewFilterFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_view_filter(request(address), || true)
            .await,
        Err(ListViewFilterFailure::NotRequested(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn absent_null_or_nonempty_metadata_filters_and_sort_form_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for value in [
        None,
        Some("null"),
        Some("{}"),
        Some(r#"[{"key":"created_by","operator":"is_empty","values":[]}]"#),
    ] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        if let Some(value) = value {
            fields[4].1 = value.into();
        } else {
            fields.remove(4);
        }
        assert!(matches!(
            transport.execute_list_view_filter(r, || true).await,
            Err(ListViewFilterFailure::NotRequested(_))
        ));
    }
    let mut r = request(address);
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields.remove(4);
    fields[3] = (
        "sorts".into(),
        r#"[{"key":"notes","ascending":true}]"#.into(),
    );
    assert!(matches!(
        transport.execute_list_view_filter(r, || true).await,
        Err(ListViewFilterFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_view_sort(request(address), || true)
            .await,
        Err(ListViewFilterFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

mod number;
