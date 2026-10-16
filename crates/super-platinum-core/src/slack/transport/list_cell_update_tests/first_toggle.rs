use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{CheckboxEdit, Document, RecordsPage},
};
fn built(address: std::net::SocketAddr, initial: Option<bool>, desired: bool) -> PreparedRequest {
    let d=Document::parse(serde_json::json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[{"id":"C1","key":"done","name":"Done","type":"checkbox"}],"views":[]}})).unwrap();
    let fields: Vec<Value> = initial
        .map(|v| serde_json::json!({"key":"done","checkbox":v}))
        .into_iter()
        .collect();
    let p =
        RecordsPage::parse(serde_json::json!({"ok":true,"records":[{"id":"R1","fields":fields}]}))
            .unwrap();
    let edit = CheckboxEdit::capture(&d, &p, "R1", "C1", Some("full"), desired).unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "fixture-token".into(),
    };
    let request = api::list_checkbox_update(&SlackClient::default(), &session, &edit);
    let RequestBody::Form(actual) = &request.body else {
        panic!()
    };
    let cells =
        serde_json::json!([{"row_id":"R1","column_id":"C1","checkbox":desired}]).to_string();
    assert_eq!(
        actual,
        &vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("cells".into(), cells),
            ("_x_reason".into(), "bulk-update-cells".into())
        ]
    );
    request
}
async fn read_body(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut all = Vec::new();
    let start = loop {
        let mut buf = [0; 2048];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        all.extend_from_slice(&buf[..n]);
        if let Some(at) = all.windows(4).position(|s| s == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let header = String::from_utf8(all[..start].to_vec()).unwrap();
    assert!(header.starts_with("POST /api/lists.cells.update"));
    let length = header
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    while all.len() < start + length {
        let mut buf = [0; 2048];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        all.extend_from_slice(&buf[..n]);
    }
    all[start..start + length].to_vec()
}
#[tokio::test]
async fn absent_first_true_then_explicit_false_use_real_builder_and_one_attempt_transport() {
    for (initial, desired) in [(None, true), (Some(true), false)] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let request = built(address, initial, desired);
        let RequestBody::Form(fields) = &request.body else {
            panic!()
        };
        let expected = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            assert_eq!(read_body(&mut socket).await, expected.as_bytes());
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
                .execute_list_cell_update(request)
                .await
                .unwrap()["ok"],
            true
        );
        server.await.unwrap();
    }
}
#[tokio::test]
async fn first_toggle_offline_rate_limited_and_lost_reply_never_replay() {
    for reply in [
        Some(
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 7\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ),
        None,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _ = read_body(&mut socket).await;
            if let Some(reply) = reply {
                socket.write_all(reply.as_bytes()).await.unwrap();
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
            .execute_list_cell_update(built(address, None, true))
            .await;
        if reply.is_some() {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            ));
        } else {
            assert!(result.is_err());
        }
        server.await.unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_cell_update(built(address, None, true))
            .await,
        Err(Error::Offline(_))
    ));
    transport.set_health(Health::Online);
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
