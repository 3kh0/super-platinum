use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr, _case: u8) -> PreparedRequest {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{
            BoardChoiceAction, BoardChoiceAttempt, BoardChoiceChange, Document, PersonalTaskLists,
        },
    };
    let document=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{
        "schema":[{"id":"C1","key":"status","name":"Status","type":"select","options":{"choices":[{"value":"a","label":"Before","color":"blue"}],"opaque":{"keep":true}}}],
        "views":[{"id":"V1","name":"Board","type":"board","grouping":{"group_by":"status"}}]}})).unwrap();
    let ordinary = PersonalTaskLists::from_response(
        &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    let intent = BoardChoiceChange::capture(
        &document,
        "V1",
        BoardChoiceAction::Rename {
            value: "a".into(),
            label: "After".into(),
        },
        Some("full"),
        Some(&ordinary),
    )
    .unwrap();
    let permit = BoardChoiceAttempt::new(intent)
        .admit(&document, Some("full"), Some(&ordinary))
        .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "fixture-token".into(),
    };
    api::list_board_choice(&SlackClient::default(), &session, permit)
}
fn fields(request: &mut PreparedRequest) -> &mut Vec<(String, String)> {
    let RequestBody::Form(fields) = &mut request.body else {
        panic!()
    };
    fields
}
async fn read_form(stream: &mut tokio::net::TcpStream) -> Vec<(String, String)> {
    let mut bytes = Vec::new();
    loop {
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let head = std::str::from_utf8(&bytes[..end]).unwrap();
            assert!(head.starts_with("POST /api/lists.columns.update"));
            let length: usize = head
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length < 32 * 1024);
            if bytes.len() >= end + 4 + length {
                return url::form_urlencoded::parse(&bytes[end + 4..end + 4 + length])
                    .into_owned()
                    .collect();
            }
        }
        assert!(bytes.len() < 32 * 1024);
        let mut chunk = [0; 4096];
        let read = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut chunk))
            .await
            .unwrap()
            .unwrap();
        assert!(read > 0);
        bytes.extend_from_slice(&chunk[..read]);
    }
}
async fn respond(stream: &mut tokio::net::TcpStream, status: u16, extra: &str, body: &str) {
    stream.write_all(format!("HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",body.len()).as_bytes()).await.unwrap();
}
async fn no_replay(listener: &tokio::net::TcpListener) {
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn exact_options_validator_refuses_duplicate_keys_lossy_defaults_and_extra_fields() {
    let address = "127.0.0.1:12345".parse().unwrap();
    assert!(allowed(&request(address, 0)));
    for options in [
        r#"{"choices":[] ,"choices":[]}"#,
        r#"{"choices":[],"default_value_typed":null}"#,
        r#"{"choices":[],"emoji_url":"output"}"#,
        r#"{"choices":[],"default_value_typed":{"rating":[2]}}"#,
        r#"{"choices":[{"value":"x","label":"A"},{"value":"x","label":"B"}]}"#,
    ] {
        let mut req = request(address, 0);
        fields(&mut req)[3].1 = options.into();
        assert!(!allowed(&req));
    }
    for index in 0..6 {
        let mut req = request(address, 0);
        let duplicate = fields(&mut req)[index].clone();
        fields(&mut req).push(duplicate);
        assert!(!allowed(&req));
        let mut req = request(address, 0);
        fields(&mut req).remove(index);
        assert!(!allowed(&req));
    }
    for key in ["name", "type", "id", "view_id", "cursor"] {
        let mut req = request(address, 0);
        fields(&mut req).push((key.into(), "extra".into()));
        assert!(!allowed(&req));
    }
}
#[tokio::test]
async fn invalid_endpoint_origin_or_retired_owner_and_offline_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    let mut bad = Vec::new();
    for tail in [
        "lists.cells.update",
        "lists.columns.update/",
        "lists.columns.update?cursor=guessed",
    ] {
        let mut req = request(address, 2);
        req.url = format!("http://{address}/api/{tail}");
        bad.push(req);
    }
    for url in [
        "https://foreign.example/api/lists.columns.update",
        "https://slack.com.evil.example/api/lists.columns.update",
        "http://example.slack.com/api/lists.columns.update",
        "https://example.slack.com:444/api/lists.columns.update",
        "https://name@example.slack.com/api/lists.columns.update",
        "https://example.slack.com/api/lists.columns.update#fragment",
    ] {
        let mut req = request(address, 2);
        req.url = url.into();
        bad.push(req);
    }
    let mut req = request(address, 2);
    req.method = "GET".into();
    bad.push(req);
    let mut req = request(address, 2);
    fields(&mut req).push(("type".into(), "number".into()));
    bad.push(req);
    for req in bad {
        assert!(matches!(
            transport.execute_list_board_choice(req, || true).await,
            Err(ListBoardChoiceFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_board_choice(request(address, 2), || false)
            .await,
        Err(ListBoardChoiceFailure::NotRequested(_))
    ));
    let mut calls = 0;
    assert!(matches!(
        transport
            .execute_list_board_choice(request(address, 2), || {
                calls += 1;
                calls == 1
            })
            .await,
        Err(ListBoardChoiceFailure::NotRequested(_))
    ));
    assert_eq!(calls, 2);
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_board_choice(request(address, 2), || true)
            .await,
        Err(ListBoardChoiceFailure::NotRequested(_))
    ));
    no_replay(&listener).await;
}
#[tokio::test]
async fn actual_builder_form_is_sent_once_and_raw_api_denial_is_retained() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let fields = read_form(&mut stream).await;
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F1".into()),
                ("key".into(), "status".into()),
                ("options".into(), serde_json::json!({"choices":[{"value":"a","label":"After","color":"blue"}],"opaque":{"keep":true}}).to_string()),
                ("backfill_missing_cells_with_default".into(), "true".into()),
                ("_x_reason".into(), "update-column".into())
            ]
        );
        respond(
            &mut stream,
            200,
            "",
            "{\"ok\":false,\"error\":\"no_permission\",\"opaque\":1}",
        )
        .await;
        drop(stream);
        no_replay(&listener).await;
    });
    let result = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_board_choice(request(address, 2), || true)
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({"ok":false,"error":"no_permission","opaque":1})
    );
    server.await.unwrap();
}
#[tokio::test]
async fn retry_after_and_redirect_never_replay_or_follow_and_retain_known_limit() {
    for status in [200, 302, 429, 503] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_form(&mut stream).await;
            let extra = if status == 302 {
                format!("Location: http://{address}/api/lists.columns.update\r\n")
            } else {
                "Retry-After: 7\r\n".into()
            };
            respond(&mut stream, status, &extra, "{\"ok\":true}").await;
            drop(stream);
            no_replay(&listener).await;
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_board_choice(request(address, 2), || true)
            .await;
        match result {
            Err(ListBoardChoiceFailure::Unconfirmed(Error::RateLimited {
                retry_after_secs: Some(7),
            })) if status == 429 => {}
            Err(ListBoardChoiceFailure::Unconfirmed(Error::HttpStatus {
                status: actual,
                retry_after_secs: limit,
            })) if actual == status => {
                assert_eq!(limit, if status == 302 { None } else { Some(7) })
            }
            other => panic!("unexpected {other:?}"),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn duplicate_json_nonobject_and_lost_response_remain_unconfirmed_without_retry() {
    for body in [
        Some("{\"ok\":false,\"ok\":true}"),
        Some("{\"ok\":true,\"nested\":{\"x\":1,\"x\":2}}"),
        Some("[]"),
        None,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_form(&mut stream).await;
            if let Some(body) = body {
                respond(&mut stream, 200, "", body).await;
            }
            drop(stream);
            no_replay(&listener).await;
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_board_choice(request(address, 2), || true)
            .await;
        assert!(matches!(
            result,
            Err(ListBoardChoiceFailure::Unconfirmed(_))
        ));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn original_response_survives_owner_retirement_for_receipt_only() {
    use std::sync::atomic::AtomicBool;
    let live = Arc::new(AtomicBool::new(true));
    let retired = live.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_form(&mut stream).await;
        retired.store(false, Ordering::SeqCst);
        respond(&mut stream, 200, "", "{\"ok\":true,\"opaque\":1}").await;
        drop(stream);
        no_replay(&listener).await;
    });
    let result = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_board_choice(request(address, 2), || live.load(Ordering::SeqCst))
        .await
        .unwrap();
    assert_eq!(result, serde_json::json!({"ok":true,"opaque":1}));
    assert!(!live.load(Ordering::SeqCst));
    server.await.unwrap();
}
