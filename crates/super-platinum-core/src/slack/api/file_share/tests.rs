use super::*;
use crate::slack::{client::RequestBody, transport::Health};
use serde_json::json;

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000001".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.slack.com".into(),
        token: "synthetic-token".into(),
    }
}
fn intent() -> ExistingFileShare {
    ExistingFileShare::new("F00000001".into(), "C00000001".into(), None).unwrap()
}
fn fields(request: PreparedRequest) -> Vec<(String, String)> {
    assert_eq!(request.method, "POST");
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    fields
}
#[test]
fn ordinary_form_has_exact_flags_and_no_upload_or_forward_fields() {
    let request = existing_file_share(&SlackClient::default(), &session(), &intent());
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/files.share"
    );
    assert_eq!(
        fields(request),
        vec![
            ("token".into(), "synthetic-token".into()),
            ("files".into(), "F00000001".into()),
            ("channel".into(), "C00000001".into()),
            ("resharing_aware".into(), "true".into()),
            ("skip_dlp_user_warning".into(), "false".into()),
            ("from_share_modal".into(), "false".into()),
        ]
    );
}
#[test]
fn rich_note_preserves_complete_unknown_metadata_and_unicode() {
    let blocks = vec![
        json!({"type":"rich_text","block_id":"opaque","unknown":{"order":[3,1]},"elements":[{"type":"rich_text_section","extra":true,"elements":[{"type":"text","text":"\u{65e5}\u{672c}\u{8a9e} \u{1f98a} e\u{301}","style":{"bold":true,"code":true,"future":7}},{"type":"link","url":"https://example.test","text":"name","future":{"x":1}}]}]}),
    ];
    let note = FileShareNote::new(blocks.clone()).unwrap();
    assert_eq!(note.blocks(), blocks);
    let i = ExistingFileShare::new("F00000001".into(), "D00000001".into(), Some(note)).unwrap();
    let f = fields(existing_file_share(&SlackClient::default(), &session(), &i));
    assert_eq!(f.len(), 7);
    assert_eq!(f[3].0, "blocks");
    let decoded = serde_json::from_str::<Vec<Value>>(&f[3].1).unwrap();
    assert_eq!(decoded, blocks);
    let text = decoded[0]["elements"][0]["elements"][0]["text"]
        .as_str()
        .unwrap();
    assert_eq!(
        text.chars().map(u32::from).collect::<Vec<_>>(),
        vec![0x65e5, 0x672c, 0x8a9e, 0x20, 0x1f98a, 0x20, 0x65, 0x301]
    );
}
#[test]
fn context_is_explicit_and_input_cannot_inject_other_routes() {
    let i = intent().with_context_team("T00000002".into()).unwrap();
    assert_eq!(
        fields(existing_file_share(&SlackClient::default(), &session(), &i))
            .last()
            .unwrap(),
        &("client_context_team_id".into(), "T00000002".into())
    );
    for file in ["", "F1,F2", "F1\n", "U00000001"] {
        assert!(ExistingFileShare::new(file.into(), "C00000001".into(), None).is_err());
    }
    for channel in ["", "U00000001", "Z00000001", "C1", "C00000001,C00000002"] {
        assert!(ExistingFileShare::new("F1".into(), channel.into(), None).is_err());
    }
    assert!(intent().with_context_team("E00000001".into()).is_err());
}
#[test]
fn malformed_and_oversized_notes_are_refused_without_flattening() {
    for blocks in [
        vec![],
        vec![json!(null)],
        vec![json!({})],
        vec![json!({"type":7})],
        vec![json!({"type":""})],
    ] {
        assert!(FileShareNote::new(blocks).is_err());
    }
    assert!(matches!(
        FileShareNote::new(vec![
            json!({"type":"rich_text","text":"a".repeat(FILE_SHARE_NOTE_BYTES)})
        ]),
        Err(FileShareInputError::NoteLimit)
    ));
    let mut nested = json!(0);
    for _ in 0..65 {
        nested = json!([nested]);
    }
    assert!(FileShareNote::new(vec![json!({"type":"rich_text","future":nested})]).is_err());
}
#[test]
fn accepted_raw_preserves_unknown_fields_without_invented_identity_echo() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"error":null,"future":{"a":[1,2]}}),
    ] {
        let r = decode_existing_file_share(raw.clone());
        assert_eq!(
            r.disposition(),
            &FileShareDisposition::Accepted { message_ts: None }
        );
        assert_eq!(r.raw(), Some(&raw));
    }
}
#[test]
fn timestamps_are_optional_navigation_only() {
    for (raw, expected) in [
        (
            json!({"ok":true,"file_msg_ts":"123.000001"}),
            Some("123.000001"),
        ),
        (
            json!({"ok":true,"ts":"124.000001","file_msg_ts":"123.000001"}),
            Some("124.000001"),
        ),
        (json!({"ok":true,"ts":"https://example.test"}), None),
        (json!({"ok":true,"file_msg_ts":123}), None),
    ] {
        assert_eq!(
            decode_existing_file_share(raw).disposition(),
            &FileShareDisposition::Accepted {
                message_ts: expected.map(str::to_owned)
            }
        );
    }
}
#[test]
fn contradictory_or_malformed_envelopes_never_become_accepted() {
    for raw in [
        json!(null),
        json!([]),
        json!({}),
        json!({"ok":1}),
        json!({"ok":false}),
        json!({"ok":false,"error":{}}),
        json!({"ok":true,"error":"denied"}),
        json!({"ok":true,"error":false}),
        json!({"ok":false,"error":"blocked_file_type","file_msg_ts":"1.000001"}),
    ] {
        let r = decode_existing_file_share(raw.clone());
        assert!(matches!(
            r.disposition(),
            FileShareDisposition::Uncertain(_)
        ));
        assert_eq!(r.raw(), Some(&raw));
    }
}
#[test]
fn known_policy_is_feedback_not_a_resend_instruction() {
    for code in [
        "blocked_file_type",
        "slack_connect_blocked_file_type",
        "slack_connect_file_link_sharing_blocked",
        "slack_connect_file_upload_sharing_blocked",
        "posting_to_channel_denied",
        "restricted_action",
        "user_is_external_guest",
        "message_limit_exceeded",
        "max_file_sharing_exceeded",
        "file_share_limit_reached",
        "file_not_found",
        "file_deleted",
        "channel_not_found",
    ] {
        assert!(matches!(
            decode_existing_file_share(json!({"ok":false,"error":code})).disposition(),
            FileShareDisposition::Policy(_)
        ));
    }
    assert_eq!(
        decode_existing_file_share(json!({"ok":false,"error":"new_server_error"})).disposition(),
        &FileShareDisposition::Uncertain(FileShareUncertainty::UnclassifiedError)
    );
}
#[test]
fn dlp_warning_and_processing_remain_distinct_and_never_override_request() {
    let raw = json!({"ok":false,"error":"dlp_user_warning","dlp_custom_message":"Review \u{65e5}\u{672c}\u{8a9e} \u{1f98a}","future":42});
    let r = decode_existing_file_share(raw.clone());
    assert_eq!(r.raw(), Some(&raw));
    assert_eq!(
        r.disposition(),
        &FileShareDisposition::DlpWarning {
            message: Some("Review \u{65e5}\u{672c}\u{8a9e} \u{1f98a}".into())
        }
    );
    assert_eq!(
        decode_existing_file_share(json!({"ok":false,"error":"dlp_processing"})).disposition(),
        &FileShareDisposition::DlpProcessing
    );
    assert_eq!(
        fields(existing_file_share(
            &SlackClient::default(),
            &session(),
            &intent()
        ))[4]
            .1,
        "false"
    );
}
#[test]
fn disposition_debug_redacts_server_content_without_altering_retained_evidence() {
    let message = "private-server-message \u{65e5}\u{672c}\u{8a9e} \u{1f98a}";
    let raw = json!({"ok":false,"error":"dlp_user_warning","dlp_custom_message":message});
    let response = decode_existing_file_share(raw.clone());
    assert_eq!(response.raw(), Some(&raw));
    assert_eq!(
        response.disposition(),
        &FileShareDisposition::DlpWarning {
            message: Some(message.into())
        }
    );
    assert_eq!(
        format!("{:?}", response.disposition()),
        "DlpWarning { has_message: true }"
    );
    assert_eq!(
        format!("{:#?}", response.disposition()),
        "DlpWarning {\n    has_message: true,\n}"
    );
    let accepted = FileShareDisposition::Accepted {
        message_ts: Some("123.000001".into()),
    };
    assert_eq!(format!("{accepted:?}"), "Accepted { has_message_ts: true }");
    for debug in [
        format!("{:?}", response.disposition()),
        format!("{:#?}", response.disposition()),
        format!("{accepted:#?}"),
    ] {
        assert!(!debug.contains(message));
        assert!(!debug.contains("private-server-message"));
        assert!(!debug.contains("123.000001"));
    }
}
#[test]
fn response_retention_bound_drops_oversized_raw_honestly() {
    let r = decode_existing_file_share(
        json!({"ok":true,"unknown":"x".repeat(FILE_SHARE_RESPONSE_BYTES)}),
    );
    assert_eq!(
        r.disposition(),
        &FileShareDisposition::Uncertain(FileShareUncertainty::ResponseLimit)
    );
    assert!(r.raw().is_none());
}
#[tokio::test]
async fn existing_single_attempt_path_refuses_known_offline_before_request() {
    let t = Transport::new("synthetic-cookie").unwrap();
    t.set_health(Health::Offline);
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        t.execute_draft_mutation(existing_file_share(
            &SlackClient::default(),
            &session(),
            &intent(),
        )),
    )
    .await
    .unwrap();
    assert!(matches!(result,Err(Error::Api(code)) if code=="draft_mutation_not_sent_offline"));
}
#[tokio::test]
async fn actual_builder_and_transport_keep_raw_policy_rate_limit_and_lost_reply_single_attempt() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for mode in ["accepted", "policy", "rate", "lost"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut request = existing_file_share(&SlackClient::default(), &session(), &intent());
        request.url = format!("http://{address}/api/files.share");
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 32 * 1024);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let size = header
                        .lines()
                        .find_map(|l| {
                            l.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + size {
                        break;
                    }
                }
            }
            let wire = String::from_utf8(bytes).unwrap();
            assert!(wire.starts_with("POST /api/files.share "));
            let body = wire.split_once("\r\n\r\n").unwrap().1;
            let actual: Vec<_> = url::form_urlencoded::parse(body.as_bytes())
                .into_owned()
                .collect();
            assert_eq!(
                actual,
                fields(existing_file_share(
                    &SlackClient::default(),
                    &session(),
                    &intent()
                ))
            );
            if mode != "lost" {
                let (status, extra, body) = match mode {
                    "rate" => ("429 Too Many Requests", "Retry-After: 7\r\n", ""),
                    "policy" => (
                        "200 OK",
                        "",
                        r#"{"ok":false,"error":"blocked_file_type","future":1}"#,
                    ),
                    _ => ("200 OK", "", r#"{"ok":true,"future":1}"#),
                };
                stream.write_all(format!("HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept())
                    .await
                    .is_err()
            );
        };
        let t = Transport::new("synthetic-cookie").unwrap();
        let ((), result) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(server, t.execute_draft_mutation(request))
        })
        .await
        .unwrap();
        match mode {
            "accepted" => assert!(matches!(
                decode_existing_file_share(result.unwrap()).disposition(),
                FileShareDisposition::Accepted { .. }
            )),
            "policy" => assert!(matches!(
                decode_existing_file_share(result.unwrap()).disposition(),
                FileShareDisposition::Policy(FileSharePolicy::FileType)
            )),
            "rate" => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(7)
                })
            )),
            _ => assert!(result.is_err()),
        }
    }
}
