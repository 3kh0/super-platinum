use super::*;
use serde_json::json;
use std::{cell::Cell, sync::atomic::AtomicBool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const ID: &str = "F12345678";
const PDF: &[u8] = b"%PDF-1.7\ncontainer only; actual decode remains required\n%%EOF\n";
fn request(address: std::net::SocketAddr, ip: bool) -> Request {
    let w = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let c = crate::slack::SlackClient::default();
    let mut r = if ip {
        Request::ip(&c, &w, ID)
    } else {
        Request::metadata(&c, &w, ID)
    }
    .unwrap();
    r.inner.url = format!(
        "http://{address}/api/{}",
        if ip {
            "teams.authorized.ip.range.checkIp"
        } else {
            "files.info"
        }
    );
    r
}
fn projection(address: std::net::SocketAddr) -> Projection {
    Projection::from_fresh(&json!({"ok":true,"file":{"id":ID,"mode":"hosted","filetype":"pdf","mimetype":"application/pdf","url_private_download":format!("http://{address}/document.pdf"),"file_access":null,"is_hidden_by_limit":false,"restriction_type":0,"is_external":false,"updated":1}}),ID).unwrap()
}
async fn wire(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut block = [0; 4096];
        let n = socket.read(&mut block).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&block[..n]);
        assert!(bytes.len() < 65536);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let h = String::from_utf8_lossy(&bytes[..end]);
            let length = h
                .lines()
                .find_map(|v| {
                    let (k, v) = v.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
async fn reply(
    socket: &mut tokio::net::TcpStream,
    status: &str,
    headers: &str,
    body: &[u8],
    declared: Option<usize>,
) {
    socket
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                declared.unwrap_or(body.len())
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    socket.write_all(body).await.unwrap();
}
#[tokio::test]
async fn isolated_pdf_actual_form_multipart_content_reads_are_bounded_once_and_keep_late_outcomes()
{
    assert_eq!(pdf_retry_after(&HeaderMap::new()), None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for ip in [false, true] {
        let r = request(address, ip);
        assert!(allowed(&r));
        for bad in ["duplicate", "header", "host", "query", "path"] {
            let mut changed = Request {
                inner: r.inner.clone(),
                multipart: r.multipart,
            };
            match bad {
                "duplicate" => {
                    let RequestBody::Form(f) = &mut changed.inner.body else {
                        panic!()
                    };
                    f.push(("file".into(), ID.into()));
                }
                "header" => changed
                    .inner
                    .headers
                    .push(("Cookie".into(), "other".into())),
                "host" => changed.inner.url = "https://foreign.example/api/files.info".into(),
                "query" => changed.inner.url.push_str("?_x_id=1&_x_id=2"),
                _ => changed.inner.url = format!("http://{address}/api/reactions.add"),
            }
            assert!(!allowed(&changed));
            assert!(
                transport
                    .execute_file_pdf_current(&changed, || panic!(
                        "invalid purpose must refuse before owner/client/IO"
                    ))
                    .await
                    .is_err()
            );
        }
        let checks = Cell::new(0);
        assert!(matches!(
            transport
                .execute_file_pdf_current(&r, || {
                    checks.set(checks.get() + 1);
                    checks.get() == 1
                })
                .await,
            Err(Error::Offline(_))
        ));
        assert_eq!(checks.get(), 2);
        transport.set_health(Health::Offline);
        assert!(matches!(
            transport.execute_file_pdf_current(&r, || true).await,
            Err(Error::Offline(_))
        ));
        transport.set_health(Health::Online);
    }
    let p = projection(address);
    let checks = Cell::new(0);
    assert!(matches!(
        transport
            .get_file_pdf_current(&p, || {
                checks.set(checks.get() + 1);
                checks.get() == 1
            })
            .await,
        Err(Error::Offline(_))
    ));
    assert_eq!(checks.get(), 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), listener.accept())
            .await
            .is_err()
    );
    let cases: &[(bool, &str, &str, &[u8], Option<usize>, bool)] = &[
        (
            false,
            "200 OK",
            "",
            br#"{"ok":true,"file":{"id":"F12345678"}}"#,
            None,
            true,
        ),
        (
            true,
            "200 OK",
            "",
            br#"{"ok":true,"ip_allowed":true}"#,
            None,
            true,
        ),
        (
            true,
            "200 OK",
            "",
            br#"{"ok":true,"ip_allowed":true,"ip_allowed":false}"#,
            None,
            false,
        ),
        (
            false,
            "200 OK",
            "",
            br#"{"ok":true,"file":{"id":1,"id":2}}"#,
            None,
            false,
        ),
        (false, "200 OK", "", b"[]", None, false),
        (
            false,
            "302 Found",
            "Location: /forbidden\r\n",
            b"{}",
            None,
            false,
        ),
        (
            true,
            "429 Too Many Requests",
            "Retry-After: 17\r\n",
            b"{}",
            None,
            false,
        ),
        (
            false,
            "429 Too Many Requests",
            "Retry-After: not-a-duration\r\n",
            b"{}",
            None,
            false,
        ),
        (
            true,
            "429 Too Many Requests",
            "Retry-After: Wed, 21 Oct 2037 07:28:00 GMT\r\n",
            b"{}",
            None,
            false,
        ),
        (
            false,
            "429 Too Many Requests",
            "Retry-After: 18446744073709551616\r\n",
            b"{}",
            None,
            false,
        ),
        (
            false,
            "429 Too Many Requests",
            "Retry-After: 17\r\nRetry-After: 17\r\n",
            b"{}",
            None,
            false,
        ),
        (
            true,
            "429 Too Many Requests",
            "Retry-After: 17\r\nRetry-After: 43\r\n",
            b"{}",
            None,
            false,
        ),
        (
            false,
            "429 Too Many Requests",
            "Retry-After: 43\r\nRetry-After: 17\r\n",
            b"{}",
            None,
            false,
        ),
        (
            true,
            "429 Too Many Requests",
            "Retry-After: 17\r\nRetry-After: not-a-duration\r\n",
            b"{}",
            None,
            false,
        ),
        (
            false,
            "429 Too Many Requests",
            "Retry-After: not-a-duration\r\nRetry-After: 17\r\n",
            b"{}",
            None,
            false,
        ),
        (
            true,
            "429 Too Many Requests",
            "Retry-After: 17\r\nRetry-After: Wed, 21 Oct 2037 07:28:00 GMT\r\n",
            b"{}",
            None,
            false,
        ),
        (
            false,
            "429 Too Many Requests",
            "Retry-After: 17\r\nRetry-After: 18446744073709551616\r\n",
            b"{}",
            None,
            false,
        ),
        (
            false,
            "200 OK",
            "Retry-After: 18446744073709551615\r\n",
            b"{}",
            None,
            false,
        ),
        (false, "200 OK", "", b"", Some(MAX_METADATA + 1), false),
    ];
    for &(ip, status, headers, body, declared, success) in cases {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let r = request(listener.local_addr().unwrap(), ip);
        let current = AtomicBool::new(true);
        let server = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let wire = wire(&mut socket).await;
            let (h, b) = wire.split_once("\r\n\r\n").unwrap();
            assert!(h.to_ascii_lowercase().contains("cookie: d=fixture-cookie"));
            if ip {
                assert!(h.starts_with("POST /api/teams.authorized.ip.range.checkIp "));
                assert!(
                    h.to_ascii_lowercase()
                        .contains("content-type: multipart/form-data; boundary=")
                );
                assert!(
                    b.contains("name=\"token\"\r\n\r\nfixture-only")
                        && b.contains("name=\"file\"\r\n\r\nF12345678")
                );
                assert_eq!(
                    b.to_ascii_lowercase()
                        .matches("content-disposition:")
                        .count(),
                    2
                );
            } else {
                assert!(h.starts_with("POST /api/files.info "));
                assert_eq!(
                    b,
                    "token=fixture-only&file=F12345678&count=1&page=1&truncate=false"
                );
            }
            current.store(false, Ordering::Release);
            reply(&mut socket, status, headers, body, declared).await;
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(40), listener.accept())
                    .await
                    .is_err()
            );
        };
        let (result, ()) = tokio::join!(
            transport.execute_file_pdf_current(&r, || current.load(Ordering::Acquire)),
            server
        );
        assert_eq!(result.is_ok(), success);
        if success {
            assert_eq!(
                result.unwrap(),
                serde_json::from_slice::<Value>(body).unwrap()
            );
        } else if status.starts_with("429") {
            if headers == "Retry-After: 17\r\n" {
                assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(17)
                    })
                ));
            } else if matches!(
                headers,
                "Retry-After: 17\r\nRetry-After: 17\r\n"
                    | "Retry-After: 17\r\nRetry-After: 43\r\n"
                    | "Retry-After: 43\r\nRetry-After: 17\r\n"
            ) {
                let expected = if headers == "Retry-After: 17\r\nRetry-After: 17\r\n" {
                    17
                } else {
                    43
                };
                assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(n)
                    }) if n == expected
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(u64::MAX)
                    })
                ));
            }
        } else if headers.starts_with("Retry-After") {
            assert!(matches!(
                result,
                Err(Error::HttpStatus {
                    status: 200,
                    retry_after_secs: Some(u64::MAX)
                })
            ));
        }
        assert!(
            !current.load(Ordering::Acquire),
            "issued read is drained without pretending its owner remains current"
        );
    }
    for (status, headers, body, declared, success) in [
        ("200 OK", "", PDF, None, true),
        (
            "200 OK",
            "",
            b"<html>not PDF</html>".as_slice(),
            None,
            false,
        ),
        ("200 OK", "", b"%PDF-1.7\ntruncated".as_slice(), None, false),
        ("206 Partial Content", "", PDF, None, false),
        ("302 Found", "Location: /forbidden\r\n", PDF, None, false),
        (
            "429 Too Many Requests",
            "Retry-After: 19\r\n",
            b"".as_slice(),
            None,
            false,
        ),
        ("200 OK", "", b"".as_slice(), Some(MAX_BYTES + 1), false),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = projection(listener.local_addr().unwrap());
        let current = AtomicBool::new(true);
        let server = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let w = wire(&mut socket).await;
            assert!(
                w.starts_with("GET /document.pdf ")
                    && w.to_ascii_lowercase().contains("cookie: d=fixture-cookie")
            );
            assert!(!w.to_ascii_lowercase().contains("range:"));
            current.store(false, Ordering::Release);
            reply(&mut socket, status, headers, body, declared).await;
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(40), listener.accept())
                    .await
                    .is_err()
            );
        };
        let (result, ()) = tokio::join!(
            transport.get_file_pdf_current(&p, || current.load(Ordering::Acquire)),
            server
        );
        assert_eq!(result.is_ok(), success);
        if success {
            assert_eq!(
                result.unwrap(),
                PDF,
                "bytes are prechecked, never decoded/ready"
            );
        } else if status.starts_with("429") {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(19)
                })
            ));
        }
    }
}
