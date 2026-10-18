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
    crate::slack::SlackClient::default().rest_form(&session,"lists.cells.update",vec![("list_id","F1".into()),("cells",serde_json::json!([{"row_id":"R1","column_id":"C1","select":[]},{"row_id":"R2","column_id":"C1","select":[]}]).to_string()),("_x_reason","bulk-update-cells".into())])
}
#[test]
fn validator_requires_nonempty_unique_same_column_exact_clear() {
    let address = "127.0.0.1:12345".parse().unwrap();
    assert!(allowed(&request(address)));
    for bad in [
        serde_json::json!([]),
        serde_json::json!([{"row_id":"R1","column_id":"C1","select":["other"]}]),
        serde_json::json!([{"row_id":"R1","column_id":"C1","select":[]},{"row_id":"R1","column_id":"C1","select":[]}]),
        serde_json::json!([{"row_id":"R1","column_id":"C1","select":[]},{"row_id":"R2","column_id":"C2","select":[]}]),
        serde_json::json!([{"row_id":"R1","column_id":"C1","select":[],"unknown":true}]),
    ] {
        let mut r = request(address);
        let RequestBody::Form(f) = &mut r.body else {
            panic!()
        };
        f[2].1 = bad.to_string();
        assert!(!allowed(&r));
    }
    let mut r = request(address);
    let RequestBody::Form(f) = &mut r.body else {
        panic!()
    };
    f.push(("cursor".into(), "guessed".into()));
    assert!(!allowed(&r));
}
#[tokio::test]
async fn actual_bulk_clear_is_one_request_and_preserves_unknown_reply() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let expected = serde_json::json!({"ok":true,"cells":[],"opaque":{"keep":[true,null]}});
    let response = expected.to_string();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = std::str::from_utf8(&bytes[..end]).unwrap();
                assert!(header.starts_with("POST /api/lists.cells.update"));
                let length: usize = header
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    let fields = url::form_urlencoded::parse(&bytes[end + 4..end + 4 + length])
                        .into_owned()
                        .collect::<Vec<_>>();
                    let RequestBody::Form(expected) = request(address).body else {
                        panic!()
                    };
                    assert_eq!(fields, expected);
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
        .execute_list_board_group_delete_cells(request(address), || true)
        .await
        .unwrap();
    assert_eq!(actual, expected);
    server.await.unwrap();
}
#[tokio::test]
async fn retired_clear_is_not_requested() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        transport
            .execute_list_board_group_delete_cells(request(listener.local_addr().unwrap()), || {
                false
            })
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
