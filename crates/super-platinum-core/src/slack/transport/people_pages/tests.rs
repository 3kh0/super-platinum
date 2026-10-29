use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        SlackClient,
        api::people_pages::{Input, Sort},
    },
};
use std::cell::{Cell, RefCell};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> Request {
    let session = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    };
    let mut request = people_pages::request(
        &SlackClient::default(),
        &session,
        Input {
            query: "Elodie \u{e9}\u{1f600}",
            page: 1,
            query_session: uuid::Uuid::from_u128(1),
            search_session: uuid::Uuid::from_u128(2),
            sort: Sort::Relevant,
            hide_deactivated: false,
            primary_channel: "CONE",
        },
    )
    .unwrap();
    // Test-only redirection of a validated closed request. No production setter.
    request.inner.url = format!("http://{address}/api/search.modules.people");
    request.destination = "127.0.0.1".into();
    request
}
async fn read(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() < 64 * 1024);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = header
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() == end + 4 + length {
                return bytes;
            }
        }
    }
}
async fn reply(socket: &mut tokio::net::TcpStream, status: &str, body: &str, extra: &str) {
    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",body.len()).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn actual_loopback_wire_is_token_first_text_multipart_with_empty_false_json_and_no_redirect_or_wrong_method()
 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let request = request(address);
    let transport = Transport::new("synthetic-cookie").unwrap();
    let observed = Cell::new(0);
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let bytes = read(&mut socket).await;
        let wire = String::from_utf8(bytes).unwrap();
        let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
        assert!(
            headers
                .lines()
                .next()
                .unwrap()
                .starts_with("POST /api/search.modules.people ")
        );
        let lower = headers.to_ascii_lowercase();
        assert!(lower.contains("content-type: multipart/form-data; boundary="));
        assert!(lower.contains("cookie: d=synthetic-cookie"));
        assert!(!body.contains("filename="));
        let content_type = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-type")
                    .then_some(value.trim())
            })
            .unwrap();
        let boundary = content_type
            .split(';')
            .find_map(|part| part.trim().strip_prefix("boundary="))
            .unwrap()
            .trim_matches('"');
        assert!(!boundary.is_empty());
        let delimiter = format!("--{boundary}");
        let parts: Vec<_> = body.split(&delimiter).collect();
        assert_eq!(parts.first(), Some(&""));
        assert_eq!(parts.last(), Some(&"--\r\n"));
        let names: Vec<_> = parts[1..parts.len() - 1]
            .iter()
            .map(|part| {
                let (header, _) = part
                    .strip_prefix("\r\n")
                    .unwrap()
                    .split_once("\r\n\r\n")
                    .unwrap();
                header
                    .strip_prefix("Content-Disposition: form-data; name=\"")
                    .unwrap()
                    .strip_suffix('"')
                    .unwrap()
            })
            .collect();
        assert_eq!(
            names,
            [
                "token",
                "module",
                "query",
                "page",
                "client_req_id",
                "search_session_id",
                "extracts",
                "highlight",
                "max_extract_len",
                "extra_message_data",
                "no_user_profile",
                "count",
                "file_title_only",
                "query_rewrite_disabled",
                "include_files_shares",
                "search_context",
                "tz",
                "external_shared_team_ids",
                "custom_fields",
                "hide_deactivated_users",
                "request_context",
                "spell_correction",
                "sort",
                "sort_dir",
                "search_only_team",
                "profile_location",
                "profile_title",
                "profile_division",
                "profile_department",
                "profile_celebration"
            ]
        );

        let token = body.find("name=\"token\"").unwrap();
        let module = body.find("name=\"module\"").unwrap();
        let query = body.find("name=\"query\"").unwrap();
        assert!(token < module && module < query);
        for (name, value) in [
            ("token", "synthetic-token"),
            ("module", "people"),
            ("query", "Elodie \u{e9}\u{1f600}"),
            ("tz", ""),
            ("file_title_only", "false"),
            ("hide_deactivated_users", "0"),
            ("custom_fields", "{}"),
            (
                "request_context",
                "{\"active_cid\":\"CONE\",\"recent_filter_in\":[],\"recent_filter_from\":[]}",
            ),
        ] {
            assert!(
                body.contains(&format!("name=\"{name}\"\r\n\r\n{value}\r\n")),
                "{name}"
            );
        }
        reply(
            &mut socket,
            "200 OK",
            r#"{"ok":true,"items":[],"pagination":{"page":1,"page_count":1,"total_count":0}}"#,
            "",
        )
        .await;
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            transport.execute_people_page_current(
                &request,
                || true,
                |_| observed.set(observed.get() + 1)
            ),
            server
        )
    })
    .await
    .expect("bounded complete People loopback exchange");
    assert!(result.is_ok());
    assert_eq!(observed.get(), 1);
    let redirected = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let _ = read(&mut socket).await;
        reply(
            &mut socket,
            "302 Found",
            "",
            &format!("Location: http://{address}/api/chat.postMessage\r\n"),
        )
        .await;
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            transport.execute_people_page_current(
                &request,
                || true,
                |_| observed.set(observed.get() + 1)
            ),
            redirected
        )
    })
    .await
    .expect("bounded complete People loopback exchange");
    assert!(matches!(result, Err(Error::HttpStatus { status: 302, .. })));
    assert_eq!(observed.get(), 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err(),
        "redirect must never send token or cookie"
    );
    let mut forbidden = request.clone();
    forbidden.inner.url = format!("http://{address}/api/chat.postMessage");
    assert!(!allowed(&forbidden));
    assert!(
        transport
            .execute_people_page_current(&forbidden, || true, |_| panic!("no dispatched response"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn actual_rate_reply_is_observed_before_owner_retirement_without_retry_or_page_publication() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let request = request(listener.local_addr().unwrap());
    let transport = Transport::new("synthetic-cookie").unwrap();
    let current = Cell::new(true);
    let observed = RefCell::new(Vec::new());
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let _ = read(&mut socket).await;
        current.set(false);
        reply(
            &mut socket,
            "429 Too Many Requests",
            r#"{"ok":false,"error":"ratelimited"}"#,
            "Retry-After: 9\r\n",
        )
        .await;
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            transport.execute_people_page_current(
                &request,
                || current.get(),
                |r| observed.borrow_mut().push(matches!(
                    r,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(9)
                    })
                ))
            ),
            server
        )
    })
    .await
    .expect("bounded complete People loopback exchange");
    assert!(matches!(result, Err(Error::Api(_))));
    assert_eq!(*observed.borrow(), [true]);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err(),
        "one actual attempt only"
    );
}

mod account_type;
