//! Real configured intent -> single-use permit -> API builder -> closed transport.
//! Local listeners are synthetic tests only; no real session or Slack requests.
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api},
    slack_lists::{
        ColumnCreate, ColumnCreateAttempt, ColumnCreatePhase, Document, EntityCreateDefaults,
        EntityCreateDomain, EntityCreateFormat, EntityCreateOptions, PersonalTaskLists,
        RecordsPage,
    },
};
use EntityCreateDefaults::{Null, Omitted, Selected};
use EntityCreateDomain::{Channel, People};
use EntityCreateFormat::{Multiple, Single};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn ids(domain: EntityCreateDomain) -> Vec<String> {
    match domain {
        People => ["W00000002", "U00000001"],
        Channel => ["G00000002", "C00000001"],
    }
    .map(str::to_owned)
    .to_vec()
}
fn request(options: EntityCreateOptions, url: String) -> (PreparedRequest, ColumnCreateAttempt) {
    let document=Document::parse(json!({"id":"F_ENTITY","mode":"list","user":"U1","access":"owner","list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[{"id":"TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"}}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[]}]}})).unwrap();
    let records = RecordsPage::parse(
        json!({"ok":true,"records":[],"response_metadata":{"next_cursor":"NEXT"}}),
    )
    .unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    let expected = options.wire();
    let domain = options.domain();
    let intent = ColumnCreate::capture_entity(
        &document,
        &records,
        "V1",
        "  Caf\u{e9} & \u{6771}\u{4eac}  ",
        options,
        "U1",
        Some("full"),
        Some(&policy),
    )
    .unwrap();
    let mut attempt = ColumnCreateAttempt::new(intent);
    let permit = attempt
        .admit(&document, &records, "U1", Some("full"), Some(&policy))
        .unwrap();
    assert!(
        attempt
            .admit(&document, &records, "U1", Some("full"), Some(&policy))
            .is_err()
    );
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url,
        token: "fixture-token".into(),
    };
    let request = api::list_column_create(&SlackClient::default(), &session, permit);
    let RequestBody::Form(fields) = &request.body else {
        panic!()
    };
    assert_eq!(
        fields,
        &vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F_ENTITY".into()),
            ("view_id".into(), "V1".into()),
            ("name".into(), "Caf\u{e9} & \u{6771}\u{4eac}".into()),
            ("type".into(), domain.as_str().into()),
            ("options".into(), expected.to_string()),
            ("_x_reason".into(), "create-column".into()),
        ]
    );
    assert!(allowed(&request));
    (request, attempt)
}
fn expected_body(request: &PreparedRequest) -> String {
    let RequestBody::Form(fields) = &request.body else {
        panic!()
    };
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish()
}
async fn read_body(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut all = Vec::new();
    let body_start = loop {
        let mut bytes = [0; 2048];
        let n = socket.read(&mut bytes).await.unwrap();
        assert!(n > 0);
        all.extend_from_slice(&bytes[..n]);
        if let Some(at) = all.windows(4).position(|s| s == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let header = String::from_utf8(all[..body_start].to_vec()).unwrap();
    assert!(header.starts_with("POST /api/lists.columns.create"));
    let length = header
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    while all.len() < body_start + length {
        let mut bytes = [0; 2048];
        let n = socket.read(&mut bytes).await.unwrap();
        assert!(n > 0);
        all.extend_from_slice(&bytes[..n]);
    }
    all[body_start..body_start + length].to_vec()
}

#[test]
fn configured_api_matrix_keeps_all_seven_fields_null_omitted_order_names_and_unicode() {
    let mut cases = 0;
    for domain in [People, Channel] {
        for format in [Single, Multiple] {
            for defaults in [
                Null,
                Omitted,
                Selected(vec![ids(domain)[0].clone()]),
                Selected(ids(domain)),
            ] {
                for names in [true, false].into_iter().filter(|n| *n || domain == People) {
                    let options =
                        EntityCreateOptions::capture(domain, format, names, defaults.clone())
                            .unwrap();
                    let (_, attempt) = request(options, "https://fixture.slack.com".into());
                    assert_eq!(attempt.phase(), ColumnCreatePhase::InFlight);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 24);
}

#[tokio::test]
async fn actual_configured_forms_dispatch_once_and_preserve_exact_encoded_options() {
    for domain in [People, Channel] {
        for defaults in [Omitted, Selected(ids(domain))] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            // Retained multiple defaults in Single must reach HTTP unchanged.
            let options = EntityCreateOptions::capture(domain, Multiple, true, defaults)
                .unwrap()
                .with_format(Single)
                .unwrap();
            let (request, mut attempt) = request(options, format!("http://{address}"));
            let expected = expected_body(&request);
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
            let response = Transport::new("fixture-cookie")
                .unwrap()
                .execute_list_column_create(request, || true)
                .await
                .unwrap();
            assert_eq!(response, json!({"ok":true}));
            assert!(attempt.finish(false));
            assert_eq!(attempt.phase(), ColumnCreatePhase::Unconfirmed); // ACK is not canonical readback.
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn configured_lost_response_rate_limit_and_retired_owner_do_not_replay() {
    for domain in [People, Channel] {
        for mode in 0..3 {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let options =
                EntityCreateOptions::capture(domain, Single, true, Selected(ids(domain))).unwrap();
            let (request, mut attempt) = request(options, format!("http://{address}"));
            let expected = expected_body(&request);
            let current = Arc::new(AtomicBool::new(true));
            let retire = current.clone();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                assert_eq!(read_body(&mut socket).await, expected.as_bytes());
                match mode {
                    0=>{},
                    1=>socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap(),
                    _=>{retire.store(false,Ordering::SeqCst);socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();},
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
                .execute_list_column_create(request, || current.load(Ordering::SeqCst))
                .await;
            match mode {
                0 => assert!(matches!(
                    result,
                    Err(ListColumnCreateFailure::Unconfirmed(_))
                )),
                1 => assert!(matches!(
                    result,
                    Err(ListColumnCreateFailure::Unconfirmed(Error::RateLimited {
                        retry_after_secs: Some(2)
                    }))
                )),
                _ => {
                    assert_eq!(result.unwrap(), json!({"ok":true}));
                    assert!(!current.load(Ordering::SeqCst));
                }
            }
            assert!(attempt.finish(false));
            assert_eq!(attempt.phase(), ColumnCreatePhase::Unconfirmed);
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn configured_stale_owner_and_malformed_options_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for domain in [People, Channel] {
        let options =
            EntityCreateOptions::capture(domain, Single, true, Selected(ids(domain))).unwrap();
        let (r, _) = request(options.clone(), format!("http://{address}"));
        let mut checks = 0;
        assert!(matches!(
            transport
                .execute_list_column_create(r, || {
                    checks += 1;
                    checks == 1
                })
                .await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
        assert_eq!(checks, 2);
        let base = options.wire();
        let mut reversed = base.clone();
        reversed["default_value_typed"][domain.as_str()]
            .as_array_mut()
            .unwrap()
            .reverse();
        let mut missing = base.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove("default_value_typed");
        let mut mixed = base.clone();
        mixed["default_value_typed"][if domain == People { "channel" } else { "user" }] = json!([]);
        let mut unknown = base.clone();
        unknown["server_extra"] = json!(true);
        let duplicate = format!("{{\"format\":\"single_entity\",{}", &base.to_string()[1..]);
        let oversized = format!("{}{}", base, " ".repeat(8193));
        for raw in [
            reversed.to_string(),
            missing.to_string(),
            mixed.to_string(),
            unknown.to_string(),
            duplicate,
            oversized,
        ] {
            let (mut r, _) = request(options.clone(), format!("http://{address}"));
            let RequestBody::Form(fields) = &mut r.body else {
                panic!()
            };
            fields[5].1 = raw;
            assert!(matches!(
                transport.execute_list_column_create(r, || true).await,
                Err(ListColumnCreateFailure::NotRequested(_))
            ));
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
