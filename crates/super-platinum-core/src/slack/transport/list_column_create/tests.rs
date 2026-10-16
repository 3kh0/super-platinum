use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/lists.columns.create"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("view_id".into(), "V1".into()),
            ("name".into(), "Notes".into()),
            ("type".into(), "text".into()),
            ("options".into(), r#"{"format":"text"}"#.into()),
            ("_x_reason".into(), "create-column".into()),
        ]),
    }
}
#[tokio::test]
async fn actual_capture_permit_builder_and_transport_preserve_exact_typed_forms_once() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{
            ColumnCreate, ColumnCreateAttempt, ColumnCreateKind, Document, PersonalTaskLists,
            RecordsPage, SelectCreateOptions,
        },
    };
    for (kind, wire_type, wire_options) in [
        (ColumnCreateKind::Text, "text", Some(r#"{"format":"text"}"#)),
        (
            ColumnCreateKind::NumberDefault0,
            "number",
            Some(r#"{"precision":0}"#),
        ),
        (ColumnCreateKind::Checkbox, "checkbox", None),
        (
            ColumnCreateKind::Select,
            "select",
            Some(
                r#"{"choices":[{"color":"cyan","label":"Ready","value":"OptABCDEFGH"}],"format":"single_select"}"#,
            ),
        ),
        (
            ColumnCreateKind::Select,
            "select",
            Some(
                r#"{"choices":[{"color":"green","label":"Second","value":"Opt12345678"},{"color":"cyan","label":"Ready","value":"OptABCDEFGH"}],"format":"multi_select"}"#,
            ),
        ),
        (
            ColumnCreateKind::DateSingleDefault,
            "date",
            Some(r#"{"date_format":"default","format":"single"}"#),
        ),
    ] {
        for name in ["Notes", "Café & 東京"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","user":"U1","access":"write","list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[{"id":"C1","key":"NAME","name":"Title","type":"text","options":{"format":"text"}}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[]}]}})).unwrap();
            let records = RecordsPage::parse(
            serde_json::json!({"ok":true,"records":[],"response_metadata":{"next_cursor":"NEXT"}}),
        )
        .unwrap();
            let personal = PersonalTaskLists::from_response(
                &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
            )
            .unwrap();
            let intent = if kind == ColumnCreateKind::Select {
                ColumnCreate::capture_select(
                    &document,
                    &records,
                    "V1",
                    name,
                    SelectCreateOptions::from_wire(
                        &serde_json::from_str(wire_options.unwrap()).unwrap(),
                    )
                    .unwrap(),
                    "U1",
                    Some("full"),
                    Some(&personal),
                )
            } else {
                ColumnCreate::capture_kind(
                    &document,
                    &records,
                    "V1",
                    name,
                    kind,
                    "U1",
                    Some("full"),
                    Some(&personal),
                )
            }
            .unwrap();
            let mut attempt = ColumnCreateAttempt::new(intent);
            let permit = attempt
                .admit(&document, &records, "U1", Some("full"), Some(&personal))
                .unwrap();
            let session = WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U1".into(),
                enterprise_id: None,
                name: "Fixture".into(),
                url: format!("http://{address}"),
                token: "fixture-token".into(),
            };
            let request = api::list_column_create(&SlackClient::default(), &session, permit);
            let RequestBody::Form(fields) = &request.body else {
                panic!()
            };
            let mut exact = vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F1".into()),
                ("view_id".into(), "V1".into()),
                ("name".into(), name.into()),
                ("type".into(), wire_type.into()),
            ];
            if let Some(options) = wire_options {
                exact.push(("options".into(), options.into()));
            }
            exact.push(("_x_reason".into(), "create-column".into()));
            assert_eq!(fields, &exact);
            assert_eq!(
                fields.len(),
                if kind == ColumnCreateKind::Checkbox {
                    6
                } else {
                    7
                }
            );
            assert!(
                !fields
                    .iter()
                    .any(|(k, _)| matches!(k.as_str(), "position" | "id" | "key" | "backfill"))
            );
            assert!(
                attempt
                    .admit(&document, &records, "U1", Some("full"), Some(&personal))
                    .is_err()
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
                assert!(header.starts_with("POST /api/lists.columns.create"));
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
                    .execute_list_column_create(request, || true)
                    .await
                    .unwrap(),
                serde_json::json!({"ok":true})
            );
            server.await.unwrap();
        }
    }
}
#[tokio::test]
async fn extra_position_wrong_type_malformed_options_and_foreign_wrapper_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for (index, value) in [
        (1, ""),
        (2, "bad\nview"),
        (3, " Notes "),
        (3, "bad\nname"),
        (4, "number"),
        (5, "null"),
        (5, "{}"),
        (5, r#"{"format":"rich_text"}"#),
        (5, r#"{"format":"text","extra":true}"#),
        (5, r#"{"format":"rich_text","format":"text"}"#),
        (6, "update-column"),
    ] {
        let mut r = request(addr);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[index].1 = value.into();
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    for extra in ["position", "id", "key", "backfill"] {
        let mut r = request(addr);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields.push((extra.into(), "x".into()));
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    let mut bad = request(addr);
    bad.url = format!("http://{addr}/api/lists.columns.update");
    assert!(matches!(
        transport.execute_list_column_create(bad, || true).await,
        Err(ListColumnCreateFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_column_create(request(addr), || false)
            .await,
        Err(ListColumnCreateFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_column_rename(request(addr), || true)
            .await,
        Err(ListColumnCreateFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_view_create(request(addr), || true)
            .await,
        Err(ListColumnCreateFailure::NotRequested(_))
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
            .execute_list_column_create(request(address), || true)
            .await;
        if body.is_some() {
            assert!(matches!(
                result,
                Err(ListColumnCreateFailure::Unconfirmed(Error::RateLimited {
                    retry_after_secs: Some(2)
                }))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ListColumnCreateFailure::Unconfirmed(_))
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
        .execute_list_column_create(request(address), || live.load(Ordering::SeqCst))
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
        let result=Transport::new("fixture-cookie").unwrap().execute_list_column_create(request(addr),||true).await;
        assert!(matches!(result,Err(ListColumnCreateFailure::Unconfirmed(_))));server.await.unwrap();
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
            .execute_list_column_create(request(address), || {
                checks += 1;
                checks == 1
            })
            .await,
        Err(ListColumnCreateFailure::NotRequested(_))
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
        transport.execute_list_column_create(foreign, || true).await,
        Err(ListColumnCreateFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_field_visibility(request(address), || true)
            .await,
        Err(ListColumnCreateFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_column_create(request(address), || true)
            .await,
        Err(ListColumnCreateFailure::NotRequested(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn type_options_cross_pairs_nonzero_and_opaque_number_options_refuse_before_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for (kind, options) in [
        ("text", r#"{"precision":0}"#),
        ("number", r#"{"format":"text"}"#),
        ("number", r#"{"precision":1}"#),
        ("number", r#"{"precision":"0"}"#),
        ("number", r#"{"precision":false}"#),
        ("number", r#"{"precision":0.0}"#),
        ("number", r#"{"precision":0,"default":0}"#),
        ("number", r#"{"precision":1,"precision":0}"#),
        ("number", "null"),
        ("number", "{}"),
        ("currency", r#"{"precision":0}"#),
    ] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[4].1 = kind.into();
        fields[5].1 = options.into();
        assert!(
            matches!(
                transport.execute_list_column_create(r, || true).await,
                Err(ListColumnCreateFailure::NotRequested(_))
            ),
            "{kind} {options}"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

mod checkbox;
mod date;

mod select;
