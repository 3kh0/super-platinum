use super::*;
use serde_json::json;
const UUID: &str = "11111111-2222-4333-8444-555555555555";
fn seed() -> (Target, Message, ReplyPolicy) {
    let d=Document::parse(json!({"id":"F01234567","mode":"list","access":"read","list_metadata":{"schema":[],"views":[]}})).unwrap();
    let r = Record::parse(json!({"id":"Rec1","fields":[]})).unwrap();
    let p = Page::parse(
        &Target::capture(&d, &r).unwrap(),
        json!({"ok":true,"messages":[{
        "type":"message","subtype":"list_record_comment","ts":"1700000000.000001",
        "slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}}]}),
    )
    .unwrap();
    (
        p.target().clone(),
        p.root().unwrap().clone(),
        ReplyPolicy::observed(json!({"id":"C01234567"}), false, true),
    )
}
fn intent() -> ReplyIntent {
    let (t, r, p) = seed();
    ReplyIntent::capture_rich("TTEST","UTEST",&t,&r,&p,vec![json!({"type":"rich_text","opaque":7,"elements":[{
        "type":"rich_text_section","elements":[{"type":"text","text":"Exact \u{4e8c}\n", "style":{"bold":true}},
        {"type":"channel","channel_id":"CMENTION01","opaque":"kept"}]}]})],UUID,"1700000001.000001").unwrap()
}
fn ack(r: &ReplyRecord) -> Value {
    json!({"ok":true,"channel":r.target().channel_id(),"ts":"1700000002.000001",
    "message":{"type":"message","ts":"1700000002.000001","thread_ts":r.target().root_ts(),
    "user":r.user(),"client_msg_id":r.client_msg_id(),"blocks":r.blocks(),"unknown":{"kept":true}}})
}
#[test]
fn acquired_root_rich_payload_and_roundtrip_are_exact_inert_records() {
    let i = intent();
    let r = i.record();
    assert_eq!(r.blocks()[0]["opaque"], 7);
    assert_eq!(
        r.blocks()[0]["elements"][0]["elements"][1]["opaque"],
        "kept"
    );
    let restored: ReplyRecord = serde_json::from_value(serde_json::to_value(r).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(&restored, r);
    let a = i.accept(ack(&restored)).unwrap();
    assert_ne!(a.message().ts.as_deref(), Some(restored.client_ts()));
    assert_eq!(a.message().extra["unknown"]["kept"], true);
}
#[test]
fn permissions_root_identity_and_fresh_root_mutations_refuse() {
    let (t, r, p) = seed();
    assert!(ReplyIntent::available(&t, &r, &p));
    for p in [
        ReplyPolicy::observed(json!({"id":t.channel_id()}), true, true),
        ReplyPolicy::observed(json!({"id":t.channel_id()}), false, false),
        ReplyPolicy::observed(json!({"id":"C76543210"}), false, true),
        ReplyPolicy::observed(
            json!({"id":t.channel_id(),"is_non_threadable":true}),
            false,
            true,
        ),
    ] {
        assert!(!ReplyIntent::available(&t, &r, &p));
    }
    let mut locked = r.clone();
    locked.extra.insert("is_locked".into(), json!(true));
    assert!(!ReplyIntent::available(&t, &locked, &p));
    let d = Document::parse(t.document_raw().clone()).unwrap();
    let row = Record::parse(t.record_raw().clone()).unwrap();
    let synthetic = Target::capture(&d, &row).unwrap();
    assert!(synthetic.root_ts().is_none());
    assert!(!ReplyIntent::available(&synthetic, &r, &p));
    let mut changed = r.clone();
    changed.extra.insert("opaque_new_revision".into(), json!(1));
    assert!(intent().revalidate(&d, &row, &changed, &p).is_err());
}
#[test]
fn malformed_ack_never_confirms_but_matching_readback_can() {
    for (pointer, value) in [
        ("/channel", json!("C76543210")),
        ("/message/client_msg_id", json!("other")),
        ("/message/user", Value::Null),
        ("/message/thread_ts", json!("1700000009.000001")),
        ("/message/blocks", json!([])),
        ("/ts", json!("1700000009.000001")),
    ] {
        let i = intent();
        let mut a = ack(i.record());
        *a.pointer_mut(pointer).unwrap() = value;
        assert!(i.accept(a).is_err(), "{pointer}");
    }
    let i = intent();
    let r = i.record();
    let empty = Page::parse(r.target(), json!({"ok":true,"messages":[]})).unwrap();
    assert!(r.observed(&empty).unwrap().is_none());
    let a = ack(r);
    let page = Page::parse(r.target(), json!({"ok":true,"messages":[a["message"]]})).unwrap();
    assert!(r.observed(&page).unwrap().is_some());
    let mut duplicate = a["message"].clone();
    duplicate["ts"] = json!("1700000003.000001");
    let page = Page::parse(
        r.target(),
        json!({"ok":true,"messages":[a["message"],duplicate]}),
    )
    .unwrap();
    assert!(r.observed(&page).is_err());
}
#[test]
fn attempts_are_spent_once_even_after_rejection_or_uncertainty() {
    for phase in [
        ReplyPhase::NotSent,
        ReplyPhase::Rejected,
        ReplyPhase::Uncertain,
    ] {
        let mut a = ReplyAttempt::new(intent());
        assert!(a.take().is_some());
        assert!(a.take().is_none());
        assert!(a.settle(phase));
        assert!(!a.settle(ReplyPhase::Ready));
        assert!(a.take().is_none());
    }
    let mut a = ReplyAttempt::new(intent());
    let i = a.take().unwrap();
    let raw = ack(i.record());
    let accepted = i.accept(raw).unwrap();
    assert!(a.accepted(&accepted));
    assert!(a.take().is_none());
}
#[test]
fn inert_record_tampering_empty_content_and_unknown_rich_nodes_refuse() {
    let i = intent();
    let raw = serde_json::to_value(i.record()).unwrap();
    for (key, value) in [
        ("version", json!(2)),
        ("client_ts", json!("bad")),
        ("client_msg_id", json!("bad")),
        ("text", json!("changed")),
        ("blocks", json!([])),
    ] {
        let mut raw = raw.clone();
        raw[key] = value;
        let r: ReplyRecord = serde_json::from_value(raw).unwrap();
        assert!(r.validate().is_err());
    }
    let (t, r, p) = seed();
    for text in ["", "   ", "\u{0000}"] {
        assert!(
            ReplyIntent::capture(
                "TTEST",
                "UTEST",
                &t,
                &r,
                &p,
                text,
                UUID,
                "1700000001.000001"
            )
            .is_err()
        );
    }
    assert!(
        ReplyIntent::capture_rich(
            "TTEST",
            "UTEST",
            &t,
            &r,
            &p,
            vec![json!({"type":"new_unknown"})],
            UUID,
            "1700000001.000001"
        )
        .is_err()
    );
}
