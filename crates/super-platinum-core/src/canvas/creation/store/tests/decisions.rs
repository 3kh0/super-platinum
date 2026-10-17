use super::*;

#[test]
fn archive_first_late_created_retains_original_and_new_draft_unchanged() {
    let temp = Temp::new();
    let mut store = temp.store();
    let initial = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &initial);
    let operation = writer();
    let new_writer = writer();
    let receipt = store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &operation,
            &new_writer,
            100,
        )
        .unwrap();
    let empty = store.active(&temp.owner()).unwrap().unwrap();
    assert!(empty.id() != draft.id() && empty.content() == &Content::empty());
    let newer = store
        .save(&empty, &writer(), content("New user text"))
        .unwrap();
    let created = store
        .settle(
            &started,
            Outcome::Created {
                file_id: "FOLD".into(),
            },
        )
        .unwrap();
    assert!(store.active(&temp.owner()).unwrap() == Some(newer.clone()));
    let archived = store.draft(&temp.owner(), draft.id()).unwrap().unwrap();
    assert!(archived.archived && archived.content == draft.content);
    assert!(store.attempt(&temp.owner(), started.id()).unwrap() == Some(created));
    // Lost archive commit response recovers one decision, even after late result and editing.
    assert!(
        store
            .archive_and_new(
                &draft,
                &started,
                ArchiveReason::AbandonedOutcomeUnknown,
                &operation,
                &new_writer,
                100
            )
            .unwrap()
            == receipt
    );
    assert!(store.active(&temp.owner()).unwrap() == Some(newer));
    assert_eq!(count(&store.conn, "drafts").unwrap(), 2);
    assert_eq!(count(&store.conn, "decisions").unwrap(), 1);
    drop(store);
    let store = temp.store();
    assert!(store.archive_receipt(&temp.owner(), &operation).unwrap() == Some(receipt));
    assert_eq!(store.archived(&temp.owner()).unwrap().len(), 1);
}
#[test]
fn created_first_forces_new_explicit_review_before_archiving() {
    let temp = Temp::new();
    let mut store = temp.store();
    let initial = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &initial);
    let created = store
        .settle(
            &started,
            Outcome::Created {
                file_id: "F1".into(),
            },
        )
        .unwrap();
    assert!(matches!(
        store.archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &writer(),
            &writer(),
            100
        ),
        Err(Fault::Stale)
    ));
    assert!(store.active(&temp.owner()).unwrap() == Some(draft.clone()));
    assert!(matches!(
        store.archive_and_new(
            &draft,
            &created,
            ArchiveReason::AbandonedOutcomeUnknown,
            &writer(),
            &writer(),
            100
        ),
        Err(Fault::Blocked)
    ));
    store
        .archive_and_new(
            &draft,
            &created,
            ArchiveReason::CreatedReviewed,
            &writer(),
            &writer(),
            100,
        )
        .unwrap();
    assert!(store.attempt(&temp.owner(), started.id()).unwrap() == Some(created));
}
#[test]
fn archive_transaction_failure_rolls_back_every_association() {
    let temp = Temp::new();
    let mut store = temp.store();
    let initial = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &initial);
    store.conn.execute_batch("CREATE TEMP TRIGGER fail_decision BEFORE INSERT ON decisions BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let op = writer();
    let new_writer = writer();
    assert!(
        store
            .archive_and_new(
                &draft,
                &started,
                ArchiveReason::AbandonedOutcomeUnknown,
                &op,
                &new_writer,
                100
            )
            .is_err()
    );
    assert!(store.active(&temp.owner()).unwrap() == Some(draft.clone()));
    assert_eq!(count(&store.conn, "drafts").unwrap(), 1);
    assert_eq!(count(&store.conn, "decisions").unwrap(), 0);
    assert!(store.attempt(&temp.owner(), started.id()).unwrap() == Some(started.clone()));
    store
        .conn
        .execute_batch("DROP TRIGGER fail_decision;")
        .unwrap();
    assert!(
        store
            .archive_and_new(
                &draft,
                &started,
                ArchiveReason::AbandonedOutcomeUnknown,
                &op,
                &new_writer,
                100
            )
            .is_ok()
    );
}
#[test]
fn archive_operation_id_cannot_be_reused_for_different_body_or_decision() {
    let temp = Temp::new();
    let mut store = temp.store();
    let initial = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &initial);
    let op = writer();
    let new_writer = writer();
    store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &op,
            &new_writer,
            100,
        )
        .unwrap();
    let mut fake_draft = draft.clone();
    let mut fake_attempt = started.clone();
    fake_draft.content = content("Fake");
    fake_attempt.frozen = fake_draft.content.clone();
    assert!(matches!(
        store.archive_and_new(
            &fake_draft,
            &fake_attempt,
            ArchiveReason::AbandonedOutcomeUnknown,
            &op,
            &new_writer,
            100
        ),
        Err(Fault::Stale)
    ));
    assert!(matches!(
        store.archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &op,
            &writer(),
            100
        ),
        Err(Fault::Stale)
    ));
}
#[test]
fn settlement_uses_original_owner_and_does_not_require_active_credentials() {
    let temp = Temp::new();
    let mut store = temp.store();
    let initial = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &initial);
    let foreign = Owner::new(&temp.account, "T1", "U2").unwrap();
    let other = store
        .create(&foreign, &writer(), content("Other account user"))
        .unwrap();
    assert!(store.attempt(&foreign, started.id()).is_err());
    store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &writer(),
            &writer(),
            100,
        )
        .unwrap();
    store
        .settle(
            &started,
            Outcome::Created {
                file_id: "F1".into(),
            },
        )
        .unwrap();
    assert!(store.active(&foreign).unwrap() == Some(other));
    assert!(store.archived(&foreign).unwrap().is_empty());
}
#[test]
fn admitted_result_can_settle_at_full_attempt_capacity() {
    let temp = Temp::new();
    let mut store = temp.store();
    let mut draft = seed(&temp, &mut store);
    for _ in 0..MAX_ATTEMPTS - 1 {
        let reservation = store.reserve(&draft, 100).unwrap();
        store.cancel_reserved(&reservation).unwrap();
        draft = reservation.draft;
    }
    let (frozen, started) = start(&mut store, &draft);
    assert_eq!(count(&store.conn, "attempts").unwrap(), MAX_ATTEMPTS);
    let result = store
        .settle(&started, Outcome::RateLimited { retry_at: 100 })
        .unwrap();
    assert!(matches!(result.delivery, Delivery::RateLimited { .. }));
    assert!(matches!(store.reserve(&frozen, 100), Err(Fault::Capacity)));
    assert_eq!(count(&store.conn, "attempts").unwrap(), MAX_ATTEMPTS);
}
