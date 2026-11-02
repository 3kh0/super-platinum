//! Actual loopback transport paths only. This corpus supplies no native action
//! owner, Slack session, SourceReady or physical acknowledgement to the app.
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        SlackClient, api,
        transport::{SearchReactionFailure, SearchReactionRead},
    },
};
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const C: &str = "C01234567";
const TS: &str = "1700000001.000001";
const LIMIT: usize = 1024 * 1024;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST".into(),
        user_id: "UTEST".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn write_request(adding: bool) -> PreparedRequest {
    let (client, session) = (SlackClient::default(), workspace());
    if adding {
        api::reactions_add(&client, &session, C.into(), TS.into(), "wave".into())
    } else {
        api::reactions_remove(&client, &session, C.into(), TS.into(), "wave".into())
    }
}
fn read_request(kind: SearchReactionRead) -> PreparedRequest {
    let (client, session) = (SlackClient::default(), workspace());
    match kind {
        SearchReactionRead::History => api::conversations_history(
            &client,
            &session,
            api::HistoryArgs {
                channel: C.into(),
                latest: Some(TS.into()),
                limit: Some(1),
                inclusive: true,
                ..Default::default()
            },
        ),
        SearchReactionRead::Channel => api::conversations_info(&client, &session, C.into()),
        SearchReactionRead::Membership => {
            api::reactions_get(&client, &session, C.into(), TS.into())
        }
    }
}

#[derive(Debug, PartialEq)]
enum Completion {
    Raw(Value),
    Error(&'static str, Option<u16>, Option<u64>),
}
#[derive(Debug, PartialEq)]
enum Seen {
    Attempted(ReactionRequestKind),
    Response(ReactionRequestKind, u16, ReactionAdvice),
    ApiLimit(ReactionRequestKind),
    Completed(ReactionRequestKind, Completion),
}
fn collect(events: &mut Vec<Seen>, event: ReactionWireEvent) {
    events.push(match event {
        ReactionWireEvent::Attempted(kind) => Seen::Attempted(kind),
        ReactionWireEvent::Response {
            kind,
            status,
            advice,
        } => Seen::Response(kind, status, advice),
        ReactionWireEvent::ApiRateLimit(kind) => Seen::ApiLimit(kind),
        ReactionWireEvent::Completed { kind, result } => Seen::Completed(
            kind,
            match result {
                Ok(raw) => Completion::Raw(raw),
                Err(Error::RateLimited { retry_after_secs }) => {
                    Completion::Error("rate", None, retry_after_secs)
                }
                Err(Error::HttpStatus {
                    status,
                    retry_after_secs,
                }) => Completion::Error("http", Some(status), retry_after_secs),
                Err(Error::Offline(_)) => Completion::Error("offline", None, None),
                Err(Error::Transport(_)) => Completion::Error("transport", None, None),
                Err(_) => Completion::Error("other", None, None),
            },
        ),
    });
}

struct Packet {
    status: &'static str,
    headers: &'static str,
    body: String,
    declared: Option<usize>,
    chunked: bool,
}
impl Packet {
    fn json(status: &'static str, headers: &'static str, body: &str) -> Self {
        Self {
            status,
            headers,
            body: body.into(),
            declared: None,
            chunked: false,
        }
    }
}
async fn consume(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 2048];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0, "original request ended before its complete form");
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() <= 32 * 1024, "bounded fixture request");
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length: usize = header
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
async fn no_connection(listener: &tokio::net::TcpListener) {
    assert!(
        tokio::time::timeout(Duration::from_millis(40), listener.accept())
            .await
            .is_err(),
        "no redirect, retry or second request"
    );
}

// Every URL is replaced with loopback before entering the production validator.
// A deliberately broken validator still cannot send a fixture to Slack.
async fn exchange(
    mut request: PreparedRequest,
    read: Option<SearchReactionRead>,
    adding: bool,
    packet: Packet,
    retire: bool,
) -> (Result<Value, SearchReactionFailure>, Vec<Seen>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let path = url::Url::parse(&request.url).unwrap().path().to_owned();
    let RequestBody::Form(expected) = &request.body else {
        panic!("original API form")
    };
    let expected = expected.clone();
    request.url = format!("http://{address}{path}");
    let current = Arc::new(AtomicBool::new(true));
    let original = current.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .expect("bounded loopback accept")
            .unwrap();
        let wire = tokio::time::timeout(Duration::from_secs(5), consume(&mut socket))
            .await
            .expect("bounded original form");
        assert!(wire.starts_with(&format!("POST {path} HTTP/1.1\r\n")));
        assert!(
            wire.to_ascii_lowercase()
                .contains("cookie: d=fixture-cookie\r\n")
        );
        let body = wire.split_once("\r\n\r\n").unwrap().1;
        let actual = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect::<Vec<_>>();
        assert_eq!(actual.len(), expected.len());
        assert_eq!(
            actual.into_iter().collect::<BTreeMap<_, _>>(),
            expected.into_iter().collect::<BTreeMap<_, _>>()
        );
        if retire {
            original.store(false, Ordering::Release);
        }
        let framing = if packet.chunked {
            "Transfer-Encoding: chunked\r\n".to_owned()
        } else {
            format!(
                "Content-Length: {}\r\n",
                packet.declared.unwrap_or(packet.body.len())
            )
        };
        let head = format!(
            "HTTP/1.1 {}\r\n{}{framing}Connection: close\r\n\r\n",
            packet.status, packet.headers
        );
        let body = if packet.chunked {
            format!("{:x}\r\n{}\r\n0\r\n\r\n", packet.body.len(), packet.body)
        } else {
            packet.body
        };
        // One write for ordinary responses avoids a fixture race when the
        // real client rejects a status immediately after reading its headers.
        let sent = socket.write_all(format!("{head}{body}").as_bytes()).await;
        // A client enforcing the streaming cap can close before the deliberately
        // oversized server write finishes. No long timeout is needed.
        if body.len() <= LIMIT {
            sent.unwrap();
        }
        drop(socket);
        no_connection(&listener).await;
    });
    let mut events = Vec::new();
    let mut checks = 0usize;
    let mut check = || {
        checks += 1;
        current.load(Ordering::Acquire)
    };
    let transport = Transport::new("fixture-cookie").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        if let Some(kind) = read {
            transport
                .execute_search_reaction_read_observed(request, kind, C, TS, &mut check, |event| {
                    collect(&mut events, event)
                })
                .await
        } else {
            transport
                .execute_search_reaction_once_observed(
                    request,
                    C,
                    TS,
                    "wave",
                    adding,
                    &mut check,
                    |event| collect(&mut events, event),
                )
                .await
        }
    })
    .await
    .expect("bounded actual loopback exchange; no retry or body timeout wait");
    assert_eq!(checks, 2, "only the two original pre-start owner fences");
    assert_eq!(current.load(Ordering::Acquire), !retire);
    server.await.unwrap();
    (result, events)
}

#[tokio::test]
async fn production_reaction_observer_fences_original_headers_and_late_outcome_corpus() {
    // Real exact validator and the first/second owner fences, not a helper-only
    // advice test. Neither refused path can report mutation issuance.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    for stage in 0..3 {
        let mut request = write_request(true);
        request.url = format!("http://{address}/api/reactions.add");
        if stage == 0 {
            let RequestBody::Form(fields) = &mut request.body else {
                unreachable!()
            };
            fields.push(("name".into(), "wave".into()));
        }
        let mut checks = 0;
        let mut events = Vec::new();
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_search_reaction_once_observed(
                request,
                C,
                TS,
                "wave",
                true,
                || {
                    checks += 1;
                    stage == 2 && checks == 1
                },
                |event| collect(&mut events, event),
            )
            .await;
        assert!(matches!(
            result,
            Err(SearchReactionFailure::NotRequested(_))
        ));
        assert_eq!(checks, stage, "validator/first/second fence respectively");
        assert!(events.is_empty());
        no_connection(&listener).await;
    }
    let mut request = write_request(true);
    request.url = format!("http://{address}/api/reactions.add");
    let offline = Transport::new("fixture-cookie").unwrap();
    offline.set_health(Health::Offline);
    let mut events = Vec::new();
    assert!(matches!(
        offline
            .execute_search_reaction_once_observed(
                request,
                C,
                TS,
                "wave",
                true,
                || panic!("known offline must not pass an owner fence"),
                |event| collect(&mut events, event)
            )
            .await,
        Err(SearchReactionFailure::NotRequested(_))
    ));
    assert!(events.is_empty());
    no_connection(&listener).await;

    // All three real read builders use Read observation, never Write issuance.
    // Search's 2xx advice quarantine preserves truthful raw Completed facts.
    for (kind, headers, advice) in [
        (SearchReactionRead::History, "", ReactionAdvice::None),
        (
            SearchReactionRead::Channel,
            "Retry-After: 4\r\n",
            ReactionAdvice::Seconds(4),
        ),
        (SearchReactionRead::Membership, "", ReactionAdvice::None),
    ] {
        let (result, events) = exchange(
            read_request(kind),
            Some(kind),
            true,
            Packet::json("200 OK", headers, "{\"ok\":true}"),
            true,
        )
        .await;
        if advice == ReactionAdvice::None {
            assert_eq!(result.unwrap(), json!({"ok":true}));
        } else {
            assert!(matches!(
                result,
                Err(SearchReactionFailure::Unknown(Error::HttpStatus {
                    status: 200,
                    retry_after_secs: Some(4)
                }))
            ));
        }
        assert_eq!(
            events,
            vec![
                Seen::Attempted(ReactionRequestKind::Read),
                Seen::Response(ReactionRequestKind::Read, 200, advice),
                Seen::Completed(
                    ReactionRequestKind::Read,
                    Completion::Raw(json!({"ok":true}))
                )
            ]
        );
    }

    // Each row exercises the actual HTTP header parser/production decoder.
    // The callback keeps original facts after the server retires the UI owner.
    for (headers, advice) in [
        ("", ReactionAdvice::None),
        (
            "Retry-After: 2\r\nRetry-After: 9\r\nRetry-After: 3\r\n",
            ReactionAdvice::Seconds(9),
        ),
        ("Retry-After: \t0 \t\r\n", ReactionAdvice::Seconds(1)),
        (
            "Retry-After: 2\r\nRetry-After: invalid\r\nRetry-After: 90\r\n",
            ReactionAdvice::Permanent,
        ),
        (
            "Retry-After: Wed, 21 Oct 2015 07:28:00 GMT\r\n",
            ReactionAdvice::Permanent,
        ),
        (
            "Retry-After: 18446744073709551616\r\n",
            ReactionAdvice::Permanent,
        ),
        ("Retry-After:\r\n", ReactionAdvice::Permanent),
        (
            "Retry-After: 18446744073709551615\r\n",
            ReactionAdvice::Seconds(u64::MAX),
        ),
    ] {
        let (result, events) = exchange(
            write_request(true),
            None,
            true,
            Packet::json("200 OK", headers, "{\"ok\":true}"),
            true,
        )
        .await;
        match advice {
            ReactionAdvice::None => assert_eq!(result.unwrap(), json!({"ok":true})),
            _ => {
                let expected = match advice {
                    ReactionAdvice::Seconds(s) => s,
                    _ => u64::MAX,
                };
                assert!(
                    matches!(result, Err(SearchReactionFailure::Unknown(Error::HttpStatus {
                    status:200, retry_after_secs:Some(s) })) if s==expected)
                );
            }
        }
        assert_eq!(
            events,
            vec![
                Seen::Attempted(ReactionRequestKind::Write),
                Seen::Response(ReactionRequestKind::Write, 200, advice),
                Seen::Completed(
                    ReactionRequestKind::Write,
                    Completion::Raw(json!({"ok":true}))
                )
            ]
        );
    }
    let (remove, events) = exchange(
        write_request(false),
        None,
        false,
        Packet::json("200 OK", "", "{\"ok\":true}"),
        false,
    )
    .await;
    assert_eq!(remove.unwrap(), json!({"ok":true}));
    assert_eq!(events.len(), 3);
    assert_eq!(events[0], Seen::Attempted(ReactionRequestKind::Write));

    for (headers, advice, seconds) in [
        ("", ReactionAdvice::Seconds(60), 60),
        (
            "Retry-After: 2\r\nRetry-After: 9\r\n",
            ReactionAdvice::Seconds(9),
            9,
        ),
        ("Retry-After: bad\r\n", ReactionAdvice::Permanent, u64::MAX),
    ] {
        let (result, events) = exchange(
            write_request(true),
            None,
            true,
            Packet::json("429 Too Many Requests", headers, "{}"),
            true,
        )
        .await;
        assert!(
            matches!(result, Err(SearchReactionFailure::Unknown(Error::RateLimited {
            retry_after_secs:Some(s) })) if s==seconds),
            "issued429 remains quarantined Unknown"
        );
        assert_eq!(
            events,
            vec![
                Seen::Attempted(ReactionRequestKind::Write),
                Seen::Response(ReactionRequestKind::Write, 429, advice),
                Seen::Completed(
                    ReactionRequestKind::Write,
                    Completion::Error("rate", None, Some(seconds))
                )
            ]
        );
    }
    let (result, events) = exchange(
        write_request(true),
        None,
        true,
        Packet::json(
            "200 OK",
            "Retry-After: 90\r\n",
            "{\"ok\":false,\"error\":\"ratelimited\"}",
        ),
        true,
    )
    .await;
    assert!(matches!(
        result,
        Err(SearchReactionFailure::Unknown(Error::RateLimited {
            retry_after_secs: Some(60)
        }))
    ));
    assert_eq!(
        events,
        vec![
            Seen::Attempted(ReactionRequestKind::Write),
            Seen::Response(ReactionRequestKind::Write, 200, ReactionAdvice::Seconds(90)),
            Seen::ApiLimit(ReactionRequestKind::Write),
            Seen::Completed(
                ReactionRequestKind::Write,
                Completion::Error("rate", None, Some(60))
            )
        ]
    );

    // Keep the owner current for these cases: no retry/redirect is not a
    // vacuous consequence of retirement. The loopback listener checks no peer.
    for (status, code, headers) in [
        ("302 Found", 302, "Location: /api/reactions.add\r\n"),
        ("503 Service Unavailable", 503, "Retry-After: 5\r\n"),
    ] {
        let (result, events) = exchange(
            write_request(true),
            None,
            true,
            Packet::json(status, headers, "{}"),
            false,
        )
        .await;
        assert!(
            matches!(result, Err(SearchReactionFailure::Unknown(Error::HttpStatus { status:s, .. })) if s==code)
        );
        assert!(
            matches!(events.last(), Some(Seen::Completed(ReactionRequestKind::Write,
            Completion::Error("http", Some(s), _))) if *s==code)
        );
    }
    for mut packet in [
        Packet::json("200 OK", "Retry-After: 7\r\n", "{\"ok\":true,\"ok\":false}"),
        Packet::json("200 OK", "Retry-After: 7\r\n", "[]"),
        Packet {
            declared: Some(LIMIT + 1),
            ..Packet::json("200 OK", "Retry-After: 7\r\n", "")
        },
        Packet {
            declared: Some(128),
            ..Packet::json("200 OK", "Retry-After: 7\r\n", "{\"ok\":true}")
        },
        Packet {
            chunked: true,
            ..Packet::json("200 OK", "Retry-After: 7\r\n", "")
        },
    ] {
        if packet.chunked {
            packet.body = format!("{{\"ok\":true,\"padding\":\"{}\"}}", "x".repeat(LIMIT));
        }
        let (result, events) = exchange(write_request(true), None, true, packet, true).await;
        assert!(matches!(result, Err(SearchReactionFailure::Unknown(_))));
        assert_eq!(events[0], Seen::Attempted(ReactionRequestKind::Write));
        assert_eq!(
            events[1],
            Seen::Response(ReactionRequestKind::Write, 200, ReactionAdvice::Seconds(7))
        );
        assert!(
            matches!(
                events.last(),
                Some(Seen::Completed(
                    ReactionRequestKind::Write,
                    Completion::Error(_, _, _)
                ))
            ),
            "malformed/bounded body is not truthful raw success or pre-call refusal"
        );
        assert_eq!(events.len(), 3);
    }
    // Ordinary explicit target/validation rejection remains raw evidence.
    let raw = json!({"ok":false,"error":"invalid_name"});
    let (result, events) = exchange(
        write_request(true),
        None,
        true,
        Packet::json("200 OK", "", &raw.to_string()),
        true,
    )
    .await;
    assert_eq!(result.unwrap(), raw);
    assert_eq!(
        events.last(),
        Some(&Seen::Completed(
            ReactionRequestKind::Write,
            Completion::Raw(raw)
        ))
    );
    for code in ["fatal_error", "internal_error"] {
        let raw = json!({"ok":false,"error":code});
        let (result, events) = exchange(
            write_request(true),
            None,
            true,
            Packet::json("200 OK", "", &raw.to_string()),
            true,
        )
        .await;
        assert!(
            matches!(result,Err(SearchReactionFailure::Unknown(Error::Api(ref value)))if value==code),
            "documented partial operation must remain quarantined"
        );
        assert_eq!(
            events.last(),
            Some(&Seen::Completed(
                ReactionRequestKind::Write,
                Completion::Error("other", None, None)
            ))
        );
        assert_eq!(events.len(), 3);
    }
    catalog_read_corpus().await;
}

// Extend this actual loopback corpus with the new isolated catalog entrypoint;
// all existing Search/write assertions above remain unchanged.
fn catalog_request() -> PreparedRequest {
    let session = workspace();
    api::emoji_catalog::Read::new(&session)
        .request(&SlackClient::default(), &session)
        .unwrap()
}
async fn catalog_exchange(
    packet: Packet,
    retire: bool,
) -> (Result<Value, ObservedReactionFailure>, Vec<Seen>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut request = catalog_request();
    request.url = format!("http://{address}/api/emoji.list");
    let current = Arc::new(AtomicBool::new(true));
    let original = current.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let wire = tokio::time::timeout(Duration::from_secs(5), consume(&mut socket))
            .await
            .unwrap();
        assert!(wire.starts_with("POST /api/emoji.list HTTP/1.1\r\n"));
        assert!(
            wire.to_ascii_lowercase()
                .contains("cookie: d=fixture-cookie\r\n")
        );
        let body = wire.split_once("\r\n\r\n").unwrap().1;
        let fields = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            vec![("token".to_owned(), "fixture-only".to_owned())]
        );
        if retire {
            original.store(false, Ordering::Release);
        }
        let head = format!(
            "HTTP/1.1 {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
            packet.status,
            packet.headers,
            packet.body.len()
        );
        socket
            .write_all(format!("{head}{}", packet.body).as_bytes())
            .await
            .unwrap();
        drop(socket);
        no_connection(&listener).await;
    });
    let transport = Transport::new("fixture-cookie").unwrap();
    let mut events = Vec::new();
    let mut checks = 0;
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        transport.execute_root_reaction_catalog_read_observed(
            request,
            || {
                checks += 1;
                current.load(Ordering::Acquire)
            },
            |event| collect(&mut events, event),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        checks, 2,
        "only original pre-start owner fences; issued read drains"
    );
    assert_eq!(current.load(Ordering::Acquire), !retire);
    assert!(!events.iter().any(|e| matches!(
        e,
        Seen::Attempted(ReactionRequestKind::Write)
            | Seen::Response(ReactionRequestKind::Write, _, _)
            | Seen::Completed(ReactionRequestKind::Write, _)
    )));
    server.await.unwrap();
    (result, events)
}
async fn catalog_read_corpus() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    for stage in 0..3 {
        let mut request = catalog_request();
        request.url = format!("http://{address}/api/emoji.list");
        if stage == 0 {
            let RequestBody::Form(fields) = &mut request.body else {
                unreachable!()
            };
            fields.push(("extra".into(), "not-token-only".into()));
        }
        let mut checks = 0;
        let mut events = Vec::new();
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_root_reaction_catalog_read_observed(
                request,
                || {
                    checks += 1;
                    stage == 2 && checks == 1
                },
                |event| collect(&mut events, event),
            )
            .await;
        assert!(matches!(
            result,
            Err(ObservedReactionFailure::NotRequested(_))
        ));
        assert_eq!(checks, stage);
        assert!(events.is_empty());
        no_connection(&listener).await;
    }
    let raw = json!({"ok":true,"emoji":{"custom_party":"https://example.invalid/exact.png","alias":"alias:custom_party"}});
    for (headers, advice) in [
        ("", ReactionAdvice::None),
        (
            "Retry-After: 2\r\nRetry-After: 9\r\nRetry-After: 3\r\n",
            ReactionAdvice::Seconds(9),
        ),
        ("Retry-After: invalid\r\n", ReactionAdvice::Permanent),
    ] {
        let (result, events) =
            catalog_exchange(Packet::json("200 OK", headers, &raw.to_string()), true).await;
        let value = result.unwrap();
        assert_eq!(value, raw);
        assert!(api::emoji_catalog::Complete::decode(value).is_ok());
        assert_eq!(
            events,
            vec![
                Seen::Attempted(ReactionRequestKind::Read),
                Seen::Response(ReactionRequestKind::Read, 200, advice),
                Seen::Completed(ReactionRequestKind::Read, Completion::Raw(raw.clone()))
            ]
        );
    }
    let (result, events) = catalog_exchange(
        Packet::json(
            "429 Too Many Requests",
            "Retry-After: 2\r\nRetry-After: 9\r\n",
            "{}",
        ),
        true,
    )
    .await;
    assert!(matches!(
        result,
        Err(ObservedReactionFailure::Unknown(Error::RateLimited {
            retry_after_secs: Some(9)
        }))
    ));
    assert_eq!(events[0], Seen::Attempted(ReactionRequestKind::Read));
    assert_eq!(
        events[1],
        Seen::Response(ReactionRequestKind::Read, 429, ReactionAdvice::Seconds(9))
    );
    assert!(matches!(
        events.last(),
        Some(Seen::Completed(
            ReactionRequestKind::Read,
            Completion::Error("rate", None, Some(9))
        ))
    ));
    let (result, events) = catalog_exchange(
        Packet::json("200 OK", "Retry-After: 7\r\n", "{malformed"),
        true,
    )
    .await;
    assert!(matches!(result, Err(ObservedReactionFailure::Unknown(_))));
    assert_eq!(
        events[1],
        Seen::Response(ReactionRequestKind::Read, 200, ReactionAdvice::Seconds(7))
    );
    assert!(matches!(
        events.last(),
        Some(Seen::Completed(
            ReactionRequestKind::Read,
            Completion::Error(_, _, _)
        ))
    ));
}
