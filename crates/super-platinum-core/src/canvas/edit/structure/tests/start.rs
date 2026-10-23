//! Enter at start uses ordinary empty insert-before, not split lineage.
use super::*;
use crate::canvas::edit::{EditSession, text::TextIntent};

mod recovery;

fn new_content(draft: &TextDraft) -> Result<RawMessage, Error> {
    // Fixture wire: a Content.text message with explicit RTML, no language.
    Ok(msg(&[m(1, &msg(&[t(1, &draft.rtml()?)]))]))
}

fn sources() -> Vec<CanvasCollabSnapshot> {
    // Independently exercise real retained canonical histories; the original
    // start-branch probe deliberately never accesses its anchor CRDT.
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../split/tests/typing/original.json")).unwrap();
    let mut result = Vec::new();
    for unicode in [false, true] {
        let v = vectors["vectors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| {
                v["sourceSession"].is_string()
                    && v["full"].as_str().unwrap().contains("Caf") == unicode
            })
            .unwrap();
        for retained in [false, true] {
            let mut s = fixture();
            let row = s.sections.iter_mut().find(|r| r.id() == "item").unwrap();
            let draft = TextDraft::from_rtml(v["full"].as_str().unwrap()).unwrap();
            *row = changed(
                &changed(row, n(2, 1000)),
                m(12, &new_content(&draft).unwrap()),
            );
            if retained {
                let bytes =
                    crate::slack::api::decode_canvas_base64(v["sourceSession"].as_str().unwrap())
                        .unwrap();
                let wire = RawMessage::decode(&bytes)
                    .unwrap()
                    .replace_fields(1, &[t(1, "item")])
                    .unwrap()
                    .replace_fields(4, &[n(4, 1_000_000)])
                    .unwrap();
                *row = changed(row, m(38, &wire));
            }
            result.push(s);
        }
    }
    result
}
fn start(s: &CanvasCollabSnapshot) -> PreparedStructureEdit {
    StructureIntent::insert(s, "item", Side::Before, &TextDraft::default(), 100)
        .unwrap()
        .prepare(s, 100)
        .unwrap()
}
fn update_text(s: &CanvasCollabSnapshot, id: &str, value: &str) -> CanvasCollabSnapshot {
    let draft = TextDraft::from_rtml(value).unwrap();
    let p = TextIntent::capture(s, id, 100)
        .unwrap()
        .prepare(s, &mut EditSession::new(), &draft, 100)
        .unwrap();
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let original = s.section(id).unwrap();
    let sequence = original.sequence().unwrap().unwrap() + 1000;
    let wire = update
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    let row = changed(
        &changed(&original.merge_partial(&update).unwrap(), n(2, sequence)),
        m(38, &wire),
    );
    let accepted = p.validate_ack(&ack(&row)).unwrap();
    let fresh = readback(s, &row);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    for old in s.sections.iter().filter(|r| r.id() != id) {
        assert_eq!(fresh.section(old.id()), Some(old));
    }
    if let Some(before) = s.section(id).unwrap().crdt().unwrap() {
        let old = automerge::Automerge::load(before.bytes(5).unwrap().unwrap()).unwrap();
        let after = row.crdt().unwrap().unwrap();
        let new = automerge::Automerge::load(after.bytes(5).unwrap().unwrap()).unwrap();
        for c in old.get_changes(&[]) {
            assert!(new.get_change_by_hash(&c.hash()).is_some());
        }
    }
    fresh
}

#[test]
fn start_empty_before_matches_original_single_bootstrap_and_preserves_all_anchor_bytes() {
    for s in sources() {
        let p = start(&s);
        let data = DocumentData::decode(p.document_data().raw()).unwrap();
        assert_eq!(data.sections().len(), 1);
        let row = &data.sections()[0];
        assert_ne!(row.id(), "item");
        assert_eq!(row.raw().string(8).unwrap(), Some("aab:temp"));
        assert_eq!(
            row.raw()
                .fields()
                .iter()
                .map(|f| f.tag())
                .collect::<Vec<_>>(),
            [
                1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38
            ]
        );
        assert_eq!(text::canonical_draft(row).unwrap().rtml().unwrap(), "");
        let wire = row.crdt().unwrap().unwrap();
        assert_eq!(wire.uint(2).unwrap(), Some(0));
        assert_eq!(wire.uint(3).unwrap(), Some(0));
        assert!(!wire.has(10));
        assert_eq!(wire.string(6).unwrap(), wire.string(8).unwrap());
        assert_eq!(wire.string(8).unwrap(), wire.string(12).unwrap());
        let doc = automerge::Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        assert_eq!(doc.get_changes(&[]).len(), 1);
        text::crdt::check_document_state(&doc, &TextDraft::default(), false).unwrap();
        let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
        assert_eq!(doc.text(&object).unwrap(), "\u{feff}");
        let checkpoint = serde_json::to_value(p.checkpoint().unwrap()).unwrap();
        assert_eq!(checkpoint["version"], 1);
        assert_eq!(checkpoint["action"]["Insert"]["before"], true);
        assert_eq!(checkpoint["action"]["Insert"]["anchor"], "item");
        let canonical = canonical_ack(&p, &s);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        let fresh = readback(&s, &canonical);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        assert_eq!(fresh.sections.len(), s.sections.len() + 1);
        for old in &s.sections {
            assert_eq!(fresh.section(old.id()), Some(old));
        }
        assert_eq!(canonical.raw().string(8).unwrap(), Some("aab"));
        // Both independent rows remain ordinary editable paragraphs after ACK
        // and a separate canonical load, without creating split associations.
        let original_edit = update_text(&fresh, "item", "Original edited &amp; 日本😀");
        let blank_edit = update_text(&original_edit, canonical.id(), "New first paragraph");
        assert!(
            !blank_edit
                .section("item")
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .has(10)
        );
        assert!(
            !blank_edit
                .section(canonical.id())
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .has(10)
        );
    }
}

#[test]
fn start_insert_never_reuses_an_existing_empty_predecessor() {
    let mut s = sources().remove(0);
    let prior = changed(
        &changed(
            &changed(s.section("item").unwrap(), t(1, "prior")),
            t(8, "aab"),
        ),
        m(12, &new_content(&TextDraft::default()).unwrap()),
    );
    s.sections.push(prior.clone());
    let p = start(&s);
    assert_ne!(p.target_section(), prior.id());
    let canonical = canonical_ack(&p, &s);
    assert!(canonical.raw().string(8).unwrap().unwrap() > "aab");
    assert!(canonical.raw().string(8).unwrap().unwrap() < "aac");
    let accepted = p.validate_ack(&ack(&canonical)).unwrap();
    let fresh = readback(&s, &canonical);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    assert_eq!(fresh.section("prior"), Some(&prior));
}

#[test]
fn start_requires_one_owned_exact_ack_and_independent_untouched_readback() {
    let s = sources().remove(1);
    for field in [
        t(1, "wrong"),
        t(7, "wrong-document"),
        t(8, "aad"),
        n(20, 2),
        m(
            12,
            &new_content(&TextDraft::from_rtml("not empty").unwrap()).unwrap(),
        ),
        n(1002, 1),
    ] {
        let p = start(&s);
        let canonical = changed(&canonical_ack(&p, &s), field);
        assert!(p.validate_ack(&ack(&canonical)).is_err());
    }
    for count in [0, 2] {
        let p = start(&s);
        let canonical = canonical_ack(&p, &s);
        let rows = if count == 0 {
            vec![]
        } else {
            vec![
                m(1, canonical.raw()),
                m(1, s.section("item").unwrap().raw()),
            ]
        };
        let a = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&rows))]).raw()).unwrap();
        assert!(p.validate_ack(&a).is_err());
    }
    let p = start(&s);
    let canonical = canonical_ack(&p, &s);
    let accepted = p.validate_ack(&ack(&canonical)).unwrap();
    assert!(!accepted.confirms_readback(&s).unwrap());
    let fresh = readback(&s, &canonical);
    for field in [n(2, 2000), t(8, "aad"), n(1002, 1)] {
        let drift = readback(&fresh, &changed(s.section("item").unwrap(), field));
        assert!(!accepted.confirms_readback(&drift).unwrap());
    }
    let missing_history = Section::from_raw(canonical.raw().remove(38).unwrap()).unwrap();
    assert!(
        !accepted
            .confirms_readback(&readback(&fresh, &missing_history))
            .unwrap()
    );
    assert!(accepted.confirms_readback(&fresh).unwrap());
}

#[test]
fn start_preflight_binds_inventory_access_positions_title_and_retained_lease() {
    let s = sources().remove(1);
    for field in [t(8, "aad"), n(2, 2000), n(1002, 1), n(35, 1)] {
        let intent =
            StructureIntent::insert(&s, "item", Side::Before, &TextDraft::default(), 100).unwrap();
        let drift = readback(&s, &changed(s.section("item").unwrap(), field));
        assert!(intent.prepare(&drift, 100).is_err());
    }
    assert!(
        StructureIntent::insert(&s, "item", Side::Before, &TextDraft::default(), 1_000_000)
            .is_err()
    );
    for field in [
        t(8, "aaa"),
        t(8, "aac:temp"),
        n(35, 1),
        t(7, "wrong-document"),
    ] {
        let drift = readback(&s, &changed(s.section("item").unwrap(), field));
        assert!(
            StructureIntent::insert(&drift, "item", Side::Before, &TextDraft::default(), 100)
                .is_err()
        );
    }
    let p = start(&s);
    let mut missing = s.clone();
    missing.sections.retain(|r| r.id() != "title");
    assert!(p.validate_snapshot(&missing, 100).is_err());
    let p = start(&s);
    let mut drift = s.clone();
    drift.document = drift.document.replace_fields(1001, &[n(1001, 1)]).unwrap();
    assert!(p.validate_snapshot(&drift, 100).is_err());
}
