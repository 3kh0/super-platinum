use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request(address: std::net::SocketAddr) -> PreparedRequest {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api},
        slack_lists::{
            Document, PersonalTaskLists, ViewReorder, ViewReorderAttempt, ViewReorderDirection,
        },
    };
    let doc=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[],"views":[
        {"id":"V_β","name":"Board","type":"board","position":"5000000000"},
        {"id":"V2","name":"Table","type":"table","position":"5000000001"},
        {"id":"V_ALL","name":"All","type":"table","default_view_key":"all_items","position":"5000000002"}]}})).unwrap();
    let personal = PersonalTaskLists::from_response(
        &serde_json::json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    let intent = ViewReorder::capture(
        &doc,
        "V_β",
        ViewReorderDirection::Down,
        Some("full"),
        Some(&personal),
    )
    .unwrap();
    let permit = ViewReorderAttempt::new(intent)
        .admit(&doc, Some("full"), Some(&personal))
        .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "fixture-token".into(),
    };
    api::list_view_reorder(&SlackClient::default(), &session, permit)
}
async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 1024];
        let read = stream.read(&mut chunk).await.unwrap();
        assert!(read > 0);
        bytes.extend_from_slice(&chunk[..read]);
        assert!(bytes.len() < 16 * 1024);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
#[test]
fn builder_has_exact_position_only_form_in_order_and_no_settings_payload() {
    let req = request("127.0.0.1:80".parse().unwrap());
    assert_eq!(req.method, "POST");
    let RequestBody::Form(fields) = req.body else {
        panic!("not form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("id".into(), "V_β".into()),
            ("position".into(), "5000000001.V".into()),
            ("_x_reason".into(), "update-view".into())
        ]
    );
}
#[tokio::test]
async fn malformed_foreign_cross_operation_offline_and_retired_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for case in 0..13 {
        let mut req = request(address);
        match case {
            0 => req.method = "GET",
            1 => req.url = "https://foreign.example/api/lists.views.update".into(),
            2 => req.url = format!("http://{address}/api/lists.views.update?cursor=guess"),
            3 => req.url = format!("http://{address}/api/lists.columns.update"),
            4 => req.url = format!("http://{address}/api/lists.views.update/"),
            5 => req.url = format!("http://{address}/api/lists.views.update#fragment"),
            _ => {
                let RequestBody::Form(fields) = &mut req.body else {
                    panic!()
                };
                match case {
                    6 => fields.push(("position".into(), "5000000000".into())),
                    7 => fields[3].0 = "sorts".into(),
                    8 => fields[3].1 = "1".into(),
                    9 => fields[3].1 = "5000000000.V0".into(),
                    10 => fields[4].1 = "rename-view".into(),
                    11 => fields[2].1 = "\n".into(),
                    _ => fields[0].1 = "".into(),
                }
            }
        }
        assert!(matches!(
            transport.execute_list_view_reorder(req, || true).await,
            Err(ListViewReorderFailure::NotRequested(_))
        ));
    }
    assert!(matches!(
        transport
            .execute_list_view_reorder(request(address), || false)
            .await,
        Err(ListViewReorderFailure::NotRequested(_))
    ));
    let mut calls = 0;
    assert!(matches!(
        transport
            .execute_list_view_reorder(request(address), || {
                calls += 1;
                calls == 1
            })
            .await,
        Err(ListViewReorderFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_view_reorder(request(address), || true)
            .await,
        Err(ListViewReorderFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn actual_loopback_posts_one_exact_form_and_retains_success_body() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let wire = read_request(&mut stream).await;
        assert!(wire.starts_with("POST /api/lists.views.update"));
        let body = wire.split_once("\r\n\r\n").unwrap().1;
        let fields = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect::<Vec<_>>();
        let RequestBody::Form(expected) = request(address).body else {
            panic!()
        };
        assert_eq!(fields, expected);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            )
            .await
            .unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    });
    let response = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_view_reorder(request(address), || true)
        .await
        .unwrap();
    assert_eq!(response, serde_json::json!({"ok":true}));
    server.await.unwrap();
}
#[tokio::test]
async fn lost_reply_rate_limit_and_duplicate_json_never_replay() {
    for case in 0..3 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_request(&mut stream).await;
            if case == 1 {
                stream.write_all(b"HTTP/1.1 429 Limited\r\nRetry-After: 7\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            }
            if case == 2 {
                let body = r#"{"ok":false,"ok":true}"#;
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_view_reorder(request(address), || true)
            .await;
        if case == 1 {
            assert!(matches!(
                result,
                Err(ListViewReorderFailure::Unconfirmed(Error::RateLimited {
                    retry_after_secs: Some(7)
                }))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ListViewReorderFailure::Unconfirmed(_))
            ));
        }
        server.await.unwrap();
    }
}
