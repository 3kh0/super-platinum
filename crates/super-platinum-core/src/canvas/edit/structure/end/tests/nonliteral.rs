//! Enter clears only admitted canonical nonliteral empty sources, before typing.
use super::*;
mod recovery;

pub(super) fn source(shape: &str, zero: bool) -> (CanvasCollabSnapshot, String) {
    let (s, id) = if zero {
        zero_reuse::source()
    } else {
        (reuse::reused_fixture(true), "other".to_owned())
    };
    (save_text(&s, &id, shape), id)
}
pub(super) fn attempt(s: &CanvasCollabSnapshot, updates: &[&str]) -> PreparedEndEdit {
    let intent = EndIntent::capture(s, "item", 100).unwrap();
    assert!(intent.reuses_child() && intent.clears_child());
    let witness = witness(&intent.seed().rtml().unwrap())
        .with_child_updates(&updates.iter().map(|v| draft(v)).collect::<Vec<_>>())
        .unwrap();
    let p = intent.prepare(s, &witness, 100).unwrap();
    assert!(p.clears_child());
    p
}
pub(super) fn replace(s: &CanvasCollabSnapshot, id: &str, field: F) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let row = s.sections.iter_mut().find(|r| r.id() == id).unwrap();
    *row = change(row, field);
    s
}
pub(super) fn processed(w: &RawMessage) -> Vec<String> {
    w.fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            RawMessage::decode(w.encoded_field(f).unwrap())
                .unwrap()
                .string(6)
                .unwrap()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn clearing_three_shapes_both_bases_preserve_exact_first_change_and_continuation() {
    for shape in ["   ", "\u{a0}", "<br>"] {
        for zero in [false, true] {
            for updates in [
                &[][..],
                &["x"][..],
                &["x", ""][..],
                &["New", "New draftx", "New draft", "New paragraph"][..],
            ] {
                let (s, id) = source(shape, zero);
                let original = s.section(&id).unwrap();
                let seed = original.crdt().unwrap().unwrap();
                let before = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
                let p = attempt(&s, updates);
                assert_eq!(p.child_section(), id);
                assert_eq!(p.child_updates().len(), updates.len());
                let rows = p.data.messages(1).unwrap();
                assert_eq!(rows.len(), 2);
                assert_eq!(
                    rows[0].fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
                    [1, 2, 13, 19, 20, 33, 38]
                );
                // Singular field ordering is not protobuf semantics. Sort
                // only this tag list, retaining multiplicity; processed heads
                // and history/report order are checked independently below.
                let mut tags = rows[1].fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
                tags.sort_unstable();
                assert_eq!(
                    tags,
                    [1, 2, 9, 12, 13, 19, 20, 33, 38],
                    "shape={shape:?},zero={zero},updates={updates:?}"
                );
                assert!(!rows[1].has(8) && !rows[1].has(21));
                assert_eq!(
                    p.child_draft().rtml().unwrap(),
                    updates.last().copied().unwrap_or("")
                );
                assert_eq!(p.child_wire.uint(2).unwrap(), seed.uint(2).unwrap());
                assert_eq!(p.child_wire.uint(3).unwrap(), seed.uint(3).unwrap());
                assert_eq!(p.child_wire.string(8).unwrap(), seed.string(8).unwrap());
                assert_eq!(
                    p.child_wire.messages(11).unwrap(),
                    seed.messages(11).unwrap()
                );
                assert!(!p.child_wire.has(4) && !p.child_wire.has(10));
                let after = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
                assert!(
                    before
                        .get_changes(&[])
                        .iter()
                        .all(|c| after.get_change_by_hash(&c.hash()).is_some())
                );
                let changes = after.get_changes(&before.get_heads());
                assert_eq!(
                    changes.len(),
                    1 + if updates.len() == 4 { 7 } else { updates.len() }
                );
                let first = &changes[0];
                assert_eq!(first.deps(), before.get_heads());
                let at = after.fork_at(&[first.hash()]).unwrap();
                text::crdt::check_document_state(&at, &TextDraft::default(), false).unwrap();
                let patches = after.diff(&before.get_heads(), &[first.hash()]);
                assert_eq!(patches.len(), 1);
                match &patches[0].action {
                    automerge::PatchAction::DeleteSeq { index, length } => {
                        assert_eq!(*index, 0);
                        assert_eq!(
                            *length,
                            if shape == "<br>" {
                                4
                            } else {
                                shape.encode_utf16().count()
                            }
                        );
                    }
                    other => panic!("clear must be exact serialized deletion: {other:?}"),
                }
                let mut expected = processed(&seed);
                expected.push(after.get_heads()[0].to_string());
                expected.extend(processed(&seed));
                assert_eq!(processed(&p.child_wire), expected);
                assert_eq!(
                    p.parent_wire,
                    validation::project_parent_session(
                        &s.section("item").unwrap().crdt().unwrap().unwrap()
                    )
                    .unwrap()
                );
                let canonical = pair(&p, &s);
                let accepted = p.validate_ack(&ack(&canonical)).unwrap();
                let fresh = readback(&s, &canonical);
                assert!(accepted.confirms_readback(&fresh).unwrap());
                assert_eq!(fresh.sections.len(), s.sections.len());
                assert_eq!(
                    fresh.section(&id).unwrap().raw().string(8).unwrap(),
                    original.raw().string(8).unwrap()
                );
                let continued = save_text(&fresh, &id, "Continued &amp; text");
                assert_eq!(continued.section("item"), fresh.section("item"));
                assert_eq!(
                    continued
                        .section(&id)
                        .unwrap()
                        .crdt()
                        .unwrap()
                        .unwrap()
                        .uint(2)
                        .unwrap(),
                    seed.uint(2).unwrap()
                );
                assert_eq!(
                    serde_json::to_value(p.checkpoint().unwrap()).unwrap()["version"],
                    7
                );
            }
        }
    }
}

#[test]
fn clearing_ack_requires_body_even_without_typing_and_fresh_whole_pair() {
    for shape in ["   ", "\u{a0}", "<br>"] {
        for zero in [false, true] {
            let (s, id) = source(shape, zero);
            let p = attempt(&s, &[]);
            let rows = pair(&p, &s);
            assert!(p.validate_ack(&ack(&rows[..1])).is_err());
            assert!(p.validate_ack(&ack(&rows[1..])).is_err());
            let missing = Section::from_raw(rows[1].raw().remove(12).unwrap()).unwrap();
            assert!(p.validate_ack(&ack(&[rows[0].clone(), missing])).is_err());
            let expected_body = p.data.messages(1).unwrap()[1].message(12).unwrap().unwrap();
            let expected_text = expected_body.message(1).unwrap().unwrap();
            let wrong_text = if expected_text.has(2) {
                expected_text.remove(2).unwrap()
            } else {
                expected_text.replace_fields(2, &[t(2, "en")]).unwrap()
            };
            let wrong_body = expected_body
                .replace_fields(1, &[m(1, &wrong_text)])
                .unwrap();
            assert_ne!(
                expected_text.bytes(2).unwrap(),
                wrong_text.bytes(2).unwrap()
            );
            for (case, field) in [
                ("old-source-body", m(12, &body(shape, true))),
                ("changed-language", m(12, &wrong_body)),
                (
                    "old-source-history",
                    m(38, &s.section(&id).unwrap().crdt().unwrap().unwrap()),
                ),
            ] {
                assert!(
                    p.validate_ack(&ack(&[rows[0].clone(), change(&rows[1], field)]))
                        .is_err(),
                    "shape={shape:?},zero={zero},case={case}"
                );
            }
            let accepted = p.validate_ack(&ack(&rows)).unwrap();
            let fresh = readback(&s, &rows);
            assert!(
                !accepted
                    .confirms_readback(&replace(&fresh, &id, m(12, &body(shape, true))))
                    .unwrap()
            );
            let wire = s.section(&id).unwrap().crdt().unwrap().unwrap();
            for changed in [
                replace(&s, &id, n(2, 9000)),
                replace(&s, &id, m(12, &body(" ", true))),
                replace(&s, &id, m(38, &wire.remove(4).unwrap())),
                replace(&s, "title", m(12, &body("Changed", false))),
            ] {
                assert!(p.validate_snapshot(&changed, 100).is_err());
            }
            assert!(p.validate_snapshot(&s, 1_000_000).is_err());
        }
    }
}

#[test]
fn clearing_refuses_unpinned_normalized_rich_and_ambiguous_sources() {
    let (s, id) = source("   ", false);
    for raw in ["\t", "\n", "\r", "&nbsp;", "<br><br>", "<br/>", "<b> </b>"] {
        let changed = replace(&s, &id, m(12, &body(raw, true)));
        assert!(
            EndIntent::capture(&changed, "item", 100).is_err(),
            "{raw:?}"
        );
    }
    for (case, field) in [
        ("read-only", n(35, 1)),
        ("nonordinary-style", n(10, 1)),
        ("default-content", m(16, &msg(&[n(13, 1)]))),
    ] {
        assert!(
            EndIntent::capture(&replace(&s, &id, field), "item", 100).is_err(),
            "case={case}"
        );
    }
    // Moving the child before the parent makes the parent final. Fresh capture
    // legitimately creates a new child; it must not clear the earlier section.
    let moved = replace(&s, &id, t(8, "aab"));
    let fresh = EndIntent::capture(&moved, "item", 100).unwrap();
    assert!(!fresh.reuses_child() && !fresh.clears_child());
    let pending = attempt(&s, &[]);
    assert!(pending.validate_snapshot(&moved, 100).is_err());
    // Keep the nonliteral child final but insert a real ordinary row between
    // it and the parent. This cannot be reinterpreted as adjacent reuse.
    let parent = s.section("item").unwrap();
    let row = Section::from_raw(parent.raw().remove(38).unwrap()).unwrap();
    let mut nonadjacent = s.clone();
    nonadjacent
        .sections
        .push(change(&change(&row, t(1, "between")), t(8, "aad")));
    assert!(
        EndIntent::capture(&nonadjacent, "item", 100).is_err(),
        "nonadjacent final child"
    );
    // An ordinary later row makes the source child nonfinal: fresh capture is
    // a new insertion and must not reuse or clear this existing candidate.
    let mut nonfinal = s.clone();
    nonfinal
        .sections
        .push(change(&change(&row, t(1, "later")), t(8, "aag")));
    let fresh = EndIntent::capture(&nonfinal, "item", 100).unwrap();
    assert!(!fresh.reuses_child() && !fresh.clears_child());
    // Neither legacy persisted action may reinterpret the new source cohort.
    let rows = s.sections.iter().collect::<Vec<_>>();
    let mut ordered = rows;
    ordered.sort_by_key(|r| r.raw().string(8).unwrap().unwrap());
    assert!(super::super::reuse::candidate(&ordered, "item").is_err());
}

#[test]
fn clearing_proof_refuses_skipped_boundary_even_with_identical_final_text() {
    for shape in ["   ", "\u{a0}", "<br>"] {
        for zero in [false, true] {
            let (s, id) = source(shape, zero);
            let p = attempt(&s, &["x"]);
            let child = s.section(&id).unwrap();
            let seed = validation::project_parent_session(&child.crdt().unwrap().unwrap()).unwrap();
            // A direct replacement can reach the same final text, but it did
            // not preserve the original report that clears the reused child.
            use automerge::transaction::Transactable;
            let mut doc = Automerge::load(seed.bytes(5).unwrap().unwrap())
                .unwrap()
                .with_actor(format!("{}00000000", p.session_id()).parse().unwrap());
            let (_, object) = doc.get(automerge::ROOT, "text").unwrap().unwrap();
            let mut tx = doc.transaction();
            tx.update_text(&object, "x\u{feff}").unwrap();
            tx.commit();
            let head = doc.get_heads()[0].to_string();
            let mut heads = processed(&seed);
            heads.push(head.clone());
            heads.extend(processed(&seed));
            let skipped = p
                .child_wire
                .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
                .unwrap()
                .replace_fields(12, &[t(12, &head)])
                .unwrap()
                .replace_fields(6, &heads.iter().map(|h| t(6, h)).collect::<Vec<_>>())
                .unwrap();
            let forged = super::super::typing::project(
                s.section("item").unwrap(),
                Some(child),
                &id,
                &p.intent.position,
                p.session_id(),
                &p.parent_wire,
                &skipped,
                p.intent.title.rich_text(),
                &draft("x"),
            )
            .unwrap();
            assert!(
                super::super::typing::validate_clearing(
                    s.section("item").unwrap(),
                    child,
                    &id,
                    &p.intent.position,
                    p.session_id(),
                    &p.witness,
                    &forged
                )
                .is_err()
            );
        }
    }
}
