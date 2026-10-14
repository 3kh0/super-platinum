use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    }
}
#[test]
fn exact_request_is_server_identity_only_and_standard_policy_does_not_retry() {
    let request = drafts_info(&SlackClient::default(), &workspace(), "DRAFT1").unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.url.split('?').next(),
        Some("https://fixture.slack.com/api/drafts.info")
    );
    assert!(!request.retry_safe());
    assert_eq!(
        request.body,
        RequestBody::Form(vec![
            ("token".into(), "fixture-token".into()),
            ("draft_id".into(), "DRAFT1".into())
        ])
    );
}
#[test]
fn invalid_identity_never_builds_a_request_or_positive_response() {
    for id in [
        "",
        " ",
        "D 1",
        "D\n1",
        "D\u{0000}1",
        "D1,D2",
        &"x".repeat(256),
    ] {
        assert!(drafts_info(&SlackClient::default(), &workspace(), id).is_err());
        assert!(decode_drafts_info(id, json!({"ok":true,"draft":{"id":id}})).is_err());
    }
}
#[test]
fn positive_raw_preserves_every_option_and_extension_without_projection() {
    let raw = json!({"ok":true,"draft":{"id":"D1","client_msg_id":"unvalidated-here","is_from_composer":false,"blocks":[{"type":"rich_text","elements":[]}],"file_ids":["F2","F1"],"destinations":[{"channel_id":"C1","thread_ts":"1.000001","broadcast":true,"future_route":17}],"future_lifecycle":{"opaque":[true,null]},"last_updated_ts":"1719700000.000001"},"response_metadata":{"future":"retained"}});
    let decoded = decode_drafts_info("D1", raw.clone()).unwrap();
    assert_eq!(decoded.id(), "D1");
    assert_eq!(decoded.raw(), &raw);
    assert_eq!(decoded.draft(), &raw["draft"]);
}
#[test]
fn missing_error_and_malformed_envelopes_cannot_prove_absence() {
    for raw in [
        json!({"ok":false,"error":"draft_not_found"}),
        json!({"ok":false,"error":"draft_already_deleted"}),
        json!({"ok":true}),
        json!({"ok":true,"draft":null}),
        json!({"ok":true,"draft":[]}),
        json!({"ok":true,"draft":{"id":"D2"}}),
        json!({"ok":"true","draft":{"id":"D1"}}),
        json!({"draft":{"id":"D1"}}),
    ] {
        assert!(decode_drafts_info("D1", raw).is_err());
    }
}
#[test]
fn lifecycle_flags_are_retained_evidence_and_not_delivery_or_deletion_receipts() {
    for flags in [
        json!({"is_deleted":true,"is_sent":false}),
        json!({"is_deleted":false,"is_sent":true}),
        json!({"is_deleted":"malformed","is_sent":17}),
    ] {
        let mut draft = flags;
        draft["id"] = json!("D1");
        let raw = json!({"ok":true,"draft":draft});
        let result = decode_drafts_info("D1", raw.clone()).unwrap();
        assert_eq!(result.raw(), &raw);
        // No conversion to RemoteDraft/Observation/Outcome exists on DraftInfo.
    }
}

#[test]
fn contradictory_error_rejects_positive_payload_but_null_and_warnings_are_retained() {
    let raw = json!({"ok":true,"draft":{"id":"D1"},"warning":"ordinary warning","response_metadata":{"warnings":["ordinary warning"]}});
    assert_eq!(decode_drafts_info("D1", raw.clone()).unwrap().raw(), &raw);
    for error in [
        json!("failed"),
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
    ] {
        let mut changed = raw.clone();
        changed["error"] = error;
        assert!(decode_drafts_info("D1", changed).is_err());
    }
    let mut with_null = raw;
    with_null["error"] = Value::Null;
    assert_eq!(
        decode_drafts_info("D1", with_null.clone()).unwrap().raw(),
        &with_null
    );
}
