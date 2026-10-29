use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        SlackClient,
        api::file_pages::{Input, Sort},
    },
};
use std::{cell::Cell, time::Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> Request {
    let workspace = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    };
    let mut r = file_pages::request(
        &SlackClient::default(),
        &workspace,
        Input {
            query: "notes \u{e9}",
            page: 2,
            query_session: uuid::Uuid::from_u128(1),
            search_session: uuid::Uuid::from_u128(2),
            sort: Sort::Newest,
            exclude_bots: Some(false),
            only_my_channels: None,
            primary_channel: "CONE",
        },
    )
    .unwrap();
    r.inner.url = format!("http://{address}/api/search.modules.files");
    r.destination = "127.0.0.1".into();
    r
}
async fn read(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut buf = [0; 4096];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() < 65536);
        if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (k, v) = line.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() == end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
#[tokio::test]
async fn file_search_actual_multipart_has_created_sort_present_false_and_no_redirect_or_legacy_fallback()
 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let r = request(listener.local_addr().unwrap());
    let transport = Transport::new("synthetic-cookie").unwrap();
    let observed = Cell::new(0);
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let wire = read(&mut socket).await;
        let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
        assert!(headers.starts_with("POST /api/search.modules.files "));
        assert!(
            headers
                .to_lowercase()
                .contains("content-type: multipart/form-data; boundary=")
        );
        assert!(
            headers
                .to_lowercase()
                .contains("cookie: d=synthetic-cookie")
        );
        assert!(!body.contains("filename="));
        for (name, value) in [
            ("token", "synthetic-token"),
            ("sort", "created"),
            ("sort_dir", "desc"),
            ("page", "2"),
            ("count", "20"),
            ("search_exclude_bots", "false"),
            ("search_only_team", ""),
        ] {
            assert!(body.contains(&format!("name=\"{name}\"\r\n\r\n{value}\r\n")));
        }
        assert!(!body.contains("name=\"search_only_my_channels\""));
        socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    };
    let client =
        transport.execute_file_page_current(&r, || true, |_| observed.set(observed.get() + 1));
    let (result, ()) = tokio::join!(client, server);
    assert!(matches!(result, Err(Error::HttpStatus { status: 302, .. })));
    assert_eq!(observed.get(), 1);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn stale_file_response_still_observes_retry_after_but_cannot_publish_and_wrong_endpoint_never_sends()
 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut r = request(listener.local_addr().unwrap());
    let transport = Transport::new("synthetic-cookie").unwrap();
    let current = Cell::new(true);
    let observed = Cell::new(0);
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        read(&mut socket).await;
        current.set(false);
        socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 71\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    };
    let client = transport.execute_file_page_current(
        &r,
        || current.get(),
        |result| {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(71)
                })
            ));
            observed.set(observed.get() + 1);
        },
    );
    let (result, ()) = tokio::join!(client, server);
    assert!(matches!(result, Err(Error::Api(_))));
    assert_eq!(observed.get(), 1);
    r.inner.url = r.inner.url.replace("search.modules.files", "search.files");
    assert!(
        transport
            .execute_file_page_current(&r, || true, |_| panic!("no dispatch"))
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn file_search_success_json_rate_limit_oversized_body_and_real_request_timeout_are_bounded() {
    for mode in 0..5 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let r = request(listener.local_addr().unwrap());
        let transport = Transport::new("synthetic-cookie").unwrap();
        let observed = Cell::new(0);
        let (release, hold) = tokio::sync::oneshot::channel::<()>();
        let server = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            read(&mut socket).await;
            match mode {
                0=>socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap(),
                1=>{
                    let body=r#"{"ok":false,"error":"ratelimited"}"#;
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nRetry-After: 23\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }
                2=>{
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",file_pages::MAX_REPLY_BYTES+1).as_bytes()).await.unwrap();
                    // Keep the peer open without any body until AFTER the
                    // independently bounded client completes. EOF cannot pass.
                    let _=hold.await;
                }
                3=>{
                    let body=format!("{{\"ok\":true,\"padding\":\"{}\"}}","x".repeat(file_pages::MAX_REPLY_BYTES));
                    assert!(body.len()>file_pages::MAX_REPLY_BYTES);
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
                    // Valid oversized JSON with NO content-length. Removing the
                    // streamed limit would accept it, rather than hit EOF/JSON.
                    let wire=format!("{:x}\r\n{}\r\n0\r\n\r\n",body.len(),body);
                    let _=socket.write_all(wire.as_bytes()).await;
                }
                4=>{
                    // No server deadline or EOF can manufacture a timeout.
                    // Only client completion (or failed outer assertion) drops it.
                    let _=hold.await;
                }
                _=>unreachable!(),
            }
        };
        let client = async {
            let started = Instant::now();
            let limit = if mode == 4 {
                Duration::from_secs(35)
            } else {
                Duration::from_secs(5)
            };
            let result = tokio::time::timeout(
                limit,
                transport.execute_file_page_current(
                    &r,
                    || true,
                    |_| observed.set(observed.get() + 1),
                ),
            )
            .await
            .expect("client must enforce size/time bound while peer remains open");
            if mode == 4 {
                assert!(
                    started.elapsed() >= Duration::from_secs(29)
                        && started.elapsed() < Duration::from_secs(35)
                );
            }
            let _ = release.send(());
            result
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(40), async {
            tokio::join!(client, server)
        })
        .await
        .expect("bounded offline exchange");
        assert_eq!(observed.get(), 1);
        match mode {
            0 => assert_eq!(result.unwrap(), serde_json::json!({"ok":true})),
            1 => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(23)
                })
            )),
            2 | 3 => assert!(matches!(result, Err(Error::Transport(_)))),
            4 => assert!(matches!(result, Err(Error::Offline(_)))),
            _ => unreachable!(),
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err(),
            "no retry or alternate endpoint"
        );
    }
}
