use super::*;
use crate::canvas::edit::{
    EditSession,
    text::{PreparedTextEdit, TextIntent},
};

fn current() -> (CanvasCollabSnapshot, String) {
    let original = fixture();
    let prepared = insert(&original);
    let inserted = canonical_ack(&prepared, &original);
    let receipt = prepared.validate_ack(&ack(&inserted)).unwrap();
    let mut current = readback(&original, &inserted);
    assert!(receipt.confirms_readback(&current).unwrap());
    // A fresh canonical extension is opaque to the content edit. Its unusual
    // encoding must remain in the exact raw baseline/readback association.
    let mut bytes = inserted.raw().raw().to_vec();
    bytes.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]);
    current = readback(&current, &Section::decode(&bytes).unwrap());
    (current, inserted.id().to_owned())
}
fn desired() -> TextDraft {
    TextDraft::from_rtml("&amp;&lt;<br>\u{1f600}<i>Edited</i>").unwrap()
}
fn prepared(
    snapshot: &CanvasCollabSnapshot,
    id: &str,
    session: &mut EditSession,
) -> PreparedTextEdit {
    TextIntent::capture(snapshot, id, 100)
        .unwrap()
        .prepare(snapshot, session, &desired(), 100)
        .unwrap()
}
fn text_ack(prepared: &PreparedTextEdit, snapshot: &CanvasCollabSnapshot) -> Section {
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    let canonical = snapshot.section(update.id()).unwrap();
    let sequence = canonical.sequence().unwrap().unwrap() + 1000;
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
    changed(
        &changed(&canonical.merge_partial(&update).unwrap(), n(2, sequence)),
        m(38, &wire),
    )
}
fn contains_history(old: &Section, new_wire: &RawMessage) {
    let old = automerge::Automerge::load(old.crdt().unwrap().unwrap().bytes(5).unwrap().unwrap())
        .unwrap();
    let new = automerge::Automerge::load(new_wire.bytes(5).unwrap().unwrap()).unwrap();
    assert!(
        old.get_changes(&[])
            .iter()
            .all(|c| new.get_change_by_hash(&c.hash()).is_some())
    );
}

#[test]
fn inserted_modern_base_zero_supports_two_text_edits_with_exact_history() {
    let (snapshot, id) = current();
    let original = snapshot.section(&id).unwrap();
    let mut session = EditSession::new();
    let edit = prepared(&snapshot, &id, &mut session);
    edit.validate_snapshot(&snapshot, 100).unwrap();
    let outgoing = edit
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap();
    assert_eq!(outgoing.uint(2).unwrap(), Some(0));
    assert_eq!(outgoing.uint(3).unwrap(), original.sequence().unwrap());
    contains_history(original, &outgoing);
    let canonical = text_ack(&edit, &snapshot);
    let accepted = edit.validate_ack(&ack(&canonical)).unwrap();
    let fresh = readback(&snapshot, &canonical);
    assert!(accepted.confirms_readback(&fresh).unwrap());
    session.remember_text_accepted(&accepted).unwrap();
    assert!(
        canonical
            .raw()
            .raw()
            .windows(4)
            .any(|b| b == [0xc8, 0x3e, 0x81, 0x00])
    );
    for old in snapshot.sections.iter().filter(|s| s.id() != id) {
        assert_eq!(fresh.section(old.id()), Some(old));
    }
    let later = TextDraft::from_rtml("&amp;&lt;<br>\u{1f600}<b>Again</b>").unwrap();
    let second = TextIntent::capture(&fresh, &id, 100)
        .unwrap()
        .prepare(&fresh, &mut session, &later, 100)
        .unwrap();
    assert_eq!(second.sequence(), 2);
    let second_wire = second
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap();
    assert_eq!(second_wire.uint(2).unwrap(), Some(0));
    contains_history(&canonical, &second_wire);
    let second_canonical = text_ack(&second, &fresh);
    let receipt = second.validate_ack(&ack(&second_canonical)).unwrap();
    assert!(
        receipt
            .confirms_readback(&readback(&fresh, &second_canonical))
            .unwrap()
    );
    assert_eq!(
        second.validate_snapshot(&fresh, 1_000_000),
        Err(Error::Expired)
    );
}

#[test]
fn zero_base_text_refuses_legacy_expired_unprocessed_and_unknown_history() {
    let (snapshot, id) = current();
    let canonical = snapshot.section(&id).unwrap();
    let wire = canonical.crdt().unwrap().unwrap();
    for field in [
        n(3, 0),
        n(4, 100),
        n(7, 1),
        t(5, "malformed"),
        t(6, wire.string(8).unwrap().unwrap()),
        n(1001, 1),
    ] {
        let raw = RawMessage::decode(field.raw()).unwrap();
        let target = changed(canonical, m(38, &wire.merge_partial(&raw).unwrap()));
        assert!(TextIntent::capture(&readback(&snapshot, &target), &id, 100).is_err());
    }
    let legacy = changed(
        &changed(canonical, t(1, "legacy-zero")),
        m(38, &wire.replace_fields(1, &[t(1, "legacy-zero")]).unwrap()),
    );
    assert!(TextIntent::capture(&readback(&snapshot, &legacy), "legacy-zero", 100).is_err());
    let zero_sequence = changed(canonical, n(2, 0));
    assert!(TextIntent::capture(&readback(&snapshot, &zero_sequence), &id, 100).is_err());
}

#[test]
fn zero_base_text_ack_rejects_semantic_extensions_and_base_replacement() {
    let (snapshot, id) = current();
    let edit = prepared(&snapshot, &id, &mut EditSession::new());
    let canonical = text_ack(&edit, &snapshot);
    for field in [
        m(15, &msg(&[])),
        m(16, &msg(&[t(29, "unrequested")])),
        n(1002, 1),
        n(1001, 2),
        t(8, "zzzz"),
        n(20, 99),
    ] {
        assert!(
            edit.validate_ack(&ack(&changed(&canonical, field)))
                .is_err()
        );
    }
    let wire = canonical
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(2, &[n(2, 1)])
        .unwrap();
    assert!(
        edit.validate_ack(&ack(&changed(&canonical, m(38, &wire))))
            .is_err()
    );
    assert!(edit.validate_ack(&ack(&canonical)).is_ok());
}

#[test]
fn zero_base_text_readback_requires_present_exact_history_revision_and_unknowns() {
    let (snapshot, id) = current();
    let edit = prepared(&snapshot, &id, &mut EditSession::new());
    let canonical = text_ack(&edit, &snapshot);
    let accepted = edit.validate_ack(&ack(&canonical)).unwrap();
    let replacement = crdt::insert(&id, &desired(), "fedcba987654", 100)
        .unwrap()
        .replace_fields(3, &[n(3, canonical.sequence().unwrap().unwrap())])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    let malformed = canonical
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(5, &[t(5, "invalid")])
        .unwrap();
    for target in [
        Section::from_raw(canonical.raw().remove(38).unwrap()).unwrap(),
        changed(&canonical, m(38, &replacement)),
        changed(&canonical, m(38, &malformed)),
        Section::from_raw(canonical.raw().remove(1001).unwrap()).unwrap(),
        changed(&canonical, n(20, 99)),
        changed(
            &canonical,
            n(2, canonical.sequence().unwrap().unwrap() + 1000),
        ),
    ] {
        assert!(
            !accepted
                .confirms_readback(&readback(&snapshot, &target))
                .unwrap()
        );
    }
    assert!(
        accepted
            .confirms_readback(&readback(&snapshot, &canonical))
            .unwrap()
    );
}

#[test]
fn zero_base_retention_never_bootstraps_missing_history_or_consumes_stale_preflight() {
    let (snapshot, id) = current();
    let mut session = EditSession::new();
    let edit = prepared(&snapshot, &id, &mut session);
    let canonical = text_ack(&edit, &snapshot);
    let accepted = edit.validate_ack(&ack(&canonical)).unwrap();
    session.remember_text_accepted(&accepted).unwrap();
    let missing = Section::from_raw(canonical.raw().remove(38).unwrap()).unwrap();
    let fresh = readback(&snapshot, &missing);
    let intent = TextIntent::capture(&fresh, &id, 100).unwrap();
    let desired = TextDraft::from_rtml("Next explicit edit").unwrap();
    let prior_id = session.id.clone();
    let prior_sequence = session.sequence;
    assert!(matches!(
        intent.prepare(&fresh, &mut session, &desired, 100),
        Err(Error::Unsupported)
    ));
    assert_eq!(session.id, prior_id);
    assert_eq!(session.sequence, prior_sequence);
    let canonical_snapshot = readback(&snapshot, &canonical);
    let intent = TextIntent::capture(&canonical_snapshot, &id, 100).unwrap();
    let drifted = readback(&canonical_snapshot, &changed(&canonical, n(1002, 1)));
    assert!(matches!(
        intent.prepare(&drifted, &mut session, &desired, 100),
        Err(Error::Changed)
    ));
    assert_eq!(session.id, prior_id);
    assert_eq!(session.sequence, prior_sequence);
}

#[test]
fn zero_base_text_does_not_admit_concurrent_conflicting_heads() {
    use automerge::transaction::Transactable;
    let (snapshot, id) = current();
    let canonical = snapshot.section(&id).unwrap();
    let wire = canonical.crdt().unwrap().unwrap();
    let original = automerge::Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    let mut left = original
        .fork()
        .with_actor("11111111111100000000".parse().unwrap());
    let mut right = original
        .fork()
        .with_actor("22222222222200000000".parse().unwrap());
    for doc in [&mut left, &mut right] {
        let mut tx = doc.transaction();
        // A same-value put is elided by Automerge; prove the fixture actually
        // creates history while preserving the final undeleted content.
        tx.put(ROOT, "deleted", false).unwrap();
        assert_eq!(tx.pending_ops(), 0);
        tx.put(ROOT, "deleted", true).unwrap();
        tx.put(ROOT, "deleted", false).unwrap();
        assert_eq!(tx.pending_ops(), 2);
        assert!(tx.commit().0.is_some());
    }
    left.merge(&mut right).unwrap();
    assert_eq!(left.get_heads().len(), 2);
    let heads = left
        .get_heads()
        .iter()
        .map(|h| t(6, &h.to_string()))
        .collect::<Vec<_>>();
    let wire = wire
        .replace_fields(5, &[F::bytes(5, &left.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &heads)
        .unwrap();
    let target = changed(canonical, m(38, &wire));
    assert!(TextIntent::capture(&readback(&snapshot, &target), &id, 100).is_err());
}

#[test]
fn zero_base_text_requires_exact_sent_processed_head_beyond_descendant_history() {
    use automerge::transaction::Transactable;
    let (snapshot, id) = current();
    let edit = prepared(&snapshot, &id, &mut EditSession::new());
    let canonical = text_ack(&edit, &snapshot);
    let accepted = edit.validate_ack(&ack(&canonical)).unwrap();
    let wire = canonical.crdt().unwrap().unwrap();
    let mut doc = automerge::Automerge::load(wire.bytes(5).unwrap().unwrap())
        .unwrap()
        .with_actor("33333333333300000000".parse().unwrap());
    let sent_head = doc.get_heads()[0];
    let mut tx = doc.transaction();
    // A same-value put commits no change. Two real operations retain the
    // final content but create the descendant required by this negative case.
    tx.put(ROOT, "deleted", false).unwrap();
    assert_eq!(tx.pending_ops(), 0);
    tx.put(ROOT, "deleted", true).unwrap();
    tx.put(ROOT, "deleted", false).unwrap();
    assert_eq!(tx.pending_ops(), 2);
    assert!(tx.commit().0.is_some());
    let descendant = doc.get_heads()[0];
    assert_ne!(sent_head, descendant);
    assert!(doc.get_change_by_hash(&sent_head).is_some());
    let wire = wire
        .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &[t(6, &descendant.to_string())])
        .unwrap();
    let target = changed(&canonical, m(38, &wire));
    // The descendant is clean and processed, with all sent history retained.
    // That alone does not prove admission of the exact outgoing save head.
    canonical_zero_base_history(&wire, &target, &desired(), Some(0)).unwrap();
    assert!(edit.validate_ack(&ack(&target)).is_err());
    assert!(
        !accepted
            .confirms_readback(&readback(&snapshot, &target))
            .unwrap()
    );
    let wire = wire
        .replace_fields(
            6,
            &[t(6, &sent_head.to_string()), t(6, &descendant.to_string())],
        )
        .unwrap();
    let target = changed(&canonical, m(38, &wire));
    assert!(edit.validate_ack(&ack(&target)).is_ok());
    assert!(
        accepted
            .confirms_readback(&readback(&snapshot, &target))
            .unwrap()
    );
}
