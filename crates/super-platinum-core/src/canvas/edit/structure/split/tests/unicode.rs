use super::*;
use crate::canvas::edit::{
    EditSession,
    structure::recovery::{
        Checkpoint,
        journal::{Begin, Journal},
    },
    text::{TextIntent, TextRun, TextStyle},
};

const LEFT: &str = "Café & 日本😀";
const RIGHT: &str = "Second paragraph";

fn draft(value: &str) -> TextDraft {
    TextDraft {
        runs: vec![TextRun {
            text: value.into(),
            style: TextStyle::default(),
        }],
    }
}
fn setup(left: &str, right: &str, retained: bool) -> (CanvasCollabSnapshot, PreparedSplitEdit) {
    let mut s = fixture();
    let i = s.sections.iter().position(|s| s.id() == "item").unwrap();
    let before = draft(&format!("{left}{right}"));
    s.sections[i] = change(&s.sections[i], m(12, &body(&before.rtml().unwrap(), true)));
    if retained {
        let seed = text::crdt::bootstrap(&s.sections[i], &before, "0123456789ab", 100)
            .unwrap()
            .replace_fields(4, &[n(4, 1_000_000)])
            .unwrap();
        s.sections[i] = change(&s.sections[i], m(38, &seed));
    }
    let witness = SplitWitness::new(
        &before,
        left.encode_utf16().count(),
        &draft(left),
        &draft(right),
    )
    .unwrap();
    let prepared = SplitIntent::capture(&s, "item", 100)
        .unwrap()
        .prepare(&s, &witness, 100)
        .unwrap();
    (s, prepared)
}

#[test]
fn unicode_witness_uses_utf16_scalar_boundaries_without_normalization() {
    for (left, right) in [
        (LEFT, RIGHT),
        ("日本", "語"),
        ("a", "😀b"),
        ("a😀", "b"),
        ("e\u{301}", "déjà"),
        ("👩🏽\u{200d}💻", " next"),
        ("✈\u{fe0f}", " later"),
        ("日😀&<>", " tail"),
    ] {
        let before = draft(&format!("{left}{right}"));
        let witness = SplitWitness::new(
            &before,
            left.encode_utf16().count(),
            &draft(left),
            &draft(right),
        )
        .unwrap();
        assert!(witness.before == before);
        assert_eq!(plain(&witness.parent).unwrap(), left);
        assert_eq!(plain(&witness.child).unwrap(), right);
    }
    let before = draft(&format!("{LEFT}{RIGHT}"));
    // Zero/end, half-surrogate, scalar-count and UTF-8-byte-count offsets all
    // fail against the exact requested pair. Out-of-range never truncates.
    for offset in [0, 10, LEFT.chars().count(), LEFT.len(), 27, usize::MAX] {
        assert!(SplitWitness::new(&before, offset, &draft(LEFT), &draft(RIGHT)).is_err());
    }
    assert!(SplitWitness::new(&draft("e\u{301}tail"), 2, &draft("é"), &draft("tail")).is_err());
    assert!(SplitWitness::new(&draft("a😀b"), 2, &draft("a"), &draft("😀b")).is_err());
}

#[test]
fn unicode_plain_admission_excludes_controls_separators_and_terminator() {
    for c in (0..=0x1f)
        .chain(0x7f..=0x9f)
        .chain([0x2028, 0x2029, 0xfeff])
        .map(|n| char::from_u32(n).unwrap())
    {
        let left = format!("é{c}");
        assert!(
            SplitWitness::new(
                &draft(&format!("{left}tail")),
                left.encode_utf16().count(),
                &draft(&left),
                &draft("tail")
            )
            .is_err()
        );
    }
    assert!(
        SplitWitness::new(
            &TextDraft::from_rtml("<b>é😀tail</b>").unwrap(),
            3,
            &draft("é😀"),
            &draft("tail")
        )
        .is_err()
    );
}

#[test]
fn unicode_pair_keeps_history_and_distinct_visible_and_serialized_offsets() {
    for (left, right) in [
        (LEFT, RIGHT),
        ("日😀&<>", "尾"),
        ("e\u{301} 👩🏽\u{200d}💻", "✈\u{fe0f} tail"),
    ] {
        for retained in [false, true] {
            let (s, p) = setup(left, right, retained);
            let serialized = draft(left).rtml().unwrap();
            assert_eq!(p.witness.offset, left.encode_utf16().count());
            assert_eq!(
                p.lineage.uint(4).unwrap(),
                Some(serialized.encode_utf16().count() as u64)
            );
            if left == LEFT {
                assert_eq!(p.witness.offset, 11);
                assert_eq!(p.lineage.uint(4).unwrap(), Some(15));
            }
            let before = Automerge::load(p.lineage.bytes(7).unwrap().unwrap()).unwrap();
            text::crdt::check_document_state(&before, &p.witness.before, false).unwrap();
            let after = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
            text::crdt::check_document_state(&after, &draft(left), false).unwrap();
            assert!(
                before
                    .get_changes(&[])
                    .iter()
                    .all(|c| after.get_change_by_hash(&c.hash()).is_some())
            );
            p.checkpoint().unwrap(); // Strict independent outgoing pair/history proof.
            let rows = pair(&p, &s);
            assert_eq!(
                text::canonical_draft(&rows[0]).unwrap().rtml().unwrap(),
                serialized
            );
            assert_eq!(
                plain(&text::canonical_draft(&rows[1]).unwrap()).unwrap(),
                right
            );
            let accepted = p.validate_ack(&ack(&rows)).unwrap();
            let fresh = readback(&s, &rows);
            assert!(accepted.confirms_readback(&fresh).unwrap());
            assert!(TextIntent::capture(&fresh, "item", 100).is_ok());
            assert!(TextIntent::capture(&fresh, p.child_section(), 100).is_ok());
            assert!(p.validate_ack(&ack(&rows[..1])).is_err());
            let mut missing = fresh.clone();
            missing.sections.retain(|r| r.id() != p.child_section());
            assert!(!accepted.confirms_readback(&missing).unwrap());
        }
    }
}

#[test]
fn unicode_lineage_refuses_entity_surrogate_and_wrong_unit_offsets() {
    let (s, p) = setup(LEFT, RIGHT, true);
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let wire = rows[0].crdt().unwrap().unwrap();
    // Serialized RTML: Café &amp; 日本😀. 7 cuts an entity; 14 cuts
    // the astral pair; 11 is the visible offset and 18 is UTF-8 bytes.
    for offset in [0, 7, 14, 11, LEFT.len() as u64, u64::MAX] {
        let altered = p.lineage.replace_fields(4, &[n(4, offset)]).unwrap();
        let bad_wire = wire.replace_fields(10, &[m(10, &altered)]).unwrap();
        let mut bad = rows.clone();
        bad[0] = change(&bad[0], m(38, &bad_wire));
        assert!(p.validate_ack(&ack(&bad)).is_err(), "offset {offset}");
        let fresh = readback(&s, &bad);
        assert!(!accepted.confirms_readback(&fresh).unwrap_or(false));
        assert!(
            TextIntent::capture(&fresh, "item", 100).is_err(),
            "offset {offset}"
        );
    }
}

#[test]
fn unicode_split_parent_can_save_again_with_exact_lineage_and_child() {
    let (s, p) = setup(LEFT, RIGHT, true);
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let mut fresh = readback(&s, &rows);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    let child = fresh.section(p.child_section()).unwrap().clone();
    let mut session = EditSession::new();
    for value in ["Café &amp; 日本😀 updated", "e\u{301} <b>👩🏽\u{200d}💻</b>"] {
        let edit = continuation::edit(&fresh, &mut session, value);
        let row = continuation::received(&edit, &fresh, value);
        assert_eq!(
            row.crdt().unwrap().unwrap().message(10).unwrap().unwrap(),
            p.lineage
        );
        let accepted = edit.validate_ack(&ack(&[row.clone()])).unwrap();
        fresh = readback(&fresh, &[row]);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        session.remember_text_accepted(&accepted).unwrap();
        assert_eq!(fresh.section(p.child_section()), Some(&child));
    }
    let actions = super::super::super::paragraph_actions(&fresh, &["item"], 100);
    assert!(!actions[0].as_ref().unwrap().delete);
    let intent = TextIntent::capture(&fresh, "item", 100).unwrap();
    let i = fresh
        .sections
        .iter()
        .position(|r| r.id() == p.child_section())
        .unwrap();
    fresh.sections[i] = change(&fresh.sections[i], n(2, 44000));
    assert!(
        intent
            .prepare(&fresh, &mut session, &draft("later"), 100)
            .is_err()
    );
}

#[test]
fn unicode_v3_durable_roundtrip_and_restored_review_preserve_both_halves() {
    const ACCOUNT: &str = "00000000-0000-4000-8000-000000000785";
    let path = std::env::temp_dir().join(format!("canvas-unicode-{}.sqlite", uuid::Uuid::new_v4()));
    let (s, p) = setup(LEFT, RIGHT, true);
    let checkpoint = p.checkpoint().unwrap();
    let stored = serde_json::to_value(&checkpoint).unwrap();
    assert_eq!(stored["version"], 3);
    assert_eq!(stored["action"]["Split"]["offset_utf16"], 11);
    let decoded: Checkpoint = serde_json::from_value(stored.clone()).unwrap();
    assert!(decoded == checkpoint);
    {
        let mut journal = Journal::open(&path, ACCOUNT).unwrap();
        for offset in [10, 15, LEFT.len()] {
            let mut bad = stored.clone();
            bad["action"]["Split"]["offset_utf16"] = offset.into();
            assert!(journal.begin(serde_json::from_value(bad).unwrap()).is_err());
            assert!(journal.get(checkpoint.scope()).unwrap().is_none());
        }
        assert!(matches!(journal.begin(decoded).unwrap(), Begin::Started(_)));
    }
    {
        let mut journal = Journal::open(&path, ACCOUNT).unwrap();
        let restored = journal.get(checkpoint.scope()).unwrap().unwrap();
        assert!(restored.checkpoint() == &checkpoint);
        let review = journal.local_review(&restored, &s).unwrap();
        let split = review.split().unwrap();
        assert_eq!(split.original_text(), format!("{LEFT}{RIGHT}"));
        assert_eq!(split.parent_text(), LEFT);
        assert_eq!(split.child_text(), RIGHT);
        assert_eq!(split.child_id(), p.child_section());
        assert_eq!(journal.local_review_count(checkpoint.scope()).unwrap(), 0);
        assert!(matches!(
            journal.begin(checkpoint.clone()).unwrap(),
            Begin::Blocked(_)
        ));
        assert!(journal.get(checkpoint.scope()).unwrap().as_ref() == Some(&restored));
    }
    std::fs::remove_file(path).unwrap();
}
