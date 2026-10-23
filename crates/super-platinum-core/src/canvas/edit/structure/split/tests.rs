mod continuation;
mod typing;
mod unicode;
use super::*;
use crate::canvas::wire::EditAck;
use automerge::{Automerge, ReadDoc};
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
pub(crate) fn fixture() -> CanvasCollabSnapshot {
    let mut s = super::super::tests::fixture();
    let i = s.sections.iter().position(|s| s.id() == "item").unwrap();
    s.sections[i] = change(
        &s.sections[i],
        m(12, &body("Keep thisSecond paragraph", true)),
    );
    s
}
pub(crate) fn witness() -> SplitWitness {
    SplitWitness::new(
        &TextDraft::from_rtml("Keep thisSecond paragraph").unwrap(),
        9,
        &TextDraft::from_rtml("Keep this").unwrap(),
        &TextDraft::from_rtml("Second paragraph").unwrap(),
    )
    .unwrap()
}
pub(crate) fn prepare(s: &CanvasCollabSnapshot) -> PreparedSplitEdit {
    SplitIntent::capture(s, "item", 100)
        .unwrap()
        .prepare(s, &witness(), 100)
        .unwrap()
}
fn pair(p: &PreparedSplitEdit, s: &CanvasCollabSnapshot) -> Vec<Section> {
    p.document_data()
        .messages(1)
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, update)| {
            let u = Section::from_raw(update.clone()).unwrap();
            let original = s.section(u.id());
            let seq = original.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
            let mut section =
                original.map_or_else(|| u.clone(), |old| old.merge_partial(&u).unwrap());
            for f in [
                n(2, seq),
                t(6, s.thread_id()),
                t(7, s.document_id()),
                m(
                    12,
                    &body(
                        &if i == 0 {
                            p.witness.parent.rtml().unwrap()
                        } else {
                            p.witness.child_draft().rtml().unwrap()
                        },
                        i == 0,
                    ),
                ),
            ] {
                section = change(&section, f);
            }
            if i == 1 {
                let pos = position::base(section.raw().string(8).unwrap().unwrap())
                    .unwrap()
                    .to_owned();
                section = change(&change(&section, t(8, &pos)), t(21, &pos));
            }
            let wire = section
                .crdt()
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[n(3, seq)])
                .unwrap()
                .replace_fields(4, &[n(4, 1_000_000)])
                .unwrap();
            change(&section, m(38, &wire))
        })
        .collect()
}
fn ack(sections: &[Section]) -> EditAck {
    EditAck::decode(
        msg(&[
            n(1, 1),
            m(
                2,
                &msg(&sections.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>()),
            ),
        ])
        .raw(),
    )
    .unwrap()
}
fn readback(s: &CanvasCollabSnapshot, sections: &[Section]) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    for row in sections {
        if let Some(i) = s.sections.iter().position(|x| x.id() == row.id()) {
            s.sections[i] = row.clone();
        } else {
            s.sections.push(row.clone());
        }
    }
    s
}
#[test]
fn split_preserves_original_history_and_independent_pair_ack_readback() {
    for retained in [false, true] {
        let mut s = fixture();
        if retained {
            let i = s.sections.iter().position(|s| s.id() == "item").unwrap();
            let seed =
                text::crdt::bootstrap(&s.sections[i], &witness().before, "0123456789ab", 100)
                    .unwrap()
                    .replace_fields(4, &[n(4, 1_000_000)])
                    .unwrap();
            s.sections[i] = change(&s.sections[i], m(38, &seed));
        }
        let original = s.sections.clone();
        let p = prepare(&s);
        validate_checkpoint_split(
            s.section("item").unwrap(),
            p.child_section(),
            &p.intent.position,
            p.session_id(),
            &p.witness,
            p.document_data(),
        )
        .unwrap();
        let lineage = &p.lineage;
        assert_eq!(lineage.uint(1).unwrap(), Some(0));
        assert_eq!(lineage.uint(4).unwrap(), Some(9));
        assert_eq!(
            lineage.string(2).unwrap(),
            Some(format!("{}00000000", p.session_id()).as_str())
        );
        let before = Automerge::load(lineage.bytes(7).unwrap().unwrap()).unwrap();
        text::crdt::check_document_state(&before, &witness().before, false).unwrap();
        let shortened = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        assert!(
            before
                .get_changes(&[])
                .iter()
                .all(|c| shortened.get_change_by_hash(&c.hash()).is_some())
        );
        let sections = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&sections)).unwrap();
        assert!(
            accepted
                .confirms_readback(&readback(&s, &sections))
                .unwrap()
        );
        assert_eq!(s.sections, original);
        assert!(p.checkpoint().is_ok());
    }
}
#[test]
fn split_witness_requires_original_plain_middle_event_shape_and_serialized_offset() {
    let full = TextDraft::from_rtml("A&amp;B").unwrap();
    let w = SplitWitness::new(
        &full,
        2,
        &TextDraft::from_rtml("A&amp;").unwrap(),
        &TextDraft::from_rtml("B").unwrap(),
    )
    .unwrap();
    let mut s = fixture();
    let i = s.sections.iter().position(|s| s.id() == "item").unwrap();
    s.sections[i] = change(&s.sections[i], m(12, &body("A&amp;B", true)));
    let p = SplitIntent::capture(&s, "item", 100)
        .unwrap()
        .prepare(&s, &w, 100)
        .unwrap();
    assert_eq!(p.lineage.uint(4).unwrap(), Some(6));
    for (before, offset, left, right) in [
        ("abc", 0, "", "abc"),
        ("abc", 3, "abc", ""),
        ("abc", 1, "a", "changed"),
        ("<b>abc</b>", 1, "a", "bc"),
        ("a\u{1f600}b", 2, "a", "\u{1f600}b"),
        ("abc", 1, "<i>a</i>", "bc"),
    ] {
        assert!(
            SplitWitness::new(
                &TextDraft::from_rtml(before).unwrap(),
                offset,
                &TextDraft::from_rtml(left).unwrap(),
                &TextDraft::from_rtml(right).unwrap()
            )
            .is_err()
        );
    }
}
#[test]
fn split_fresh_inventory_revision_title_permission_and_trailing_empty_guards() {
    let s = fixture();
    for id in ["item", "other", "title"] {
        let intent = SplitIntent::capture(&s, "item", 100).unwrap();
        let mut fresh = s.clone();
        let i = fresh.sections.iter().position(|x| x.id() == id).unwrap();
        fresh.sections[i] = change(&fresh.sections[i], n(2, 12000));
        assert!(intent.prepare(&fresh, &witness(), 100).is_err());
    }
    let mut changed = s.clone();
    let i = changed
        .sections
        .iter()
        .position(|x| x.id() == "other")
        .unwrap();
    changed.sections[i] = change(&changed.sections[i], m(12, &body("", true)));
    assert!(SplitIntent::capture(&changed, "item", 100).is_err());
    let other = SplitWitness::new(
        &TextDraft::from_rtml("Wrong paragraph").unwrap(),
        5,
        &TextDraft::from_rtml("Wrong").unwrap(),
        &TextDraft::from_rtml(" paragraph").unwrap(),
    )
    .unwrap();
    assert!(
        SplitIntent::capture(&s, "item", 100)
            .unwrap()
            .prepare(&s, &other, 100)
            .is_err()
    );
}
#[test]
fn split_ack_needs_complete_owned_pair_and_exact_lineage() {
    let s = fixture();
    let p = prepare(&s);
    let rows = pair(&p, &s);
    for invalid in [vec![], vec![rows[0].clone()], vec![rows[1].clone()]] {
        assert!(p.validate_ack(&ack(&invalid)).is_err());
    }
    // Duplicate section IDs are rejected at the outer protobuf decoder before
    // a typed acknowledgement exists; do not unwrap a deliberately invalid ACK.
    let duplicate = msg(&[
        n(1, 1),
        m(2, &msg(&[m(1, rows[0].raw()), m(1, rows[0].raw())])),
    ]);
    assert!(matches!(
        EditAck::decode(duplicate.raw()),
        Err(crate::canvas::wire::Error::Duplicate)
    ));
    let wire = rows[0].crdt().unwrap().unwrap();
    for field in [
        n(1, 1),
        t(2, "wrong-actor"),
        t(3, "wrong-child"),
        n(4, 8),
        t(5, "[]"),
        t(6, "wrong-base"),
        F::bytes(7, b"broken").unwrap(),
        t(99, "unknown"),
    ] {
        let altered = p
            .lineage
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
        let mut bad = rows.clone();
        bad[0] = change(
            &bad[0],
            m(38, &wire.replace_fields(10, &[m(10, &altered)]).unwrap()),
        );
        assert!(p.validate_ack(&ack(&bad)).is_err());
    }
    for altered in [
        wire.remove(10).unwrap(),
        wire.replace_fields(10, &[m(10, &p.lineage), m(10, &p.lineage)])
            .unwrap(),
    ] {
        let mut bad = rows.clone();
        bad[0] = change(&bad[0], m(38, &altered));
        assert!(p.validate_ack(&ack(&bad)).is_err());
    }
    let mut bad = rows.clone();
    bad[1] = change(
        &bad[1],
        m(
            38,
            &bad[1]
                .crdt()
                .unwrap()
                .unwrap()
                .replace_fields(10, &[m(10, &p.lineage)])
                .unwrap(),
        ),
    );
    assert!(p.validate_ack(&ack(&bad)).is_err());
}
#[test]
fn split_ack_rejects_wrong_content_language_position_counter_trace_and_history() {
    let s = fixture();
    let p = prepare(&s);
    let rows = pair(&p, &s);
    for (i, f) in [
        (0, m(12, &body("Keep this", false))),
        (0, m(12, &body("Other", true))),
        (0, t(6, "foreign")),
        (0, t(33, "foreign-0")),
        (0, n(20, 2)),
        (1, t(8, "zzz")),
        (1, t(21, "zzz")),
        (1, t(99, "unknown")),
        (1, n(3, 1)),
    ] {
        let mut bad = rows.clone();
        bad[i] = change(&bad[i], f);
        assert!(p.validate_ack(&ack(&bad)).is_err());
    }
    let mut bad = rows.clone();
    let replacement = text::crdt::bootstrap(&bad[0], &witness().parent, "abcdef012345", 100)
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap()
        .replace_fields(10, &[m(10, &p.lineage)])
        .unwrap();
    bad[0] = change(&bad[0], m(38, &replacement));
    assert!(p.validate_ack(&ack(&bad)).is_err());
    let outer = ack(&rows).raw().replace_fields(99, &[n(99, 1)]).unwrap();
    assert!(
        p.validate_ack(&EditAck::decode(outer.raw()).unwrap())
            .is_err()
    );
}
#[test]
fn split_readback_must_keep_pair_untouched_inventory_and_response_lineage() {
    let s = fixture();
    let p = prepare(&s);
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let fresh = readback(&s, &rows);
    for id in ["item", p.child_section(), "other", "title"] {
        let mut missing = fresh.clone();
        missing.sections.retain(|s| s.id() != id);
        assert!(!accepted.confirms_readback(&missing).unwrap());
    }
    for id in ["item", p.child_section(), "other", "title"] {
        let mut changed = fresh.clone();
        let i = changed.sections.iter().position(|s| s.id() == id).unwrap();
        changed.sections[i] = change(&changed.sections[i], n(2, 13000));
        assert!(!accepted.confirms_readback(&changed).unwrap_or(false));
    }
    let mut absent = fresh.clone();
    let i = absent
        .sections
        .iter()
        .position(|s| s.id() == "item")
        .unwrap();
    let wire = absent.sections[i]
        .crdt()
        .unwrap()
        .unwrap()
        .remove(10)
        .unwrap();
    absent.sections[i] = change(&absent.sections[i], m(38, &wire));
    assert!(!accepted.confirms_readback(&absent).unwrap());
}
