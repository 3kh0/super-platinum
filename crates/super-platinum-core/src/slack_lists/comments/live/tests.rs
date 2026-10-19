use super::*;
use crate::slack::{events::RtEvent, realtime::parse_event};
use crate::slack_lists::comments::{Target, projection::Projection};
use serde_json::json;

const CHANNEL: &str = "C01234567";
const ROOT: &str = "1700000000.000001";
const REPLY: &str = "1700000001.000001";

fn page() -> Page {
    let (document, record, root) = crate::slack_lists::comments::tests::seed();
    let target = Target::capture(&document, &record).unwrap();
    Page::parse(
        &target,
        json!({"ok":true,"channel":CHANNEL,"messages":[root],
        "has_more":true,"response_metadata":{"next_cursor":"opaque+/=cursor"},
        "opaque_receipt":{"keep":true}}),
    )
    .unwrap()
}

fn message(patch: Value) -> Message {
    let mut raw = json!({"type":"message","channel":CHANNEL,"ts":REPLY,"thread_ts":ROOT,
        "user":"U1","text":"New reply","opaque":{"keep":[1,2]},
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section",
            "elements":[{"type":"text","text":"New reply","style":{"bold":true}}]}]}],
        "reactions":[{"name":"wave","count":1,"users":["U2"],"opaque":"keep"}]});
    raw.as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    serde_json::from_value(raw).unwrap()
}

#[test]
fn parsed_socket_reply_is_admitted_without_mutating_page_or_event() {
    let page = page();
    let original = message(json!({}));
    let wire = serde_json::to_value(&original).unwrap();
    let RtEvent::Message(incoming) = parse_event(&wire.to_string()).unwrap() else {
        panic!()
    };
    let before = (
        page.raw().clone(),
        page.target().clone(),
        serde_json::to_value(page.messages()).unwrap(),
    );
    let event_before = serde_json::to_value(&incoming).unwrap();
    assert!(page.accepts_live_reply(&incoming));
    assert_eq!(page.raw(), &before.0);
    assert_eq!(page.target(), &before.1);
    assert_eq!(serde_json::to_value(page.messages()).unwrap(), before.2);
    assert_eq!(serde_json::to_value(incoming).unwrap(), event_before);
    assert_eq!(page.cursor(), Some("opaque+/=cursor"));
    assert!(page.has_more());
    assert_eq!(page.root().unwrap().ts.as_deref(), Some(ROOT));
}

#[test]
fn explicit_channel_and_valid_nonroot_thread_identity_are_required() {
    let page = page();
    for patch in [
        json!({"channel":null}),
        json!({"channel":""}),
        json!({"channel":"COTHER"}),
        json!({"ts":null}),
        json!({"ts":""}),
        json!({"ts":"invalid"}),
        json!({"ts":ROOT}),
        json!({"thread_ts":null}),
        json!({"thread_ts":""}),
        json!({"thread_ts":"invalid"}),
        json!({"thread_ts":"1700000999.000001"}),
    ] {
        assert!(!page.accepts_live_reply(&message(patch.clone())), "{patch}");
    }
    let mut unresolved = page.clone();
    // Public Page parsing already requires resolution; retain a defensive check.
    unresolved.target.root = None;
    assert!(!unresolved.accepts_live_reply(&message(json!({}))));
}

#[test]
fn original_deleted_receipt_prevents_admission_without_rewriting_it() {
    let original = page();
    let mut raw = original.raw().clone();
    raw["deleted"] = json!([REPLY]);
    let page = Page::parse(original.target(), raw.clone()).unwrap();
    assert!(!page.accepts_live_reply(&message(json!({}))));
    assert!(page.accepts_live_reply(&message(json!({"ts":"1700000002.000001"}))));
    assert_eq!(page.raw(), &raw);
}

#[test]
fn unconditional_hidden_and_deleted_rows_use_existing_truthiness() {
    let page = page();
    for subtype in ["tombstone", "message_deleted"] {
        assert!(!page.accepts_live_reply(&message(json!({"subtype":subtype}))));
    }
    for key in [
        "isNonExistent",
        "no_display",
        "_hidden_stub",
        "is_deleted",
        "is_tombstoned",
    ] {
        for value in [json!(true), json!("yes"), json!(1), json!([]), json!({})] {
            assert!(!page.accepts_live_reply(&message(json!({key:value}))));
        }
        for value in [Value::Null, json!(false), json!(0), json!("")] {
            assert!(page.accepts_live_reply(&message(json!({key:value}))));
        }
    }
    // Existing Projection deliberately does not hide this flag.
    assert!(page.accepts_live_reply(&message(json!({"_hidden_reply":true}))));
}

#[test]
fn actual_observed_projection_and_duplicate_checks_remain_caller_owned() {
    let mut page = page();
    let incoming = message(json!({"subtype":"list_edit","list_edit":{}}));
    let projection = Projection::observed(json!({"ok":true,"assignments":{
        "lists_pilot":{"group":"on"},"lists_pilot_teams":{"group":"on"}}}))
    .unwrap();
    assert!(page.accepts_live_reply(&incoming));
    assert!(
        projection
            .visible(std::slice::from_ref(&incoming), ROOT)
            .is_empty()
    );
    page.messages.push(incoming.clone());
    assert!(page.accepts_live_reply(&incoming));
    assert_eq!(page.messages().len(), 2);
}
