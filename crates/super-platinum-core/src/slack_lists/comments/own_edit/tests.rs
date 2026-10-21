use super::*;
use crate::slack_lists::{Document, Record};
use serde_json::json;
const NOW: u64 = 1_700_000_061_000;

fn page() -> Page {
    let doc = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let row =
        Record::parse(json!({"id":"Rec1","fields":[],"created_by":"UOWNER","date_created":1}))
            .unwrap();
    Page::parse(&Target::capture(&doc, &row).unwrap(), json!({"ok":true,"has_more":true,
        "response_metadata":{"next_cursor":"original-cursor"}, "messages":[
        {"ts":"1700000000.000001","type":"message","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
        {"ts":"1700000001.000001","thread_ts":"1700000000.000001","user":"UOWNER","type":"message","text":"Original",
        "blocks":[{"type":"context","elements":[{"type":"plain_text","text":"Preserved"}],"opaque":{"original":true}},
        {"type":"rich_text","block_id":"original-rich","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Original","style":{"bold":true},"opaque":7},{"type":"user","user_id":"U1","opaque":9}]}]}],
        "reactions":[{"name":"wave","count":1,"users":["U1"]}],"opaque_message":{"untouched":true}}]})).unwrap()
}
fn policy() -> OwnEditPolicy {
    OwnEditPolicy::observed(
        "T1",
        "UOWNER",
        json!({"id":"C01234567"}),
        true,
        false,
        vec![-1],
        false,
        false,
    )
    .unwrap()
}
fn seed() -> OwnEditSeed {
    let p = page();
    OwnEditSeed::capture("T1", "UOWNER", &p, &p.messages()[1], &policy(), NOW).unwrap()
}
fn intent() -> OwnEditIntent {
    let p = page();
    OwnEditIntent::capture(
        &seed(),
        &p.messages()[1],
        &policy(),
        content::plain("Changed *literal* \u{4e2d}"),
        NOW,
    )
    .unwrap()
}
fn ack(r: &OwnEditRecord) -> Value {
    json!({"ok":true,"channel":r.target().channel_id(),"ts":r.ts(),
        "message":{"text":"Changed fallback","blocks":r.blocks(),"edited":{"user":r.user(),"ts":"1700000062.000001"}}})
}

#[test]
fn exact_http_rich_seed_and_context_append_once_are_inert() {
    let p = page();
    let s = seed();
    assert_eq!(s.http_raw(), p.raw());
    assert_eq!(s.original_raw(), &p.raw()["messages"][1]);
    assert_eq!(s.editable()[0]["elements"][0]["elements"][0]["opaque"], 7);
    assert_eq!(
        s.preserved(),
        &p.raw()["messages"][1]["blocks"].as_array().unwrap()[..1]
    );
    let i = intent();
    let r = i.record();
    assert_eq!(r.blocks().len(), 2);
    assert_eq!(r.blocks()[1], s.preserved()[0]);
    let restored: OwnEditRecord = serde_json::from_value(serde_json::to_value(r).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(&restored, r);
    let mut corrupt = serde_json::to_value(r).unwrap();
    corrupt["seed"]["editable"][0]["elements"][0]["elements"][0]["text"] = json!("forged");
    assert!(
        serde_json::from_value::<OwnEditRecord>(corrupt)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn original_http_required_after_socket_body_change_but_reactions_are_independent() {
    let mut p = page();
    let s = seed();
    p.messages[1].reactions.clear();
    p.messages[1].reply_count = Some(99);
    p.messages[1].channel = Some("C01234567".into());
    assert!(s.current(&p.messages[1]));
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, &p.messages()[1], &policy(), NOW).is_ok());
    p.messages[1].blocks = content::plain("New socket body");
    assert!(!s.current(&p.messages[1]));
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, &p.messages()[1], &policy(), NOW).is_err());
    let mut removed = page();
    removed.messages.remove(1);
    assert!(
        OwnEditSeed::capture(
            "T1",
            "UOWNER",
            &removed,
            &page().messages()[1],
            &policy(),
            NOW
        )
        .is_err()
    );
}

#[test]
fn foreign_root_other_author_hidden_deleted_and_unsupported_shapes_refuse() {
    for (key, value) in [
        ("user", json!("UOTHER")),
        ("channel", json!("C76543210")),
        ("subtype", json!("reply_broadcast")),
        ("is_ephemeral", json!(true)),
        ("no_display", json!(true)),
        ("list_edit", json!(true)),
        ("pendingFiles", json!([{"id":"pending"}])),
        ("markdown_text", json!("portable")),
        ("files", json!([{"id":"F1"}])),
        ("attachments", json!([{"text":"unsupported"}])),
        ("blocks", json!([])),
        (
            "blocks",
            json!([{"type":"section","text":{"type":"mrkdwn","text":"bad"}}]),
        ),
    ] {
        let p = page();
        let mut raw = p.raw().clone();
        raw["messages"][1][key] = value;
        if let Ok(p) = Page::parse(p.target(), raw) {
            assert!(
                OwnEditSeed::capture("T1", "UOWNER", &p, &p.messages()[1], &policy(), NOW).is_err(),
                "{key}"
            );
        }
    }
    let p = page();
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, p.root().unwrap(), &policy(), NOW).is_err());
    let mut raw = p.raw().clone();
    raw["deleted"] = json!(["1700000001.000001"]);
    let p = Page::parse(p.target(), raw).unwrap();
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, &p.messages()[1], &policy(), NOW).is_err());
    let mut p = page();
    p.messages[0].extra.insert("is_locked".into(), json!(true));
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, &p.messages()[1], &policy(), NOW).is_err());
}

#[test]
fn positive_modes_windows_identity_and_fresh_policy_are_required() {
    let p = page();
    let known = &p.messages()[1];
    assert!(
        OwnEditPolicy::observed(
            "T1",
            "UOWNER",
            json!({"id":"C01234567"}),
            true,
            false,
            vec![],
            false,
            false
        )
        .is_err()
    );
    assert!(
        OwnEditPolicy::observed(
            "T1",
            "UOWNER",
            json!({"id":"C01234567"}),
            true,
            false,
            vec![-2],
            false,
            false
        )
        .is_err()
    );
    for (windows, plain, portable, can_thread) in [
        (vec![0], false, false, true),
        (vec![-1], true, false, true),
        (vec![-1], false, true, true),
        (vec![-1], false, false, false),
    ] {
        let pol = OwnEditPolicy::observed(
            "T1",
            "UOWNER",
            json!({"id":"C01234567"}),
            can_thread,
            false,
            windows,
            plain,
            portable,
        )
        .unwrap();
        assert!(OwnEditSeed::capture("T1", "UOWNER", &p, known, &pol, NOW).is_err());
    }
    let limited = OwnEditPolicy::observed(
        "T1",
        "UOWNER",
        json!({"id":"C01234567"}),
        true,
        false,
        vec![0, 1],
        false,
        false,
    )
    .unwrap();
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, known, &limited, NOW).is_ok());
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, known, &limited, NOW + 1).is_err());
    let unlimited = OwnEditPolicy::observed(
        "T1",
        "UOWNER",
        json!({"id":"C01234567"}),
        true,
        false,
        vec![0, 1, -1],
        false,
        false,
    )
    .unwrap();
    assert!(OwnEditSeed::capture("T1", "UOWNER", &p, known, &unlimited, NOW + 999999).is_ok());
    assert!(OwnEditSeed::capture("TOTHER", "UOWNER", &p, known, &policy(), NOW).is_err());
    assert!(seed().revalidate(&p, known, &limited, NOW + 1).is_err());
}

#[test]
fn empty_or_context_in_editable_input_cannot_delete_or_duplicate_context() {
    let p = page();
    for edited in [vec![], content::plain("  \n"), seed().preserved().to_vec()] {
        assert!(OwnEditIntent::capture(&seed(), &p.messages()[1], &policy(), edited, NOW).is_err());
    }
}

#[test]
fn strict_ack_wrong_identity_body_or_preserved_context_is_unconfirmed() {
    for (key, value) in [
        ("channel", json!("COTHER")),
        ("ts", json!("1700000009.000001")),
        ("ok", json!(false)),
    ] {
        let i = intent();
        let mut raw = ack(i.record());
        raw[key] = value;
        assert!(i.accept(raw).is_err());
    }
    for (key, value) in [
        ("user", json!("UOTHER")),
        ("thread_ts", json!("1700000009.000001")),
        ("channel", json!("COTHER")),
        ("ts", Value::Null),
        ("no_display", json!(true)),
        ("blocks", json!([])),
    ] {
        let i = intent();
        let mut raw = ack(i.record());
        raw["message"][key] = value;
        assert!(i.accept(raw).is_err(), "{key}");
    }
    let i = intent();
    let mut raw = ack(i.record());
    raw["message"]["blocks"][1]["opaque"] = json!(false);
    assert!(i.accept(raw).is_err());
}

#[test]
fn confirmed_typed_body_publication_preserves_current_reactions_raw_and_paging() {
    let i = intent();
    let raw = ack(i.record());
    let accepted = i.accept(raw.clone()).unwrap();
    let mut p = page();
    let receipt = p.raw().clone();
    let target = p.target().clone();
    p.messages[1].reactions.clear();
    p.messages[1]
        .extra
        .insert("later_unrelated".into(), json!(5));
    assert!(p.apply_own_edit(&accepted));
    assert_eq!(p.messages()[1].blocks, accepted.record().blocks());
    assert_eq!(p.messages()[1].text.as_deref(), Some("Changed fallback"));
    assert!(p.messages()[1].reactions.is_empty());
    assert_eq!(p.messages()[1].extra["later_unrelated"], 5);
    assert_eq!(p.raw(), &receipt);
    assert_eq!(p.target(), &target);
    assert_eq!(p.cursor(), Some("original-cursor"));
    assert!(p.has_more());
    assert!(!p.apply_own_edit(&accepted));
    let mut newer = page().messages()[1].clone();
    newer.blocks = content::plain("Newer socket body");
    assert!(accepted.apply(&newer).is_none());
}

#[test]
fn positive_readback_uses_original_http_not_mutated_typed_projection() {
    let i = intent();
    let record = i.record().clone();
    let accepted = i.accept(ack(&record)).unwrap();
    let mut p = page();
    assert!(p.apply_own_edit(&accepted));
    assert!(record.observed(&p).unwrap().is_none());
    let mut raw = p.raw().clone();
    raw["messages"][1]["blocks"] = json!(record.blocks());
    let fresh = Page::parse(p.target(), raw).unwrap();
    assert!(record.observed(&fresh).unwrap().is_some());
    let mut empty = fresh.raw().clone();
    empty["messages"].as_array_mut().unwrap().remove(1);
    assert!(
        record
            .observed(&Page::parse(p.target(), empty).unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn consuming_attempt_never_retries_and_terminal_receipts_remain_inert() {
    let mut attempt = OwnEditAttempt::new(intent());
    let intent = attempt.take().unwrap();
    assert!(attempt.take().is_none());
    let accepted = intent.accept(ack(attempt.record())).unwrap();
    assert!(attempt.accepted(&accepted));
    assert_eq!(attempt.phase(), OwnEditPhase::Accepted);
    assert!(attempt.take().is_none());
    assert!(!attempt.settle(OwnEditPhase::Uncertain));
    for phase in [
        OwnEditPhase::NotSent,
        OwnEditPhase::Rejected,
        OwnEditPhase::Uncertain,
    ] {
        let mut a = OwnEditAttempt::new(super::tests::intent());
        a.take().unwrap();
        assert!(a.settle(phase));
        assert!(a.take().is_none());
    }
}
