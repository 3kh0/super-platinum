//! Original updateText_ completes before updateMarks_; empty phases add no change.
use super::*;
use automerge::{Automerge, ROOT, ReadDoc};

fn processed(wire: &RawMessage) -> Vec<String> {
    wire.fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            RawMessage::decode(wire.encoded_field(f).unwrap())
                .unwrap()
                .string(6)
                .unwrap()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn text_then_marks_preserves_report_authority_without_empty_phase_changes() {
    let snapshot = fixture();
    let section = &snapshot.sections[0];
    for (before, after, count) in [
        ("", "<b>Restored owner</b>", 2),
        (
            "",
            "<a href=\"https://example.test\">Restored \u{65e5}\u{672c}\u{1f600} owner</a>",
            2,
        ),
        ("plain", "plain added", 1),
        ("plain", "<b>plain</b>", 1),
        ("<b>plain</b>", "plain", 1),
        ("<b>kept</b> tail", "<b>kept</b> tail added", 1),
        (
            "<b>same \u{65e5}\u{672c}\u{1f600}</b>",
            "<b>same \u{65e5}\u{672c}\u{1f600}</b>",
            0,
        ),
    ] {
        let original = TextDraft::from_rtml(before).unwrap();
        let desired = TextDraft::from_rtml(after).unwrap();
        let seed = crdt::bootstrap(section, &original, "012345abcdef", 1_000_000).unwrap();
        let old = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
        let wire = crdt::update(
            &seed,
            section,
            &original,
            &desired,
            "abcdef012345",
            2_000_000,
        )
        .unwrap();
        let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        let suffix = doc.get_changes(&old.get_heads());
        assert_eq!(suffix.len(), count, "{before:?} -> {after:?}");
        for change in old.get_changes(&[]) {
            assert_eq!(
                doc.get_change_by_hash(&change.hash()).unwrap().raw_bytes(),
                change.raw_bytes()
            );
        }
        crdt::check_document_state(&doc, &desired, false).unwrap();
        let final_head = doc.get_heads()[0];
        let mut expected = processed(&seed);
        expected.push(final_head.to_string());
        assert_eq!(processed(&wire), expected);
        assert_eq!(
            wire.string(12).unwrap(),
            Some(final_head.to_string().as_str())
        );
        if count == 2 {
            let text_phase = doc.fork_at(&[suffix[0].hash()]).unwrap();
            let object = text_phase.get(ROOT, "text").unwrap().unwrap().1;
            assert_eq!(
                text_phase.text(&object).unwrap(),
                format!("{}\u{feff}", desired.model().unwrap().text)
            );
            assert!(text_phase.marks(&object).unwrap().is_empty());
            assert_eq!(suffix[1].deps(), &[suffix[0].hash()]);
            assert_eq!(suffix[1].seq(), suffix[0].seq() + 1);
            assert_eq!(suffix[1].actor_id(), suffix[0].actor_id());
            assert!(!processed(&wire).contains(&suffix[0].hash().to_string()));
        }
    }
}

#[test]
fn inherited_append_marks_are_a_separate_unprocessed_phase() {
    let snapshot = fixture();
    let section = &snapshot.sections[0];
    for text in [
        "Second paragraph",
        "Second \u{65e5}\u{672c}\u{1f600} &amp; paragraph",
    ] {
        let linked = format!("<a href=\"https://example.test/\">{text}</a>");
        let original = TextDraft::from_rtml(&linked).unwrap();
        for retain_link in [false, true] {
            let rtml = if retain_link {
                format!("<a href=\"https://example.test/\">{text} edited</a>")
            } else {
                format!("{text} edited")
            };
            let desired = TextDraft::from_rtml(&rtml).unwrap();
            let seed = crdt::bootstrap(section, &original, "012345abcdef", 1_000_000).unwrap();
            let old = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
            let wire = crdt::update(
                &seed,
                section,
                &original,
                &desired,
                "abcdef012345",
                2_000_000,
            )
            .unwrap();
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            let changes = doc.get_changes(&old.get_heads());
            assert_eq!(changes.len(), if retain_link { 2 } else { 3 });
            for c in old.get_changes(&[]) {
                assert_eq!(
                    doc.get_change_by_hash(&c.hash()).unwrap().raw_bytes(),
                    c.raw_bytes()
                );
            }
            let ops = changes[1].decode().operations;
            assert_eq!(ops.len(), 2);
            use automerge::legacy::OpType;
            assert!(
                matches!(&ops[0].action, OpType::MarkBegin(m) if m.name == "a" && m.value == original.model().unwrap().marks[0].value && !m.expand)
            );
            assert!(matches!(&ops[1].action, OpType::MarkEnd(true)));
            let mut intermediate = old.clone();
            intermediate.apply_changes([changes[0].clone()]).unwrap();
            let object = intermediate.get(ROOT, "text").unwrap().unwrap().1;
            let inherited = intermediate.marks(&object).unwrap();
            intermediate.apply_changes([changes[1].clone()]).unwrap();
            assert_eq!(
                intermediate.marks(&object).unwrap(),
                inherited,
                "explicit original mark keeps the inherited projection"
            );
            for c in &changes[..changes.len() - 1] {
                assert!(!processed(&wire).contains(&c.hash().to_string()));
            }
            let mut expected = processed(&seed);
            expected.push(doc.get_heads()[0].to_string());
            assert_eq!(processed(&wire), expected);
            crdt::check_document_state(&doc, &desired, false).unwrap();
            if !retain_link {
                let removed = changes[2].decode().operations;
                assert!(
                    matches!(&removed[0].action, OpType::MarkBegin(m) if m.name == "a" && m.value == automerge::ScalarValue::Boolean(false))
                );
            }
        }
    }
}

#[test]
fn append_does_not_replay_inactive_false_mark_tombstones() {
    let snapshot = fixture();
    let section = &snapshot.sections[0];
    let linked = TextDraft::from_rtml("<a href=\"https://example.test/\">plain</a>").unwrap();
    let plain = TextDraft::from_rtml("plain").unwrap();
    let desired = TextDraft::from_rtml("plain appended").unwrap();
    let seed = crdt::bootstrap(section, &linked, "012345abcdef", 1_000_000).unwrap();
    let removed = crdt::update(&seed, section, &linked, &plain, "abcdef012345", 2_000_000).unwrap();
    let old = Automerge::load(removed.bytes(5).unwrap().unwrap()).unwrap();
    let object = old.get(ROOT, "text").unwrap().unwrap().1;
    assert!(
        old.marks(&object)
            .unwrap()
            .iter()
            .any(|m| m.value() == &automerge::ScalarValue::Boolean(false))
    );
    let wire = crdt::update(
        &removed,
        section,
        &plain,
        &desired,
        "abcdef012345",
        3_000_000,
    )
    .unwrap();
    let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    assert_eq!(doc.get_changes(&old.get_heads()).len(), 1);
    crdt::check_document_state(&doc, &desired, false).unwrap();
}
