use super::*;
use serde_json::json;
use std::{cell::Cell, sync::atomic::AtomicBool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const CHANNEL: &str = "C12345678";
const TS: &str = "1700000000.123456";
fn session() -> crate::config::WorkspaceSession {
    crate::config::WorkspaceSession {
        team_id: "T12345678".into(),
        user_id: "U12345678".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn request(address: std::net::SocketAddr, kind: Kind) -> Request {
    let c = crate::slack::SlackClient::default();
    let s = session();
    let mut r = match kind {
        Kind::History => Request::history(&c, &s, CHANNEL, TS),
        Kind::Channel => Request::channel(&c, &s, CHANNEL, TS),
    }
    .unwrap();
    r.inner.prepared.url = format!(
        "http://{address}/api/{}",
        match kind {
            Kind::History => "conversations.history",
            Kind::Channel => "conversations.info",
        }
    );
    r
}
async fn wire(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut block = [0; 4096];
        let n = socket.read(&mut block).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&block[..n]);
        assert!(bytes.len() < 65536);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let len = headers
                .lines()
                .find_map(|s| {
                    let (k, v) = s.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            if bytes.len() >= end + 4 + len {
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
    let mut all = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        declared.unwrap_or(body.len())
    )
    .into_bytes();
    all.extend_from_slice(body);
    socket.write_all(&all).await.unwrap();
}
#[derive(Clone, Copy)]
enum Outcome {
    Raw,
    Rejected,
    Http(u16, Option<u64>),
    Rate(Option<u64>),
}
struct Case {
    kind: Kind,
    status: &'static str,
    headers: &'static str,
    body: &'static [u8],
    declared: Option<usize>,
    outcome: Outcome,
}

#[tokio::test]
async fn thread_timestamp_exact_owned_form_two_fences_and_late_bounded_single_attempt_outcomes() {
    let transport = Transport::new("fixture-cookie").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let client = crate::slack::SlackClient::default();
    let mut s = session();
    for (channel, ts) in [
        ("", TS),
        ("G12345678", TS),
        ("C_BAD", TS),
        (CHANNEL, "1700000000123456"),
        (CHANNEL, "1700000000.12345"),
    ] {
        assert!(Request::history(&client, &s, channel, ts).is_err());
        assert!(Request::channel(&client, &s, channel, ts).is_err());
    }
    for url in [
        "https://foreign.example",
        "http://fixture.slack.com",
        "https://u@fixture.slack.com",
        "https://fixture.slack.com:443",
        "https://fixture.slack.com/path",
    ] {
        s.url = url.into();
        assert!(Request::history(&client, &s, CHANNEL, TS).is_err());
    }
    s = session();
    s.token = " ".into();
    assert!(Request::channel(&client, &s, CHANNEL, TS).is_err());
    s.token = "a".repeat(16 * 1024 + 1);
    assert!(Request::history(&client, &s, CHANNEL, TS).is_err());
    for kind in [Kind::History, Kind::Channel] {
        let r = request(address, kind);
        assert!(allowed(&r));
        assert_eq!(r.kind(), kind);
        assert_eq!(r.channel_id(), CHANNEL);
        assert_eq!(r.timestamp(), TS);
        for bad in [
            "duplicate",
            "extra",
            "channel",
            "latest-or-members",
            "header",
            "method",
            "json",
            "query",
            "retry",
            "host",
            "path",
            "fragment",
        ] {
            let mut changed = request(address, kind);
            let p = &mut changed.inner.prepared;
            match bad {
                "duplicate" => {
                    let RequestBody::Form(f) = &mut p.body else {
                        panic!()
                    };
                    f.push(("token".into(), "other".into()));
                }
                "extra" => {
                    let RequestBody::Form(f) = &mut p.body else {
                        panic!()
                    };
                    f.push(("oldest".into(), TS.into()));
                }
                "channel" => {
                    let RequestBody::Form(f) = &mut p.body else {
                        panic!()
                    };
                    f.iter_mut().find(|(k, _)| k == "channel").unwrap().1 = "COTHER123".into();
                }
                "latest-or-members" => {
                    let RequestBody::Form(f) = &mut p.body else {
                        panic!()
                    };
                    let key = if kind == Kind::History {
                        "latest"
                    } else {
                        "include_num_members"
                    };
                    f.iter_mut().find(|(k, _)| k == key).unwrap().1 = if kind == Kind::History {
                        "1700000000.123457"
                    } else {
                        "true"
                    }
                    .into();
                }
                "header" => p.headers.push(("Cookie".into(), "other".into())),
                "method" => p.method = "GET",
                "json" => p.body = RequestBody::Json(json!({})),
                "query" => p.url.push_str("?_x_id=one&_x_id=two"),
                "retry" => p.url.push_str("?_x_num_retries=1"),
                "host" => p.url = "https://foreign.example/api/conversations.history".into(),
                "path" => p.url = format!("http://{address}/api/chat.postMessage"),
                _ => p.url.push_str("#fragment"),
            }
            assert!(!allowed(&changed), "{bad}");
            assert!(
                transport
                    .execute_thread_timestamp_current(&changed, || panic!(
                        "invalid purpose refuses before owner/client/IO"
                    ))
                    .await
                    .is_err()
            );
        }
        let calls = Cell::new(0);
        assert!(matches!(
            transport
                .execute_thread_timestamp_current(&r, || {
                    calls.set(calls.get() + 1);
                    calls.get() == 1
                })
                .await,
            Err(Error::Offline(_))
        ));
        assert_eq!(calls.get(), 2, "second owner fence occurs before dispatch");
        transport.set_health(Health::Offline);
        assert!(matches!(
            transport
                .execute_thread_timestamp_current(&r, || panic!("known offline is immediate"))
                .await,
            Err(Error::Offline(_))
        ));
        transport.set_health(Health::Online);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(40), listener.accept())
            .await
            .is_err()
    );
    assert_eq!(retry_after(&HeaderMap::new()), None);
    let cases = [
        Case {
            kind: Kind::History,
            status: "200 OK",
            headers: "",
            body: br#"{"ok":true,"messages":[{"type":"message","ts":"1700000000.123456"}]}"#,
            declared: None,
            outcome: Outcome::Raw,
        },
        Case {
            kind: Kind::Channel,
            status: "200 OK",
            headers: "",
            body: br#"{"ok":true,"channel":{"id":"C12345678"}}"#,
            declared: None,
            outcome: Outcome::Raw,
        },
        Case {
            kind: Kind::History,
            status: "200 OK",
            headers: "",
            body: br#"{"ok":true,"ok":false}"#,
            declared: None,
            outcome: Outcome::Rejected,
        },
        Case {
            kind: Kind::Channel,
            status: "200 OK",
            headers: "",
            body: br#"{"ok":true,"channel":{"id":1,"id":2}}"#,
            declared: None,
            outcome: Outcome::Rejected,
        },
        Case {
            kind: Kind::History,
            status: "200 OK",
            headers: "",
            body: b"[]",
            declared: None,
            outcome: Outcome::Rejected,
        },
        Case {
            kind: Kind::Channel,
            status: "200 OK",
            headers: "",
            body: b"invalid private-response-marker",
            declared: None,
            outcome: Outcome::Rejected,
        },
        Case {
            kind: Kind::History,
            status: "206 Partial Content",
            headers: "",
            body: b"{}",
            declared: None,
            outcome: Outcome::Http(206, None),
        },
        Case {
            kind: Kind::Channel,
            status: "302 Found",
            headers: "Location: /forbidden\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Http(302, None),
        },
        Case {
            kind: Kind::History,
            status: "429 Too Many Requests",
            headers: "",
            body: b"{}",
            declared: None,
            outcome: Outcome::Rate(None),
        },
        Case {
            kind: Kind::Channel,
            status: "429 Too Many Requests",
            headers: "Retry-After: 17\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Rate(Some(17)),
        },
        Case {
            kind: Kind::History,
            status: "429 Too Many Requests",
            headers: "Retry-After: 17\r\nRetry-After: 43\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Rate(Some(43)),
        },
        Case {
            kind: Kind::Channel,
            status: "429 Too Many Requests",
            headers: "Retry-After: 43\r\nRetry-After: 17\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Rate(Some(43)),
        },
        Case {
            kind: Kind::History,
            status: "429 Too Many Requests",
            headers: "Retry-After: 17\r\nRetry-After: malformed\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Rate(Some(u64::MAX)),
        },
        Case {
            kind: Kind::Channel,
            status: "429 Too Many Requests",
            headers: "Retry-After: Wed, 21 Oct 2037 07:28:00 GMT\r\nRetry-After: 17\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Rate(Some(u64::MAX)),
        },
        Case {
            kind: Kind::History,
            status: "429 Too Many Requests",
            headers: "Retry-After: 17\r\nRetry-After: 18446744073709551616\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Rate(Some(u64::MAX)),
        },
        Case {
            kind: Kind::Channel,
            status: "200 OK",
            headers: "Retry-After: 18446744073709551615\r\n",
            body: b"{}",
            declared: None,
            outcome: Outcome::Http(200, Some(u64::MAX)),
        },
        Case {
            kind: Kind::History,
            status: "200 OK",
            headers: "",
            body: br#"{"ok":false,"error":"ratelimited"}"#,
            declared: None,
            outcome: Outcome::Rate(None),
        },
        Case {
            kind: Kind::Channel,
            status: "200 OK",
            headers: "",
            body: b"",
            declared: Some(MAX_RESPONSE + 1),
            outcome: Outcome::Rejected,
        },
    ];
    for case in cases {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let r = request(listener.local_addr().unwrap(), case.kind);
        let current = AtomicBool::new(true);
        let fences = Cell::new(0);
        let server = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let w = wire(&mut socket).await;
            let (h, b) = w.split_once("\r\n\r\n").unwrap();
            assert!(h.to_ascii_lowercase().contains("cookie: d=fixture-cookie"));
            assert!(
                h.to_ascii_lowercase()
                    .contains("content-type: application/x-www-form-urlencoded")
            );
            assert!(!h.to_ascii_lowercase().contains("authorization:"));
            match case.kind {
                Kind::History => {
                    assert!(h.starts_with("POST /api/conversations.history "));
                    assert_eq!(
                        b,
                        "token=fixture-only&channel=C12345678&latest=1700000000.123456&inclusive=true&limit=1"
                    );
                }
                Kind::Channel => {
                    assert!(h.starts_with("POST /api/conversations.info "));
                    assert_eq!(
                        b,
                        "token=fixture-only&channel=C12345678&include_num_members=false"
                    );
                }
            }
            current.store(false, Ordering::Release);
            reply(
                &mut socket,
                case.status,
                case.headers,
                case.body,
                case.declared,
            )
            .await;
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(40), listener.accept())
                    .await
                    .is_err(),
                "no retry/redirect"
            );
        };
        let (result, ()) = tokio::join!(
            transport.execute_thread_timestamp_current(&r, || {
                fences.set(fences.get() + 1);
                current.load(Ordering::Acquire)
            }),
            server
        );
        assert_eq!(
            fences.get(),
            2,
            "issued original outcome is not discarded after retirement"
        );
        assert!(!current.load(Ordering::Acquire));
        match case.outcome {
            Outcome::Raw => assert_eq!(
                result.unwrap(),
                serde_json::from_slice::<Value>(case.body).unwrap()
            ),
            Outcome::Rate(expected) => assert!(
                matches!(result,Err(Error::RateLimited{retry_after_secs}) if retry_after_secs==expected)
            ),
            Outcome::Http(code, expected) => assert!(
                matches!(result,Err(Error::HttpStatus{status,retry_after_secs}) if status==code && retry_after_secs==expected)
            ),
            Outcome::Rejected => {
                let error = result.err().expect("strict response refusal");
                assert!(!format!("{error:?}").contains("private-response-marker"));
            }
        }
    }
}
