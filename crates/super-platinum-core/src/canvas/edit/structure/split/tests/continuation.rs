use super::*;
use crate::canvas::edit::{
    EditSession,
    text::{PreparedTextEdit, TextIntent},
};
fn current() -> (CanvasCollabSnapshot, String) {
    let s = fixture();
    let p = prepare(&s);
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let fresh = readback(&s, &rows);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    (fresh, p.child_section().into())
}
pub(super) fn edit(
    s: &CanvasCollabSnapshot,
    session: &mut EditSession,
    value: &str,
) -> PreparedTextEdit {
    TextIntent::capture(s, "item", 100)
        .unwrap()
        .prepare(s, session, &TextDraft::from_rtml(value).unwrap(), 100)
        .unwrap()
}
pub(super) fn received(p: &PreparedTextEdit, s: &CanvasCollabSnapshot, value: &str) -> Section {
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let original = s.section("item").unwrap();
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
    let mut row = original.merge_partial(&update).unwrap();
    for f in [n(2, seq), m(12, &body(value, true)), m(38, &wire)] {
        row = change(&row, f);
    }
    row
}
#[test]
fn accepted_split_parent_remains_editable_and_keeps_lineage_across_two_text_saves() {
    let (mut s, child) = current();
    let original_child = s.section(&child).unwrap().clone();
    let lineage = s
        .section("item")
        .unwrap()
        .crdt()
        .unwrap()
        .unwrap()
        .message(10)
        .unwrap()
        .unwrap();
    let mut session = EditSession::new();
    assert!(TextIntent::capture(&s, &child, 100).is_ok());
    for value in ["Keep this updated", "Keep this <b>again</b>"] {
        let p = edit(&s, &mut session, value);
        p.validate_snapshot(&s, 100).unwrap();
        let out = p
            .document_data()
            .message(1)
            .unwrap()
            .unwrap()
            .message(38)
            .unwrap()
            .unwrap();
        assert_eq!(out.message(10).unwrap().unwrap(), lineage);
        let row = received(&p, &s, value);
        let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
        s = readback(&s, &[row]);
        assert!(accepted.confirms_readback(&s).unwrap());
        session.remember_text_accepted(&accepted).unwrap();
        assert_eq!(s.section(&child), Some(&original_child));
    }
    let actions = super::super::super::paragraph_actions(&s, &["item"], 100);
    let actions = actions[0].as_ref().unwrap();
    assert!(actions.delete);
    assert!(actions.insert_before && actions.insert_after);
    assert!(super::super::super::StructureIntent::delete(&s, "item", 100).is_ok());
    assert!(SplitIntent::capture(&s, "item", 100).is_err());
}
#[test]
fn split_parent_text_preflight_binds_exact_child_and_missing_or_corrupt_lineage_is_refused() {
    let (s, child) = current();
    let intent = TextIntent::capture(&s, "item", 100).unwrap();
    let mut changed = s.clone();
    let i = changed
        .sections
        .iter()
        .position(|r| r.id() == child)
        .unwrap();
    changed.sections[i] = change(&changed.sections[i], n(2, 3000));
    assert!(
        intent
            .prepare(
                &changed,
                &mut EditSession::new(),
                &TextDraft::from_rtml("later").unwrap(),
                100
            )
            .is_err()
    );
    let mut missing = s.clone();
    missing.sections.retain(|r| r.id() != child);
    assert!(TextIntent::capture(&missing, "item", 100).is_err());
    let i = s.sections.iter().position(|r| r.id() == "item").unwrap();
    let original_wire = s.sections[i].crdt().unwrap().unwrap();
    let lineage = original_wire.message(10).unwrap().unwrap();
    for f in [
        n(1, 1),
        t(2, "abcdefabcdef00000000"),
        n(4, 8),
        t(5, "[]"),
        t(6, "changed"),
        t(99, "unknown"),
    ] {
        let mut broken = s.clone();
        let altered = lineage
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap();
        broken.sections[i] = change(
            &broken.sections[i],
            m(
                38,
                &original_wire
                    .replace_fields(10, &[m(10, &altered)])
                    .unwrap(),
            ),
        );
        assert!(TextIntent::capture(&broken, "item", 100).is_err());
    }
    let mut broken = s.clone();
    let j = broken
        .sections
        .iter()
        .position(|r| r.id() == child)
        .unwrap();
    broken.sections[j] = Section::from_raw(broken.sections[j].raw().remove(38).unwrap()).unwrap();
    assert!(TextIntent::capture(&broken, "item", 100).is_err());
}
#[test]
fn split_parent_ack_and_independent_readback_require_lineage_and_whole_child_association() {
    let (s, child) = current();
    let p = edit(&s, &mut EditSession::new(), "Keep this updated");
    let row = received(&p, &s, "Keep this updated");
    let wire = row.crdt().unwrap().unwrap();
    for altered in [
        wire.remove(10).unwrap(),
        wire.replace_fields(
            10,
            &[m(
                10,
                &wire
                    .message(10)
                    .unwrap()
                    .unwrap()
                    .replace_fields(3, &[t(3, "temp:C:xyz0000000000000000000000000")])
                    .unwrap(),
            )],
        )
        .unwrap(),
    ] {
        assert!(
            p.validate_ack(&ack(&[change(&row, m(38, &altered))]))
                .is_err()
        );
    }
    let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
    let good = readback(&s, &[row]);
    assert!(accepted.confirms_readback(&good).unwrap());
    let mut missing = good.clone();
    missing.sections.retain(|r| r.id() != child);
    assert!(!accepted.confirms_readback(&missing).unwrap());
    let mut later = good.clone();
    let i = later
        .sections
        .iter()
        .position(|r| r.id() == "item")
        .unwrap();
    later.sections[i] = change(&later.sections[i], n(2, 40000));
    assert!(!accepted.confirms_readback(&later).unwrap());
    let mut missing_wire = good.clone();
    missing_wire.sections[i] = change(&missing_wire.sections[i], m(38, &wire.remove(10).unwrap()));
    assert!(!accepted.confirms_readback(&missing_wire).unwrap_or(false));
}
#[test]
fn known_split_parent_omitted_history_does_not_rebootstrap_from_retained_text_session() {
    let (s, _) = current();
    let mut session = EditSession::new();
    let p = edit(&s, &mut session, "Keep this updated");
    let row = received(&p, &s, "Keep this updated");
    let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
    session.remember_text_accepted(&accepted).unwrap();
    let mut fresh = readback(&s, &[row]);
    let i = fresh
        .sections
        .iter()
        .position(|r| r.id() == "item")
        .unwrap();
    fresh.sections[i] = Section::from_raw(fresh.sections[i].raw().remove(38).unwrap()).unwrap();
    let intent = TextIntent::capture(&fresh, "item", 100).unwrap();
    assert!(
        intent
            .prepare(
                &fresh,
                &mut session,
                &TextDraft::from_rtml("another").unwrap(),
                100
            )
            .is_err()
    );
}
