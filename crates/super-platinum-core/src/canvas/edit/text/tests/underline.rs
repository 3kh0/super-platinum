//! Selected actual retained visitor/Automerge outputs; ACKs below are synthetic.
use super::*;
use automerge::{ReadDoc, ScalarValue};
use std::collections::BTreeMap;

fn vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/underline.json")).unwrap()
}

fn signature(marks: &[automerge::marks::Mark]) -> BTreeMap<(usize, String), Value> {
    let mut result = BTreeMap::new();
    for mark in marks {
        let value = match mark.value() {
            ScalarValue::Boolean(value) => json!(value),
            ScalarValue::Str(value) => json!(value.to_string()),
            _ => panic!("unexpected mark value"),
        };
        for index in mark.start..mark.end {
            result.insert((index, mark.name().to_owned()), value.clone());
        }
    }
    result
}

fn source_signature(marks: &Value) -> BTreeMap<(usize, String), Value> {
    let mut result = BTreeMap::new();
    for mark in marks.as_array().unwrap() {
        for index in mark["start"].as_u64().unwrap()..mark["end"].as_u64().unwrap() {
            result.insert(
                (index as usize, mark["name"].as_str().unwrap().to_owned()),
                mark["value"].clone(),
            );
        }
    }
    result
}

fn canonical_vector(vector: &Value) -> CanvasCollabSnapshot {
    let bytes = crate::slack::api::decode_canvas_base64(vector["wire"].as_str().unwrap()).unwrap();
    let raw = RawMessage::decode(&bytes)
        .unwrap()
        .message(1)
        .unwrap()
        .unwrap();
    let outgoing = Section::from_raw(raw).unwrap();
    let wire = outgoing
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    let mut snapshot = fixture();
    let original = Section::from_raw(
        snapshot.sections[0]
            .raw()
            .replace_fields(1, &[t(1, outgoing.id())])
            .unwrap()
            .replace_fields(2, &[n(2, outgoing.sequence().unwrap().unwrap())])
            .unwrap()
            .replace_fields(10, &[n(10, vector["style"].as_u64().unwrap())])
            .unwrap(),
    )
    .unwrap();
    // Add only canonical identities/style and synthetic expiry to the actual
    // outgoing original-byte section. Automerge history bytes remain unchanged.
    snapshot.sections[0] = Section::from_raw(
        original
            .merge_partial(&outgoing)
            .unwrap()
            .raw()
            .replace_fields(38, &[m(38, &wire)])
            .unwrap(),
    )
    .unwrap();
    snapshot
}

fn synthetic_ack(prepared: &PreparedTextEdit) -> AcceptedText {
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    let seq = prepared.base_sequence() + 1000;
    let wire = update
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, seq)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    let raw = prepared
        .canonical
        .merge_partial(&update)
        .unwrap()
        .raw()
        .replace_fields(2, &[n(2, seq)])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, &raw)]))]).raw()).unwrap();
    prepared.validate_ack(&ack).unwrap()
}

#[test]
fn original_visitor_vectors_match_escaped_utf16_and_all_companion_marks() {
    for input in vectors()["inputs"].as_array().unwrap() {
        let draft = TextDraft::from_rtml(input["rtml"].as_str().unwrap()).unwrap();
        let model = draft.model().unwrap();
        assert_eq!(model.text, input["text"].as_str().unwrap());
        assert_eq!(signature(&model.marks), source_signature(&input["marks"]));
        assert!(TextDraft::from_rtml(&draft.rtml().unwrap()).unwrap() == draft);
        assert!(TextDraft::from_rich_blocks(&draft.rich_blocks()).unwrap() == draft);
    }
    let draft = TextDraft::from_rtml("&amp;&lt;<br>\u{1f642}<u>\u{4e2d}\u{1f98a}</u>").unwrap();
    let model = draft.model().unwrap();
    assert_eq!(model.marks.len(), 1);
    assert_eq!((model.marks[0].start, model.marks[0].end), (15, 18));
    assert_eq!(model.marks[0].name(), "u");
}

#[test]
fn original_wire_histories_admit_underlined_paragraph_and_headings_without_style_projection() {
    for vector in vectors()["vectors"].as_array().unwrap() {
        let snapshot = canonical_vector(vector);
        let section = &snapshot.sections[0];
        let draft = canonical_draft(section).unwrap();
        let wire = section.crdt().unwrap().unwrap();
        let doc = crdt::validate(&wire, section, &draft, Some(100)).unwrap();
        assert_eq!(
            draft.model().unwrap().text,
            vector["escapedText"].as_str().unwrap()
        );
        let object = doc.get(automerge::ROOT, "text").unwrap().unwrap().1;
        assert_eq!(
            signature(&doc.marks(&object).unwrap()),
            source_signature(&vector["marks"])
        );
        for head in vector["originalHeads"].as_array().unwrap() {
            assert!(
                doc.get_change_by_hash(&head.as_str().unwrap().parse().unwrap())
                    .is_some()
            );
        }
        let intent = TextIntent::capture(&snapshot, section.id(), 100).unwrap();
        let level = vector["style"].as_u64().unwrap() as u8;
        assert_eq!(intent.heading_level(), (level != 0).then_some(level));
        let desired =
            TextDraft::from_rtml(&format!("N{}", vector["editedRtml"].as_str().unwrap())).unwrap();
        let prepared = intent
            .prepare(&snapshot, &mut EditSession::new(), &desired, 100)
            .unwrap();
        let update = prepared.document_data().message(1).unwrap().unwrap();
        for tag in [3, 8, 10, 11, 16, 29] {
            assert!(!update.has(tag));
        }
        let accepted = synthetic_ack(&prepared);
        let mut fresh = snapshot.clone();
        fresh.sections[0] = accepted.baseline.clone();
        assert!(accepted.confirms_readback(&fresh).unwrap());
        assert_eq!(fresh.sections[0].style().unwrap(), Some(u64::from(level)));
        assert!(canonical_draft(&fresh.sections[0]).unwrap() == desired);
    }
}

#[test]
fn underline_removal_retains_false_history_and_companion_styles_in_all_source_cohorts() {
    for vector in vectors()["vectors"].as_array().unwrap() {
        let snapshot = canonical_vector(vector);
        let section = &snapshot.sections[0];
        let original = canonical_draft(section).unwrap();
        let mut desired = original.clone();
        for run in &mut desired.runs {
            run.style.underline = false;
        }
        let prepared = TextIntent::capture(&snapshot, section.id(), 100)
            .unwrap()
            .prepare(&snapshot, &mut EditSession::new(), &desired, 100)
            .unwrap();
        let wire = prepared
            .document_data()
            .message(1)
            .unwrap()
            .unwrap()
            .message(38)
            .unwrap()
            .unwrap();
        let doc = automerge::Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        crdt::check_document_state(&doc, &desired, false).unwrap();
        let object = doc.get(automerge::ROOT, "text").unwrap().unwrap().1;
        let marks = doc.marks(&object).unwrap();
        assert!(
            marks
                .iter()
                .any(|mark| mark.name() == "u" && mark.value() == &ScalarValue::Boolean(false))
        );
        assert!(
            !marks
                .iter()
                .any(|mark| mark.name() == "u" && mark.value() != &ScalarValue::Boolean(false))
        );
        let companions = marks
            .into_iter()
            .filter(|mark| mark.name() != "u")
            .collect::<Vec<_>>();
        assert_eq!(
            signature(&companions),
            signature(&desired.model().unwrap().marks)
        );
        assert_eq!(
            original.model().unwrap().text,
            desired.model().unwrap().text
        );
        assert!(!desired.rtml().unwrap().contains("<u>"));
        let accepted = synthetic_ack(&prepared);
        let mut fresh = snapshot.clone();
        fresh.sections[0] = accepted.baseline.clone();
        assert!(accepted.confirms_readback(&fresh).unwrap());
    }
}

#[test]
fn underline_rich_boolean_false_is_explicit_removal_but_malformed_styles_refuse() {
    let original = TextDraft::from_rtml("<u><b>Linked</b></u>").unwrap();
    let mut blocks = original.rich_blocks();
    assert_eq!(
        blocks[0]["elements"][0]["elements"][0]["style"]["underline"],
        true
    );
    blocks[0]["elements"][0]["elements"][0]["style"]["underline"] = json!(false);
    let plain = TextDraft::from_rich_blocks(&blocks).unwrap();
    assert!(plain.runs[0].style.bold && !plain.runs[0].style.underline);
    for value in [json!(null), json!(1), json!("true"), json!({})] {
        blocks[0]["elements"][0]["elements"][0]["style"]["underline"] = value;
        assert!(TextDraft::from_rich_blocks(&blocks).is_err());
    }
    for source in [
        "<u class='x'>text</u>",
        "<u style='color:red'>text</u>",
        "<u><u>twice</u></u>",
        "<u>unclosed",
        "<u><control>x</control></u>",
        "<u><span>x</span></u>",
    ] {
        assert!(TextDraft::from_rtml(source).is_err(), "{source}");
    }
}

#[test]
fn underline_annotation_admission_does_not_open_structural_actions() {
    use crate::canvas::edit::{annotation, structure};
    let mut snapshot = structure::tests::fixture();
    snapshot.file.extra.insert("access".into(), json!("write"));
    snapshot.thread = snapshot.thread.replace_fields(23, &[n(23, 0)]).unwrap();
    assert!(annotation::AnnotationIntent::capture(&snapshot, "item").is_ok());
    assert!(structure::paragraph_actions(&snapshot, &["item"], 100)[0].is_some());
    let underlined = TextDraft::from_rtml("<u>Preserved <b>body</b></u>").unwrap();
    assert!(matches!(
        structure::StructureIntent::insert(
            &snapshot,
            "item",
            structure::Side::After,
            &underlined,
            100
        ),
        Err(Error::Unsupported)
    ));
    snapshot.sections[0] = Section::from_raw(
        snapshot.sections[0]
            .raw()
            .replace_fields(
                12,
                &[m(12, &content(&snapshot.sections[0], &underlined).unwrap())],
            )
            .unwrap(),
    )
    .unwrap();
    assert!(TextIntent::capture(&snapshot, "item", 100).is_ok());
    assert!(structure::paragraph_actions(&snapshot, &["item"], 100)[0].is_none());
    assert!(annotation::AnnotationIntent::capture(&snapshot, "item").is_ok());
    assert!(
        annotation::paragraph_seeds(&snapshot)
            .unwrap()
            .iter()
            .any(|(id, draft)| id == "item" && draft == &underlined)
    );
}

#[test]
fn malformed_underlining_history_cannot_be_flattened_to_a_valid_draft() {
    use automerge::{
        marks::{ExpandMark, Mark},
        transaction::Transactable,
    };
    let all = vectors();
    let snapshot = canonical_vector(&all["vectors"][0]);
    let section = &snapshot.sections[0];
    let draft = canonical_draft(section).unwrap();
    let wire = section.crdt().unwrap().unwrap();
    for (name, value) in [
        ("u", ScalarValue::Int(1)),
        ("u", ScalarValue::Str("true".into())),
        ("unknown", ScalarValue::Boolean(false)),
    ] {
        let mut doc = automerge::Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        let object = doc.get(automerge::ROOT, "text").unwrap().unwrap().1;
        let mut tx = doc.transaction();
        tx.mark(
            &object,
            Mark::new(name.into(), value, 0, 1),
            ExpandMark::After,
        )
        .unwrap();
        tx.commit();
        assert!(crdt::check_document_state(&doc, &draft, false).is_err());
    }
}
