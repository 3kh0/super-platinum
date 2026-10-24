use super::*;
use crate::canvas::wire::EditAck;
use serde_json::{Value, json};

mod ack_content;
pub(crate) mod formatted_title;
mod headings;
mod phases;
mod underline;
mod whitespace;

fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn t(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
pub(crate) fn fixture() -> CanvasCollabSnapshot {
    let mut snapshot = super::super::tests::plain_without_crdt();
    let original = &snapshot.sections[0];
    let text = msg(&[t(1, "A <b>bold</b> line 🦊 日本語."), t(2, "en")]);
    snapshot.sections[0] = Section::from_raw(
        original
            .raw()
            .remove(38)
            .unwrap()
            .replace_fields(13, &[m(13, &msg(&[]))])
            .unwrap()
            .replace_fields(12, &[m(12, &msg(&[m(1, &text)]))])
            .unwrap(),
    )
    .unwrap();
    snapshot
}
fn changed() -> TextDraft {
    TextDraft::from_rtml("A <b>bold</b> line 🦊 日本語. <i>Added</i>").unwrap()
}
fn accepted(prepared: &PreparedTextEdit, sequence: u64, expiry: u64) -> AcceptedText {
    let section = prepared.document_data().message(1).unwrap().unwrap();
    let wire = section
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, expiry)])
        .unwrap();
    let raw = prepared
        .canonical
        .raw()
        .replace_fields(2, &[n(2, sequence)])
        .unwrap()
        .replace_fields(12, &[m(12, &section.message(12).unwrap().unwrap())])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, &raw)]))]).raw()).unwrap();
    prepared.validate_ack(&ack).unwrap()
}
fn canonical(snapshot: &CanvasCollabSnapshot, accepted: &AcceptedText) -> CanvasCollabSnapshot {
    let mut fresh = snapshot.clone();
    fresh.sections[0] =
        Section::from_raw(retained::association(accepted.section()).unwrap()).unwrap();
    fresh
}

#[test]
fn captured_visitor_marks_use_escaped_utf16_and_soft_break_rules() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/marks.json")).unwrap();
    for case in cases {
        let source = case["rtml"].as_str().unwrap();
        if case["name"] == "control_normalization" {
            continue;
        } // HTML repair rejected conservatively.
        let draft = TextDraft::from_rtml(source).unwrap();
        let model = draft.model().unwrap();
        assert_eq!(model.text, case["text"].as_str().unwrap());
        let canonical_marks = |marks: Vec<Value>| {
            let mut result = std::collections::BTreeMap::new();
            for mark in marks {
                for i in mark["start"].as_u64().unwrap()..mark["end"].as_u64().unwrap() {
                    result.insert(
                        (i, mark["name"].as_str().unwrap().to_owned()),
                        mark["value"].clone(),
                    );
                }
            }
            result
        };
        let got=model.marks.iter().map(|mark|json!({"name":mark.name(),"value":match mark.value(){automerge::ScalarValue::Boolean(b)=>json!(b),automerge::ScalarValue::Str(s)=>json!(s.to_string()),_=>panic!("unexpected mark")},"start":mark.start,"end":mark.end})).collect();
        assert_eq!(
            canonical_marks(got),
            canonical_marks(case["marks"].as_array().unwrap().clone())
        );
    }
}

#[test]
fn unknown_markup_attributes_and_multiple_rich_paragraphs_never_flatten() {
    for source in [
        "<span>unsupported</span>",
        "<control id='x'></control>",
        "<b data-extra='1'>x</b>",
        "<a href='x' data-nolink='true'>x</a>",
        "<b>unclosed",
        "<!--hidden-->x",
    ] {
        assert!(
            TextDraft::from_rtml(source).is_err(),
            "must reject {source}"
        );
    }
    let draft = changed();
    assert!(TextDraft::from_rich_blocks(&draft.rich_blocks()).is_ok());
    let mut blocks = draft.rich_blocks();
    blocks[0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"rich_text_section","elements":[]}));
    assert!(TextDraft::from_rich_blocks(&blocks).is_err());
    let mut blocks = draft.rich_blocks();
    blocks[0]["elements"][0]["elements"][0]["style"] = json!({"highlight":true});
    assert!(TextDraft::from_rich_blocks(&blocks).is_err());
    assert!(
        TextDraft {
            runs: vec![TextRun {
                text: "\n".into(),
                style: TextStyle {
                    bold: true,
                    ..Default::default()
                }
            }]
        }
        .rtml()
        .is_err()
    );
}

#[test]
fn text_projection_preserves_language_unrelated_sections_and_shared_counter() {
    let snapshot = fixture();
    let before = snapshot.sections.clone();
    let mut session = EditSession::new();
    let intent = TextIntent::capture(&snapshot, "item", 100).unwrap();
    let prepared = intent
        .prepare(&snapshot, &mut session, &changed(), 100)
        .unwrap();
    let update = prepared.document_data().message(1).unwrap().unwrap();
    assert_eq!(
        update.fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
        vec![1, 2, 9, 12, 13, 19, 20, 33, 38]
    );
    assert!(!update.has(16));
    assert!(!update.has(39));
    assert_eq!(
        update
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap()
            .string(2)
            .unwrap(),
        Some("en")
    );
    assert!(snapshot.sections == before);
    assert_eq!(prepared.sequence(), 1);
    assert_eq!(session.sequence, 1);
    let wire = update.message(38).unwrap().unwrap();
    assert!(wire.has(12));
    assert!(!wire.has(4));
    crdt::validate(&wire, &snapshot.sections[0], prepared.desired(), None).unwrap();
}

#[test]
fn stale_target_title_and_noop_reserve_no_counter_or_actor() {
    let snapshot = fixture();
    let intent = TextIntent::capture(&snapshot, "item", 100).unwrap();
    let mut session = EditSession::new();
    let id = session.id.clone();
    assert!(matches!(
        intent.prepare(&snapshot, &mut session, intent.seed(), 100),
        Err(Error::Unchanged)
    ));
    let mut fresh = snapshot.clone();
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(99, &[n(99, 1)])
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        intent.prepare(&fresh, &mut session, &changed(), 100),
        Err(Error::Changed)
    ));
    let mut fresh = snapshot.clone();
    let title = fresh
        .sections
        .iter_mut()
        .find(|s| s.section_type().unwrap() == Some(64))
        .unwrap();
    *title = Section::from_raw(title.raw().replace_fields(2, &[n(2, 10_001)]).unwrap()).unwrap();
    assert!(matches!(
        intent.prepare(&fresh, &mut session, &changed(), 100),
        Err(Error::Changed)
    ));
    assert_eq!(session.sequence, 0);
    assert_eq!(session.id, id);
}

#[test]
fn later_canonical_edit_settles_sealed_receipt_but_equal_revision_must_match() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let prepared = TextIntent::capture(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, &mut session, &changed(), 100)
        .unwrap();
    let receipt = accepted(&prepared, 12_000, 1_000);
    let mut fresh = canonical(&snapshot, &receipt);
    let later = TextDraft::from_rtml("Another author's later paragraph").unwrap();
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(12, &[m(12, &content(&fresh.sections[0], &later).unwrap())])
            .unwrap(),
    )
    .unwrap();
    assert!(
        !receipt.confirms_readback(&fresh).unwrap(),
        "equal sequence cannot bless inconsistent text"
    );
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(2, &[n(2, 13_000)])
            .unwrap(),
    )
    .unwrap();
    assert!(
        receipt.confirms_readback(&fresh).unwrap(),
        "a newer canonical edit must not trap AwaitingRead"
    );
    // Rebuild through the canonical loader: mutating public file metadata alone
    // would leave Snapshot's validated, cached file identity unchanged.
    let mut file = fresh.file.clone();
    file.id = Some("AnotherFile".into());
    let mut objects = vec![m(3, &fresh.thread), m(6, &fresh.document)];
    let mut index = vec![t(1, &format!("Section/{}", fresh.document_id()))];
    for section in &fresh.sections {
        objects.push(m(7, section.raw()));
        index.push(m(2, &msg(&[t(1, section.id())])));
    }
    let response = msg(&[n(1, 1), m(2, &msg(&objects)), m(3, &msg(&index))]);
    let load =
        crate::canvas::wire::LoadDataResponse::decode(msg(&[m(2, &response)]).raw()).unwrap();
    let other = CanvasCollabSnapshot::from_responses(file, fresh.init.clone(), load).unwrap();
    assert_eq!(other.file_id(), "AnotherFile");
    assert!(
        !receipt.confirms_readback(&other).unwrap(),
        "new revision does not bypass owner/target binding"
    );
}

#[test]
fn partial_ack_omitted_top_level_parents_matches_explicit_empty_readback() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let prepared = TextIntent::capture(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, &mut session, &changed(), 100)
        .unwrap();
    let full = accepted(&prepared, 12_000, 1_000);
    let partial = full.section().raw().remove(13).unwrap();
    let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, &partial)]))]).raw()).unwrap();
    let receipt = prepared.validate_ack(&ack).unwrap();
    assert!(!receipt.section().raw().has(13));
    let mut fresh = canonical(&snapshot, &full);
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(13, &[m(13, &msg(&[]))])
            .unwrap(),
    )
    .unwrap();
    assert!(
        fresh.sections[0]
            .parents()
            .unwrap()
            .is_some_and(|p| p.fields().is_empty())
    );
    assert!(receipt.confirms_readback(&fresh).unwrap());
    // Exact stock capture direction: ACK Parents13 is an explicit zero-byte
    // message, while a complete load omits Parents13 on top-level sections.
    let mut omitted = fresh.clone();
    omitted.sections[0] = Section::from_raw(omitted.sections[0].raw().remove(13).unwrap()).unwrap();
    assert!(full.section().raw().has(13));
    assert!(!omitted.sections[0].raw().has(13));
    assert!(full.confirms_readback(&omitted).unwrap());
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(13, &[m(13, &msg(&[t(1, "unexpected-parent")]))])
            .unwrap(),
    )
    .unwrap();
    assert!(
        !receipt.confirms_readback(&fresh).unwrap(),
        "nonempty parents at the same revision remain inconsistent"
    );
}

#[test]
fn two_saves_adopt_ack_history_while_expiry_rotates_and_late_ack_cannot_poison() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let intent = TextIntent::capture(&snapshot, "item", 100).unwrap();
    let first = intent
        .prepare(&snapshot, &mut session, &changed(), 100)
        .unwrap();
    let receipt = accepted(&first, 12_000, 1_000);
    session.remember_text_accepted(&receipt).unwrap();
    let fresh = canonical(&snapshot, &receipt);
    assert!(receipt.confirms_readback(&fresh).unwrap());
    let second = TextIntent::capture(&fresh, "item", 200)
        .unwrap()
        .prepare(&fresh, &mut session, intent.seed(), 200)
        .unwrap();
    assert_eq!(first.session_id(), second.session_id());
    assert_eq!(second.sequence(), 2);
    let receipt2 = accepted(&second, 13_000, 300);
    session.remember_text_accepted(&receipt2).unwrap();
    let fresh2 = canonical(&fresh, &receipt2);
    let third = TextIntent::capture(&fresh2, "item", 400)
        .unwrap()
        .prepare(&fresh2, &mut session, &changed(), 400)
        .unwrap();
    assert_ne!(third.session_id(), first.session_id());
    assert_eq!(third.sequence(), 1);
    assert!(matches!(
        session.remember_text_accepted(&receipt2),
        Err(Error::Changed)
    ));
    assert!(
        second.validate_snapshot(&fresh, 1_000).is_err(),
        "retained expiry rechecked at send"
    );
}

#[test]
fn captured_empty_ack_parents_to_omitted_load_reuses_session_and_history() {
    use automerge::ReadDoc;
    let snapshot = fixture();
    let original = TextIntent::capture(&snapshot, "item", 100).unwrap();
    let mut session = EditSession::new();
    let first = original
        .prepare(&snapshot, &mut session, &changed(), 100)
        .unwrap();
    let receipt = accepted(&first, 12_000, 1_000);
    assert!(
        receipt
            .section()
            .parents()
            .unwrap()
            .is_some_and(|p| p.fields().is_empty())
    );
    session.remember_text_accepted(&receipt).unwrap();
    let mut fresh = snapshot.clone();
    // Actual full-load shape, not a helper that also normalizes the ACK.
    fresh.sections[0] = Section::from_raw(
        receipt
            .section()
            .raw()
            .remove(13)
            .unwrap()
            .remove(38)
            .unwrap(),
    )
    .unwrap();
    assert!(!fresh.sections[0].raw().has(13));
    let second = TextIntent::capture(&fresh, "item", 200)
        .unwrap()
        .prepare(&fresh, &mut session, original.seed(), 200)
        .unwrap();
    assert_eq!(second.session_id(), first.session_id());
    assert_eq!(second.sequence(), 2);
    let wire = second
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap();
    let prior = receipt.section().crdt().unwrap().unwrap();
    assert_eq!(
        wire.uint(2).unwrap(),
        prior.uint(2).unwrap(),
        "base sequence is retained, not replaced by fresh section revision"
    );
    assert_eq!(wire.string(8).unwrap(), prior.string(8).unwrap());
    assert_eq!(wire.uint(3).unwrap(), Some(12_000));
    let previous_head = first
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap()
        .string(12)
        .unwrap()
        .unwrap()
        .parse()
        .unwrap();
    let history = automerge::Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    assert!(
        history.get_change_by_hash(&previous_head).is_some(),
        "second save extends the first acknowledged history"
    );
}

#[test]
fn sealed_ack_rejects_wrong_content_multisection_and_server_validation_failure() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let prepared = TextIntent::capture(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, &mut session, &changed(), 100)
        .unwrap();
    let receipt = accepted(&prepared, 12_000, 1_000);
    let valid = receipt.section.raw();
    for sections in [
        vec![m(1, valid), m(1, valid)],
        vec![m(1, snapshot.sections[0].raw())],
    ] {
        let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&sections))]).raw());
        assert!(ack.is_err() || prepared.validate_ack(&ack.unwrap()).is_err());
    }
    let bad = valid
        .replace_fields(
            38,
            &[m(
                38,
                &valid
                    .message(38)
                    .unwrap()
                    .unwrap()
                    .replace_fields(7, &[n(7, 1)])
                    .unwrap(),
            )],
        )
        .unwrap();
    let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, &bad)]))]).raw()).unwrap();
    assert!(prepared.validate_ack(&ack).is_err());
}

#[test]
fn captured_javascript_styled_bootstrap_matches_native_history() {
    use automerge::ReadDoc;
    let fixture: Value = serde_json::from_str(include_str!("fixtures/automerge.json")).unwrap();
    let bytes = fixture["bytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    let js = automerge::Automerge::load(&bytes).unwrap();
    let source = super::tests::fixture();
    let draft = TextDraft::from_rtml("<b>A&amp;😀</b><br>B").unwrap();
    let wire = crdt::bootstrap(
        &source.sections[0],
        &draft,
        "012345abcdef",
        1_791_897_000_000_000,
    )
    .unwrap();
    let native = automerge::Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    assert_eq!(
        native.get_heads(),
        js.get_heads(),
        "exact source actor, UTF16 marks, expansion and commit time"
    );
    let object = js.get(automerge::ROOT, "text").unwrap().unwrap().1;
    assert_eq!(js.text(object).unwrap(), "A&amp;😀<br>B\u{feff}");
}

/// Root runs this offline exporter, then the captured official JS loader. Its
/// artifact is synthetic only and is never a mutation of a real canvas.
#[test]
#[ignore = "explicit offline captured-JavaScript interoperability export"]
fn export_native_text_edits_for_captured_javascript() {
    let path = std::env::var("CANVAS_TEXT_ORACLE_OUTPUT").expect("explicit synthetic output path");
    let snapshot = fixture();
    let section = &snapshot.sections[0];
    let mut cases = Vec::new();
    for (original, desired) in [
        ("<b>A&amp;😀</b><br>B", "<b>A&amp;😀</b><br>B <i>added</i>"),
        (
            "<a href=\"https://example.test\">go😀</a>",
            "<a href=\"https://example.test/next\"><b>go🦊</b></a>",
        ),
        ("<b>Delete bold</b>", "plain"),
        ("<b>Empty</b>", ""),
    ] {
        let original = TextDraft::from_rtml(original).unwrap();
        let desired = TextDraft::from_rtml(desired).unwrap();
        let seed =
            crdt::bootstrap(section, &original, "012345abcdef", 1_791_897_000_000_000).unwrap();
        let update = crdt::update(
            &seed,
            section,
            &original,
            &desired,
            "012345abcdef",
            1_791_897_001_000_000,
        )
        .unwrap();
        let model = desired.model().unwrap();
        let marks=model.marks.into_iter().map(|m|json!({"name":m.name(),"value":match m.value(){automerge::ScalarValue::Boolean(b)=>json!(b),automerge::ScalarValue::Str(s)=>json!(s.to_string()),_=>panic!("mark")},"start":m.start,"end":m.end})).collect::<Vec<_>>();
        cases.push(json!({"text":format!("{}\u{feff}",model.text),"marks":marks,"bytes":update.bytes(5).unwrap().unwrap(),"head":update.string(12).unwrap().unwrap()}));
    }
    std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
}
