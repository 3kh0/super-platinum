use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    let session = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "fixture-token".into(),
    };
    crate::slack::api::lists_records_list(&crate::slack::SlackClient::default(), &session, "F1")
}
#[test]
fn records_read_is_exact_without_guessed_pagination_or_mutation_fields() {
    let address = "127.0.0.1:12345".parse().unwrap();
    assert!(allowed_records(&request(address)));
    for extra in ["cursor", "limit", "cells", "options"] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields.push((extra.into(), "wrong".into()));
        assert!(!allowed_records(&r));
    }
    for index in 0..6 {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        let duplicate = fields[index].clone();
        fields.push(duplicate);
        assert!(!allowed_records(&r));
    }
}
#[tokio::test]
async fn actual_current_records_request_uses_one_executor_and_preserves_unknown_page() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let expected = serde_json::json!({"ok":true,"list_id":"F1","records":[],"response_metadata":{"next_cursor":"opaque+/="},"future":{"keep":[true,null]}});
    let response = expected.to_string();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let header = std::str::from_utf8(&bytes[..end]).unwrap();
                assert!(header.starts_with("POST /api/lists.records.list"));
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    let fields = url::form_urlencoded::parse(&bytes[end + 4..end + 4 + length])
                        .into_owned()
                        .collect::<Vec<_>>();
                    assert_eq!(
                        fields,
                        vec![
                            ("token".into(), "fixture-token".into()),
                            ("list_id".into(), "F1".into()),
                            ("include_subtasks".into(), "true".into()),
                            ("archived".into(), "false".into()),
                            ("include_suggested".into(), "false".into()),
                            ("_x_reason".into(), "get-list-records".into())
                        ]
                    );
                    break;
                }
            }
            assert!(bytes.len() < 16384);
            let mut chunk = [0; 4096];
            let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut chunk))
                .await
                .unwrap()
                .unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
        }
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                )
                .as_bytes(),
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
    let actual = Transport::new("fixture-cookie")
        .unwrap()
        .execute_list_board_choice_read(request(address), || true)
        .await
        .unwrap();
    assert_eq!(actual, expected);
    let page = crate::slack::api::parse_list_records(actual, "F1").unwrap();
    assert_eq!(page.raw, expected);
    server.await.unwrap();
}
