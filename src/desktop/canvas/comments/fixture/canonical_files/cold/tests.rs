use super::*;
use super_platinum_core::canvas::comments::file_facts::{Store, embedded_files};

#[test]
fn every_named_permission_gate_is_required_without_environment_mutation() {
    let name = Some("canvas-canonical-file-custody");
    assert!(permitted(true, true, Some("1"), Some("1"), name));
    assert!(!permitted(false, true, Some("1"), Some("1"), name));
    assert!(!permitted(true, false, Some("1"), Some("1"), name));
    for missing in [None, Some(""), Some("true"), Some("0")] {
        assert!(!permitted(true, true, missing, Some("1"), name));
        assert!(!permitted(true, true, Some("1"), missing, name));
    }
    assert!(!permitted(true, true, Some("1"), Some("1"), None));
    assert!(!permitted(
        true,
        true,
        Some("1"),
        Some("1"),
        Some("canvas-canonical-file-roots")
    ));
}
#[test]
fn exact_closed_identity_refuses_foreign_credentials_and_scope() {
    assert!(identity(
        Some(ACCOUNT),
        Some("T1"),
        "U0",
        false,
        false,
        Some("Ffixture")
    ));
    assert!(!identity(
        None,
        Some("T1"),
        "U0",
        false,
        false,
        Some("Ffixture")
    ));
    assert!(!identity(
        Some("foreign"),
        Some("T1"),
        "U0",
        false,
        false,
        Some("Ffixture")
    ));
    assert!(!identity(
        Some(ACCOUNT),
        Some("T2"),
        "U0",
        false,
        false,
        Some("Ffixture")
    ));
    assert!(!identity(
        Some(ACCOUNT),
        Some("T1"),
        "U1",
        false,
        false,
        Some("Ffixture")
    ));
    assert!(!identity(
        Some(ACCOUNT),
        Some("T1"),
        "U0",
        true,
        false,
        Some("Ffixture")
    ));
    assert!(!identity(
        Some(ACCOUNT),
        Some("T1"),
        "U0",
        false,
        true,
        Some("Ffixture")
    ));
    assert!(!identity(
        Some(ACCOUNT),
        Some("T1"),
        "U0",
        false,
        false,
        Some("Fforeign")
    ));
    for name in [
        "canvas-canonical-file-custody",
        "canvas-canonical-file-custody-narrow",
        "canvas-canonical-native-canvas",
        "canvas-canonical-native-canvas-narrow",
        "canvas-canonical-native-list",
        "canvas-canonical-native-list-narrow",
    ] {
        assert!(named(name));
    }
    for name in [
        "canvas-canonical-file-roots",
        "canvas-comments",
        "canvas-canonical-file-custody-extra",
        "",
    ] {
        assert!(!named(name));
    }
}
#[test]
fn decoration_removes_every_warm_selected_id_without_changing_root_body_or_controls() {
    for id in [HOSTED, CANVAS, LIST] {
        let original = super::super::original(ROOT, id, "Original", super::super::Mode::Original);
        let other = super::super::original(ROOTS[1], id, "Other", super::super::Mode::Original);
        let mut rows = vec![original.clone(), other.clone()];
        decorate_for(&mut rows, id);
        assert_eq!(
            rows[0]["files"],
            json!([{"id":id,"file_access":"check_file_info"}])
        );
        for (row, before) in rows.iter().zip([original, other]) {
            for key in [
                "ts",
                "user",
                "text",
                "blocks",
                "attachments",
                "document_comment",
                "reply_count",
            ] {
                assert_eq!(row.get(key), before.get(key), "{key}");
            }
            let _: Message = serde_json::from_value(row.clone()).unwrap();
        }
        let sidecar = embedded_files(&rows).unwrap();
        assert!(sidecar.iter().all(|file| file["id"] != id));
        assert!(sidecar.iter().any(|file| file["id"] == SECOND));
        let mut store = Store::default();
        store.observe_batch(&sidecar).unwrap();
        assert!(store.get(id).is_none());
    }
}
#[test]
fn unknown_missing_name_and_missing_permalink_are_observed_rows_not_absence() {
    for raw in [
        json!({"id":HOSTED,"isUnknown":true}),
        json!({"id":HOSTED,"permalink":"https://example.test/file"}),
        json!({"id":HOSTED,"name":"Known without permalink"}),
    ] {
        let batch = embedded_files(&[json!({"files":[raw.clone()]})]).unwrap();
        let mut store = Store::default();
        store.observe_batch(&batch).unwrap();
        assert_eq!(store.get(HOSTED).unwrap().source(), &raw);
    }
}
#[test]
fn shared_factory_accepts_fresh_trace_ids_and_exact_six_source_arguments() {
    let client = SlackClient::default();
    let session = session();
    let request = super::super::super::super::file_acquisition::request(&client, &session, HOSTED);
    let args = protocol::validate(&client, &session, &request, HOSTED).unwrap();
    assert_eq!(
        args,
        json!({"file":HOSTED,"page":"1","count":"500","truncate":"true",
        "skip_shares":"true","_x_reason":"files-store-unknown-fetch"})
    );
    assert!(
        protocol::validate(
            &client,
            &session,
            &super_platinum_core::slack::api::file_info(&client, &session, HOSTED),
            HOSTED
        )
        .is_err()
    );
}
#[test]
fn wire_refuses_foreign_extra_duplicate_fields_headers_and_malformed_traces() {
    let client = SlackClient::default();
    let session = session();
    let original = protocol::expected(&client, &session, HOSTED);
    for mutation in 0..10 {
        let mut bad = original.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            panic!("form")
        };
        match mutation {
            0 => fields.push(("file".into(), HOSTED.into())),
            1 => fields.push(("extra".into(), "true".into())),
            2 => fields.iter_mut().find(|(k, _)| k == "file").unwrap().1 = SECOND.into(),
            3 => {
                fields
                    .iter_mut()
                    .find(|(k, _)| k == "skip_shares")
                    .unwrap()
                    .1 = "false".into()
            }
            4 => fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 = "navigation".into(),
            5 => bad.url.push_str("&extra=true"),
            6 => bad.url.push_str("&_x_id=0123456789abcdef0123456789abcdef"),
            7 => {
                let mut url = url::Url::parse(&bad.url).unwrap();
                let pairs = url
                    .query_pairs()
                    .map(|(k, v)| {
                        let v = if k == "_x_id" {
                            "not-a-trace".into()
                        } else {
                            v.into_owned()
                        };
                        (k.into_owned(), v)
                    })
                    .collect::<Vec<_>>();
                url.query_pairs_mut().clear().extend_pairs(pairs);
                bad.url = url.into();
            }
            8 => bad.url = bad.url.replace("fixture.slack.com", "foreign.invalid"),
            _ => bad.headers.push(("X-Foreign".into(), "true".into())),
        }
        assert!(
            protocol::validate(&client, &session, &bad, HOSTED).is_err(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn original_response_modes_are_distinct_and_title_never_substitutes_name() {
    let success = response(Mode::Success, HOSTED);
    assert_eq!(success, response(Mode::Held, HOSTED));
    assert_eq!(
        success["file"]["name"],
        "Cold canonical FCANON001 & \u{4e2d}\u{1f98a}"
    );
    assert_ne!(success["file"]["name"], success["file"]["title"]);
    assert_eq!(
        success["file"]["fixture_opaque"],
        file(HOSTED)["fixture_opaque"]
    );
    assert_eq!(response(Mode::Offline, HOSTED), Value::Null);
    assert_eq!(
        response(Mode::Malformed, HOSTED),
        json!({"ok":true,"file":false})
    );
    assert_eq!(response(Mode::WrongId, HOSTED)["file"]["id"], SECOND);
    assert_eq!(
        response(Mode::Error, HOSTED),
        json!({"ok":false,"error":"file_not_found"})
    );
}
#[test]
fn held_original_survives_mode_change_and_retired_receiver_cannot_deliver() {
    let (sender, receiver) = oneshot::channel();
    let original = response(Mode::Held, HOSTED);
    let held = Held {
        serial: 1,
        id: HOSTED.into(),
        response: original.clone(),
        sender,
    };
    let _future = response(Mode::WrongId, HOSTED);
    assert_eq!(held.response, original);
    drop(receiver);
    assert!(held.sender.is_closed());
    assert_eq!(held.sender.send(held.response).unwrap_err(), original);
}
#[test]
fn rate_response_is_typed_without_a_fabricated_raw_ack_or_resetting_first_time() {
    assert_eq!(Mode::RateLimited.label(), "rate-limited");
    assert_eq!(response(Mode::RateLimited, HOSTED), Value::Null);
    assert!(matches!(
        outcome(Mode::RateLimited, response(Mode::RateLimited, HOSTED)),
        Err(Error::RateLimited {
            retry_after_secs: Some(15)
        })
    ));
    let mut ledger = Ledger::default();
    let first = std::time::Instant::now();
    record_rate(&mut ledger, Mode::Offline, first);
    assert!(ledger.rate_started_at.is_none());
    record_rate(&mut ledger, Mode::RateLimited, first);
    let later = first + std::time::Duration::from_secs(16);
    record_rate(&mut ledger, Mode::Success, later);
    record_rate(&mut ledger, Mode::RateLimited, later);
    assert_eq!(ledger.rate_started_at, Some(first));
    assert_eq!(
        later
            .duration_since(ledger.rate_started_at.unwrap())
            .as_millis(),
        16_000
    );
    assert!(ledger.requests.is_empty());
    assert!(ledger.responses.is_empty());
}
