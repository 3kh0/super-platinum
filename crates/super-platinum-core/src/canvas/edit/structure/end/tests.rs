use super::*;
use crate::canvas::edit::{EditSession, text::TextIntent};
use crate::canvas::wire::EditAck;
use automerge::{Automerge, ReadDoc};
mod empty;
mod nonliteral;
mod recovery;
mod repeat;
mod reuse;
mod sequence;
mod typing;
mod whitespace;
mod zero_reuse;

fn n(tag: u32, v: u64) -> F {
    F::uint(tag, v).unwrap()
}
fn t(tag: u32, v: &str) -> F {
    F::bytes(tag, v.as_bytes()).unwrap()
}
fn m(tag: u32, v: &RawMessage) -> F {
    F::message(tag, v).unwrap()
}
fn msg(v: &[F]) -> RawMessage {
    RawMessage::from_fields(v).unwrap()
}
fn change(s: &Section, f: F) -> Section {
    Section::from_raw(
        s.raw()
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn body(value: &str, language: bool) -> RawMessage {
    let mut v = vec![t(1, value)];
    if language {
        v.push(t(2, "en"));
    }
    msg(&[m(1, &msg(&v))])
}
fn draft(value: &str) -> TextDraft {
    TextDraft::from_rtml(value).unwrap()
}
fn fixture(value: &str, retained: bool) -> CanvasCollabSnapshot {
    let mut s = super::super::tests::fixture();
    let i = s.sections.iter().position(|r| r.id() == "item").unwrap();
    if retained {
        let original = draft("Earlier");
        s.sections[i] = change(&s.sections[i], m(12, &body("Earlier", true)));
        let seed = text::crdt::bootstrap(&s.sections[i], &original, "0123456789ab", 100).unwrap();
        let updated = text::crdt::update(
            &seed,
            &s.sections[i],
            &original,
            &draft(value),
            "abcdef012345",
            100,
        )
        .unwrap()
        .remove(12)
        .unwrap();
        let doc = Automerge::load(updated.bytes(5).unwrap().unwrap()).unwrap();
        let seq = s.sections[i].sequence().unwrap().unwrap();
        let wire = updated
            .replace_fields(4, &[n(4, 1_000_000)])
            .unwrap()
            .replace_fields(
                11,
                &[m(
                    11,
                    &msg(&[t(1, &doc.get_heads()[0].to_string()), n(2, seq)]),
                )],
            )
            .unwrap();
        s.sections[i] = change(&s.sections[i], m(38, &wire));
    }
    s.sections[i] = change(&s.sections[i], m(12, &body(value, true)));
    s
}
fn witness(value: &str) -> EndWitness {
    let d = draft(value);
    EndWitness::new(
        &d,
        split::plain(&d).unwrap().encode_utf16().count(),
        &d,
        &TextDraft::default(),
    )
    .unwrap()
}
fn prepare(s: &CanvasCollabSnapshot) -> PreparedEndEdit {
    let intent = EndIntent::capture(s, "item", 100).unwrap();
    let w = witness(&intent.seed().rtml().unwrap());
    intent.prepare(s, &w, 100).unwrap()
}
fn pair(p: &PreparedEndEdit, s: &CanvasCollabSnapshot) -> Vec<Section> {
    p.data
        .messages(1)
        .unwrap()
        .into_iter()
        .map(|raw| {
            let u = Section::from_raw(raw).unwrap();
            let original = s.section(u.id());
            let seq = original.and_then(|r| r.sequence().unwrap()).unwrap_or(0) + 1000;
            let mut row = original.map_or_else(|| u.clone(), |old| old.merge_partial(&u).unwrap());
            for f in [n(2, seq), t(6, s.thread_id()), t(7, s.document_id())] {
                row = change(&row, f);
            }
            if original.is_none() {
                let pos = position::base(row.raw().string(8).unwrap().unwrap())
                    .unwrap()
                    .to_owned();
                row = change(&change(&row, t(8, &pos)), t(21, &pos));
            }
            let wire = row
                .crdt()
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[n(3, seq)])
                .unwrap()
                .replace_fields(4, &[n(4, 1_000_000)])
                .unwrap();
            change(&row, m(38, &wire))
        })
        .collect()
}
fn ack(rows: &[Section]) -> EditAck {
    EditAck::decode(
        msg(&[
            n(1, 1),
            m(
                2,
                &msg(&rows.iter().map(|r| m(1, r.raw())).collect::<Vec<_>>()),
            ),
        ])
        .raw(),
    )
    .unwrap()
}
fn readback(s: &CanvasCollabSnapshot, rows: &[Section]) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    for row in rows {
        if let Some(i) = s.sections.iter().position(|r| r.id() == row.id()) {
            s.sections[i] = row.clone();
        } else {
            s.sections.push(row.clone());
        }
    }
    s
}

#[test]
fn end_witness_requires_exact_nonempty_plain_source_end_and_canonical_empty_child() {
    for value in ["Keep this", "Café &amp; 日本😀", "e\u{301} 👩🏽\u{200d}💻"] {
        let w = witness(value);
        assert!(w.before == draft(value));
    }
    let d = draft("Café &amp; 日本😀");
    for offset in [0, 10, 15, 18, usize::MAX] {
        assert!(EndWitness::new(&d, offset, &d, &TextDraft::default()).is_err());
    }
    assert!(EndWitness::new(&d, 11, &draft("changed"), &TextDraft::default()).is_err());
    assert!(EndWitness::new(&d, 11, &d, &draft("tail")).is_err());
    assert!(
        EndWitness::new(
            &TextDraft::default(),
            0,
            &TextDraft::default(),
            &TextDraft::default()
        )
        .is_err()
    );
    for value in ["<b>text</b>", "a<br>b", "a\u{feff}b", "a\u{2028}b"] {
        let d = draft(value);
        assert!(EndWitness::new(&d, 3, &d, &TextDraft::default()).is_err());
    }
}

#[test]
fn end_projection_keeps_parent_history_and_empty_child_bootstrap_exact() {
    for value in ["Keep this", "Café &amp; 日本😀"] {
        for retained in [false, true] {
            let s = fixture(value, retained);
            let before = s.sections.clone();
            let p = prepare(&s);
            let updates = p.data.messages(1).unwrap();
            assert_eq!(
                updates[0]
                    .fields()
                    .iter()
                    .map(|f| f.tag())
                    .collect::<Vec<_>>(),
                [1, 2, 13, 19, 20, 33, 38]
            );
            assert!(!p.parent_wire.has(4) && !p.parent_wire.has(10) && !p.parent_wire.has(12));
            if retained {
                assert_eq!(
                    p.parent_wire,
                    validation::project_parent_session(
                        &s.section("item").unwrap().crdt().unwrap().unwrap()
                    )
                    .unwrap()
                );
                assert!(p.parent_wire.has(11));
            }
            let parent = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
            text::crdt::check_document_state(&parent, &draft(value), false).unwrap();
            assert_eq!(parent.get_changes(&[]).len(), if retained { 2 } else { 1 });
            let child = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
            assert_eq!(child.get_changes(&[]).len(), 1);
            text::crdt::check_document_state(&child, &TextDraft::default(), false).unwrap();
            assert_eq!(
                p.child_wire.string(8).unwrap(),
                p.child_wire.string(12).unwrap()
            );
            assert_eq!(p.child_wire.uint(2).unwrap(), Some(0));
            assert_eq!(
                updates[1]
                    .message(12)
                    .unwrap()
                    .unwrap()
                    .message(1)
                    .unwrap()
                    .unwrap()
                    .bytes(1)
                    .unwrap(),
                Some(b"".as_slice())
            );
            p.checkpoint().unwrap();
            let rows = pair(&p, &s);
            let accepted = p.validate_ack(&ack(&rows)).unwrap();
            assert!(accepted.confirms_readback(&readback(&s, &rows)).unwrap());
            let mut metadata = rows.clone();
            metadata[0] = Section::from_raw(metadata[0].raw().remove(12).unwrap()).unwrap();
            let accepted = p.validate_ack(&ack(&metadata)).unwrap();
            assert!(accepted.confirms_readback(&readback(&s, &rows)).unwrap());
            assert!(
                !accepted
                    .confirms_readback(&readback(&s, &metadata))
                    .unwrap_or(false)
            );
            assert_eq!(before, s.sections);
        }
    }
}

#[test]
fn end_preflight_refuses_stale_inventory_title_permission_and_final_empty_reuse() {
    let s = fixture("Keep this", true);
    for id in ["item", "other", "title"] {
        let intent = EndIntent::capture(&s, "item", 100).unwrap();
        let mut fresh = s.clone();
        let i = fresh.sections.iter().position(|r| r.id() == id).unwrap();
        fresh.sections[i] = change(&fresh.sections[i], n(2, 20000));
        assert!(intent.prepare(&fresh, &witness("Keep this"), 100).is_err());
    }
    for (id, field) in [("other", m(12, &body("", true))), ("item", n(35, 1))] {
        let mut fresh = s.clone();
        let i = fresh.sections.iter().position(|r| r.id() == id).unwrap();
        fresh.sections[i] = change(&fresh.sections[i], field);
        assert!(EndIntent::capture(&fresh, "item", 100).is_err());
    }
    assert!(
        EndIntent::capture(&s, "item", 100)
            .unwrap()
            .prepare(&s, &witness("different"), 100)
            .is_err()
    );
}

#[test]
fn end_ack_and_readback_need_both_owned_members_exact_content_and_unchanged_history() {
    let s = fixture("Café &amp; 日本😀", true);
    let p = prepare(&s);
    let rows = pair(&p, &s);
    for invalid in [vec![], vec![rows[0].clone()], vec![rows[1].clone()]] {
        assert!(p.validate_ack(&ack(&invalid)).is_err());
    }
    let duplicate = msg(&[
        n(1, 1),
        m(2, &msg(&[m(1, rows[0].raw()), m(1, rows[0].raw())])),
    ]);
    assert!(EditAck::decode(duplicate.raw()).is_err());
    // Tag19 is modified:boolean, not the local counter (tag20). Malformed
    // Boolean wire is rejected before a typed Section/ACK can be constructed.
    let malformed = rows[0].raw().replace_fields(19, &[n(19, 2)]).unwrap();
    assert!(matches!(
        Section::from_raw(malformed),
        Err(crate::canvas::wire::Error::Boolean)
    ));
    for (i, field) in [
        (0, m(12, &body("changed", true))),
        (0, m(12, &body("Café &amp; 日本😀", false))),
        (0, t(33, "wrong")),
        (0, n(19, 0)),
        (0, n(20, 2)),
        (1, m(12, &body("tail", false))),
        (1, m(12, &msg(&[m(1, &msg(&[]))]))),
        (1, t(8, "zzz")),
        (1, t(99, "unknown")),
    ] {
        let mut bad = rows.clone();
        bad[i] = change(&bad[i], field);
        assert!(p.validate_ack(&ack(&bad)).is_err());
    }
    for i in 0..2 {
        let mut bad = rows.clone();
        let wire = bad[i].crdt().unwrap().unwrap();
        bad[i] = change(
            &bad[i],
            m(
                38,
                &wire.replace_fields(10, &[m(10, &msg(&[n(1, 0)]))]).unwrap(),
            ),
        );
        assert!(p.validate_ack(&ack(&bad)).is_err());
    }
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let fresh = readback(&s, &rows);
    for id in ["item", p.child_section(), "other", "title"] {
        let mut bad = fresh.clone();
        bad.sections.retain(|r| r.id() != id);
        assert!(!accepted.confirms_readback(&bad).unwrap_or(false));
        let mut bad = fresh.clone();
        let i = bad.sections.iter().position(|r| r.id() == id).unwrap();
        bad.sections[i] = change(&bad.sections[i], n(2, 50000));
        assert!(!accepted.confirms_readback(&bad).unwrap_or(false));
    }
}

fn save_text(s: &CanvasCollabSnapshot, id: &str, value: &str) -> CanvasCollabSnapshot {
    let p = TextIntent::capture(s, id, 100)
        .unwrap()
        .prepare(s, &mut EditSession::new(), &draft(value), 100)
        .unwrap();
    let original = s.section(id).unwrap();
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let seq = original.sequence().unwrap().unwrap() + 1000;
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
    let row = change(
        &change(&original.merge_partial(&update).unwrap(), n(2, seq)),
        m(38, &wire),
    );
    let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
    let fresh = readback(s, &[row]);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    fresh
}
#[test]
fn accepted_empty_child_and_unchanged_parent_support_independent_normal_text_saves() {
    let s = fixture("Café &amp; 日本😀", true);
    let p = prepare(&s);
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let fresh = readback(&s, &rows);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    let intent = TextIntent::capture(&fresh, p.child_section(), 100).unwrap();
    assert!(intent.draft().runs.is_empty());
    let edited = save_text(&fresh, p.child_section(), "New 👩🏽\u{200d}💻 paragraph");
    assert_eq!(fresh.section("item"), edited.section("item"));
    let final_state = save_text(&edited, "item", "Café &amp; 日本😀 again");
    assert_eq!(
        edited.section(p.child_section()),
        final_state.section(p.child_section())
    );
    assert!(
        !final_state
            .section("item")
            .unwrap()
            .crdt()
            .unwrap()
            .unwrap()
            .has(10)
    );
    let old = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
    let current = Automerge::load(
        final_state
            .section(p.child_section())
            .unwrap()
            .crdt()
            .unwrap()
            .unwrap()
            .bytes(5)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(
        old.get_changes(&[])
            .iter()
            .all(|c| current.get_change_by_hash(&c.hash()).is_some())
    );
}
