use super::journal::{Store, started};
use super::*;
use crate::canvas::edit::structure::recovery::journal::LocalDecision;

#[test]
fn actual_main_canvas_constructor_is_bounded_and_one_shared_session() {
    let snapshot = fixture();
    assert!(can_replace_main_canvas(&snapshot, "item", 100));
    assert!(!can_replace_main_canvas(
        &super::super::super::tests::fixture(),
        "item",
        100
    ));
    let intent = ReplacementIntent::main_canvas(&snapshot, "item", 100).unwrap();
    let id = intent.replacement_section().to_owned();
    let prepared = intent.prepare(&snapshot, 100).unwrap();
    assert_eq!(prepared.target_section(), "item");
    assert_eq!(prepared.replacement_section(), id);
    let updates = prepared.document_data().messages(1).unwrap();
    assert_eq!(updates.len(), 2);
    for update in updates {
        assert_eq!(
            update.string(33).unwrap(),
            Some(format!("{}-0", prepared.session_id()).as_str())
        );
    }
}

#[test]
fn local_review_of_partial_pair_preserves_both_effects_unknown_and_full_observation() {
    let snapshot = fixture();
    let prepared = prepared(&snapshot);
    let checkpoint = prepared.checkpoint().unwrap();
    let pair = pair(&prepared, &snapshot);
    // A deleted old row alone is a plausible partial observation, not acceptance.
    let mut partial = snapshot.clone();
    *partial
        .sections
        .iter_mut()
        .find(|s| s.id() == "item")
        .unwrap() = pair[0].clone();
    assert!(Inventory::capture(&partial).is_err());
    let store = Store::new();
    let mut journal = store.open();
    let receipt = started(&mut journal, checkpoint.clone());
    let review = journal.local_review(&receipt, &partial).unwrap();
    assert!(review.is_replacement());
    assert!(!review.is_delete());
    let mut different = partial.clone();
    different.sections[0] = changed(&different.sections[0], n(1002, 17));
    assert!(
        journal
            .remove_local_lock(
                &review,
                &different,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                100
            )
            .is_err()
    );
    assert!(journal.get(checkpoint.scope()).unwrap().unwrap() == receipt);
    assert!(
        journal
            .remove_local_lock(
                &review,
                &partial,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                100
            )
            .unwrap()
    );
    assert!(journal.has_local_review(&receipt).unwrap());
    assert!(journal.get(checkpoint.scope()).unwrap().is_none());
    assert!(
        !journal
            .remove_local_lock(
                &review,
                &partial,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                101
            )
            .unwrap()
    );
    drop(journal);
    let reopened = store.open();
    assert!(reopened.has_local_review(&receipt).unwrap());
    assert_eq!(reopened.local_review_count(checkpoint.scope()).unwrap(), 1);
}

#[test]
fn pair_archive_failed_commit_keeps_exact_active_body_and_no_partial_archive() {
    let snapshot = fixture();
    let checkpoint = prepared(&snapshot).checkpoint().unwrap();
    let store = Store::new();
    let mut journal = store.open();
    let receipt = started(&mut journal, checkpoint.clone());
    let review = journal.local_review(&receipt, &snapshot).unwrap();
    let db = rusqlite::Connection::open(&store.0).unwrap();
    db.execute_batch("CREATE TRIGGER refuse_pair_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(
        journal
            .remove_local_lock(
                &review,
                &snapshot,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                100
            )
            .is_err()
    );
    assert!(journal.get(checkpoint.scope()).unwrap().unwrap() == receipt);
    assert!(!journal.has_local_review(&receipt).unwrap());
    assert_eq!(journal.local_review_count(checkpoint.scope()).unwrap(), 0);
    drop(db);
    drop(journal);
    assert!(store.open().get(checkpoint.scope()).unwrap().unwrap() == receipt);
}
