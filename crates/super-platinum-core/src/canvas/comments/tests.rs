use super::*;
use serde_json::json;
fn root() -> Value {
    json!({"type":"message","subtype":"document_comment_root","ts":"10.000001","document_comment":{"thread_id":"quip-thread"},"text":"root"})
}
fn reply() -> Value {
    json!({"type":"message","ts":"10.000002","thread_ts":"10.000001","text":"reply"})
}
#[test]
fn stream_keeps_roots_and_matching_latest_reply_in_source_order() {
    let page = StreamPage::parse("F123", &json!({"ok":true,"messages":[reply(),root()]})).unwrap();
    assert_eq!(page.threads.len(), 1);
    assert_eq!(
        page.threads[0].last_reply.as_ref().unwrap().text.as_deref(),
        Some("reply")
    );
}
#[test]
fn errors_are_not_an_empty_comment_stream() {
    for value in [
        json!({"ok":false,"error":"channel_not_found","messages":[]}),
        json!({"ok":true,"error":"denied","messages":[]}),
        json!({"ok":true}),
    ] {
        assert!(StreamPage::parse("F123", &value).is_err());
    }
}
#[test]
fn thread_reply_cannot_be_adopted_from_another_parent_or_channel() {
    let mut foreign = reply();
    foreign["thread_ts"] = json!("20.000001");
    assert!(
        ReplyPage::parse(
            "F123",
            "10.000001",
            true,
            &json!({"ok":true,"messages":[root(),foreign]})
        )
        .is_err()
    );
    let mut foreign = root();
    foreign["channel"] = json!("C456");
    assert!(StreamPage::parse("F123", &json!({"ok":true,"messages":[foreign]})).is_err());
}
#[test]
fn initial_replies_require_the_exact_document_root_but_later_pages_do_not() {
    let page = json!({"ok":true,"messages":[reply()]});
    assert!(ReplyPage::parse("F123", "10.000001", true, &page).is_err());
    assert_eq!(
        ReplyPage::parse("F123", "10.000001", false, &page)
            .unwrap()
            .messages
            .len(),
        1
    );
}
#[test]
fn unknown_root_shape_and_duplicate_ids_fail_closed() {
    let mut bad = root();
    bad["document_comment"] = json!({});
    for rows in [vec![bad], vec![root(), root()]] {
        assert!(StreamPage::parse("F123", &json!({"ok":true,"messages":rows})).is_err());
    }
}
#[test]
fn pagination_requires_a_usable_cursor_when_more_is_true() {
    for metadata in [
        json!({}),
        json!({"next_cursor":12}),
        json!({"next_cursor":"\n"}),
    ] {
        assert!(
            StreamPage::parse(
                "F123",
                &json!({"ok":true,"messages":[],"has_more":true,"response_metadata":metadata})
            )
            .is_err()
        );
    }
    assert_eq!(StreamPage::parse("F123",&json!({"ok":true,"has_more":true,"messages":[],"response_metadata":{"next_cursor":"opaque:/+="}})).unwrap().cursor.as_deref(),Some("opaque:/+="));
}
#[test]
fn no_numeric_rounding_or_quip_id_to_slack_timestamp_guess() {
    for ts in [
        "Qpc:12345678",
        "NaN",
        "1",
        "1.0000001",
        "1.1.2",
        "-1.000001",
    ] {
        assert!(!timestamp(ts));
    }
    assert!(timestamp("1719800000.000001"));
    assert_eq!(channel("F123").unwrap(), "C123");
    assert!(channel("C123").is_err());
    assert!(channel("F123&channel=C4").is_err());
}
#[test]
fn replies_sort_exact_timestamps_without_mutating_payload() {
    let page = ReplyPage::parse(
        "F123",
        "10.000001",
        true,
        &json!({"ok":true,"messages":[reply(),root()]}),
    )
    .unwrap();
    assert_eq!(page.messages[0].text.as_deref(), Some("root"));
    assert_eq!(page.messages[1].text.as_deref(), Some("reply"));
}
