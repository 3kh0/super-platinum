use super::*;
fn fixture() -> CanvasCollabSnapshot {
    super::super::tests::plain_without_crdt()
}
fn num(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn nested(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn ack(prepared: &PreparedCheckboxEdit, sequence: u64, expiry: u64) -> EditAck {
    let crdt = prepared
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap()
        .replace_fields(3, &[num(3, sequence)])
        .unwrap()
        .replace_fields(4, &[num(4, expiry)])
        .unwrap();
    let attrs = prepared
        .canonical_section
        .attributes()
        .unwrap()
        .unwrap()
        .replace_fields(2, &[num(2, u64::from(prepared.desired))])
        .unwrap();
    let raw = prepared
        .canonical_section
        .raw()
        .replace_fields(2, &[num(2, sequence)])
        .unwrap()
        .replace_fields(16, &[nested(16, &attrs)])
        .unwrap()
        .replace_fields(38, &[nested(38, &crdt)])
        .unwrap()
        .replace_fields(18, &[num(18, 1)])
        .unwrap()
        .replace_fields(103, &[num(103, 1)])
        .unwrap()
        .replace_fields(104, &[num(104, 1)])
        .unwrap();
    let data = RawMessage::from_fields(&[nested(1, &raw)]).unwrap();
    EditAck::decode(
        RawMessage::from_fields(&[num(1, 1), nested(2, &data)])
            .unwrap()
            .raw(),
    )
    .unwrap()
}
fn canonical_after(
    snapshot: &CanvasCollabSnapshot,
    accepted: &AcceptedCheckbox,
) -> CanvasCollabSnapshot {
    let mut fresh = snapshot.clone();
    fresh.sections[0] = Section::from_raw(association(accepted.section()).unwrap()).unwrap();
    fresh
}
fn prepared(
    snapshot: &CanvasCollabSnapshot,
    session: &mut EditSession,
    desired: bool,
    now: u64,
) -> PreparedCheckboxEdit {
    CheckboxIntent::capture(snapshot, "item", desired, now)
        .unwrap()
        .prepare(snapshot, session, now)
        .unwrap()
}
fn crdt(prepared: &PreparedCheckboxEdit) -> RawMessage {
    prepared
        .document_data()
        .message(1)
        .unwrap()
        .unwrap()
        .message(38)
        .unwrap()
        .unwrap()
}
#[test]
fn accepted_crdt_survives_omitted_load_field_and_second_toggle_preserves_history() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let first = prepared(&snapshot, &mut session, true, 1_000_000);
    let accepted = first
        .validate_ack(&ack(&first, 12_000, 10_000_000))
        .unwrap();
    session.remember_accepted(&accepted).unwrap();
    let fresh = canonical_after(&snapshot, &accepted);
    let second = prepared(&fresh, &mut session, false, 2_000_000);
    assert_eq!(first.session_id(), second.session_id());
    assert_eq!(second.sequence(), 2);
    assert_eq!(
        second
            .document_data()
            .message(1)
            .unwrap()
            .unwrap()
            .uint(20)
            .unwrap(),
        Some(2)
    );
    let a = crdt(&first);
    let b = crdt(&second);
    for tag in [1, 2, 5, 6, 8] {
        assert_eq!(encoded(&a, tag).unwrap(), encoded(&b, tag).unwrap());
    }
    assert_eq!(b.uint(3).unwrap(), Some(12_000));
    assert!(!b.has(4));
    assert!(!b.has(7));
    let restored = second
        .validate_ack(&ack(&second, 13_000, 20_000_000))
        .unwrap();
    session.remember_accepted(&restored).unwrap();
}
#[test]
fn local_seed_is_reused_for_a_new_explicit_attempt_that_was_known_not_sent() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let first = prepared(&snapshot, &mut session, true, 1_000_000);
    let second = prepared(&snapshot, &mut session, true, 2_000_000);
    assert_eq!(first.session_id(), second.session_id());
    assert_eq!(crdt(&first), crdt(&second));
    assert_eq!(second.sequence(), 2);
}
#[test]
fn expired_retention_rotates_identity_and_rejects_late_old_receipt() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let first = prepared(&snapshot, &mut session, true, 1_000_000);
    let accepted = first.validate_ack(&ack(&first, 12_000, 2_000_000)).unwrap();
    session.remember_accepted(&accepted).unwrap();
    let fresh = canonical_after(&snapshot, &accepted);
    let next = prepared(&fresh, &mut session, false, 3_000_000);
    assert_ne!(first.session_id(), next.session_id());
    assert_eq!(next.sequence(), 1);
    assert_eq!(crdt(&next).uint(2).unwrap(), Some(12_000));
    assert_ne!(
        crdt(&first).string(8).unwrap(),
        crdt(&next).string(8).unwrap()
    );
    assert!(matches!(
        session.remember_accepted(&accepted),
        Err(Error::Changed)
    ));
    assert_eq!(session.sequence, 1);
}
#[test]
fn stale_intent_rejects_before_rotation_but_new_fresh_intent_can_reset_drifted_history() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let first = prepared(&snapshot, &mut session, true, 1_000_000);
    let accepted = first
        .validate_ack(&ack(&first, 12_000, 10_000_000))
        .unwrap();
    session.remember_accepted(&accepted).unwrap();
    let mut fresh = canonical_after(&snapshot, &accepted);
    let old = CheckboxIntent::capture(&fresh, "item", false, 2_000_000).unwrap();
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(2, &[num(2, 13_000)])
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        old.prepare(&fresh, &mut session, 2_000_000),
        Err(Error::Changed)
    ));
    assert_eq!(session.id, first.session_id());
    assert_eq!(session.sequence, 1);
    let next = prepared(&fresh, &mut session, false, 2_000_000);
    assert_ne!(next.session_id(), first.session_id());
    assert_eq!(next.sequence(), 1);
}
#[test]
fn unusable_ack_cannot_rebootstrap_under_same_actor_and_server_failure_is_unconfirmed() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let first = prepared(&snapshot, &mut session, true, 1_000_000);
    let mut acknowledgement = ack(&first, 12_000, 10_000_000);
    let section = &acknowledgement.final_data.as_ref().unwrap().sections()[0];
    let failed = section
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(7, &[num(7, 1)])
        .unwrap();
    let failed_section = section
        .raw()
        .replace_fields(38, &[nested(38, &failed)])
        .unwrap();
    let data = RawMessage::from_fields(&[nested(1, &failed_section)]).unwrap();
    acknowledgement.final_data =
        Some(super::super::super::wire::DocumentData::from_raw(data).unwrap());
    assert!(matches!(
        first.validate_ack(&acknowledgement),
        Err(Error::Unconfirmed)
    ));
    acknowledgement = ack(&first, 12_000, 10_000_000);
    let missing = acknowledgement.final_data.as_ref().unwrap().sections()[0]
        .raw()
        .remove(38)
        .unwrap();
    acknowledgement.final_data = Some(
        super::super::super::wire::DocumentData::from_raw(
            RawMessage::from_fields(&[nested(1, &missing)]).unwrap(),
        )
        .unwrap(),
    );
    let accepted = first.validate_ack(&acknowledgement).unwrap();
    assert!(matches!(
        session.remember_accepted(&accepted),
        Err(Error::Unsupported)
    ));
    let fresh = canonical_after(&snapshot, &accepted);
    let next = prepared(&fresh, &mut session, false, 2_000_000);
    assert_ne!(first.session_id(), next.session_id());
}
#[test]
fn capacity_reset_changes_actor_without_discarding_history_under_the_same_identity() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let first = prepared(&snapshot, &mut session, true, 1_000_000);
    let (key, entry) = session
        .retained
        .iter()
        .next()
        .map(|(key, entry)| (key.clone(), entry.clone()))
        .unwrap();
    for i in 0..MAX_SECTIONS - 1 {
        let mut key = key.clone();
        key.section = format!("other-{i}");
        session.retained.insert(key, entry.clone());
    }
    let mut fresh = snapshot.clone();
    fresh.sections[0] = Section::from_raw(
        fresh.sections[0]
            .raw()
            .replace_fields(1, &[F::bytes(1, b"new-item").unwrap()])
            .unwrap(),
    )
    .unwrap();
    let intent = CheckboxIntent::capture(&fresh, "new-item", true, 2_000_000).unwrap();
    let next = intent.prepare(&fresh, &mut session, 2_000_000).unwrap();
    assert_ne!(next.session_id(), first.session_id());
    assert_eq!(next.sequence(), 1);
    assert_eq!(session.retained.len(), 1);
    assert_eq!(session.local.len(), 1);
}

#[test]
fn retained_server_expiry_is_rechecked_at_final_send_even_when_load_omits_crdt() {
    let snapshot = fixture();
    let mut session = EditSession::new();
    let first = prepared(&snapshot, &mut session, true, 1_000_000);
    let accepted = first.validate_ack(&ack(&first, 12_000, 3_000_000)).unwrap();
    session.remember_accepted(&accepted).unwrap();
    let fresh = canonical_after(&snapshot, &accepted);
    assert!(!fresh.sections[0].raw().has(38));
    let second = prepared(&fresh, &mut session, false, 2_000_000);
    assert!(second.validate_snapshot(&fresh, 2_999_999).is_ok());
    assert!(matches!(
        second.validate_snapshot(&fresh, 3_000_000),
        Err(Error::Expired)
    ));
    assert!(matches!(
        second.validate_snapshot(&fresh, 4_000_000),
        Err(Error::Expired)
    ));
    assert!(
        !crdt(&second).has(4),
        "expiry remains local request provenance"
    );
}
