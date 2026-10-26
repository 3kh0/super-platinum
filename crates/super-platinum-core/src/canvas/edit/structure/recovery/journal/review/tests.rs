use super::*;
use crate::canvas::edit::structure::{Side, StructureIntent};
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000992";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-local-review-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, ACCOUNT).unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn setup() -> (Store, CanvasCollabSnapshot, Receipt) {
    let store = Store::new();
    let snapshot = crate::canvas::edit::structure::tests::fixture();
    let prepared = StructureIntent::insert(
        &snapshot,
        "item",
        Side::After,
        &TextDraft::from_rtml("<b>Unconfirmed</b>").unwrap(),
        100,
    )
    .unwrap()
    .prepare(&snapshot, 100)
    .unwrap();
    let receipt = match store.open().begin(prepared.checkpoint().unwrap()).unwrap() {
        Begin::Started(receipt) => receipt,
        _ => panic!("fresh store"),
    };
    (store, snapshot, receipt)
}
fn archive_count(j: &Journal) -> i64 {
    j.conn
        .query_row("SELECT count(*) FROM local_reviews", [], |r| r.get(0))
        .unwrap()
}
fn remove(
    j: &mut Journal,
    review: &LocalReview,
    snapshot: &CanvasCollabSnapshot,
) -> Result<bool, AppError> {
    j.remove_local_lock(
        review,
        snapshot,
        LocalDecision::RemoveEditLockOutcomeUnknown,
        123,
    )
}

#[test]
fn viewing_absent_target_never_resolves_and_explicit_local_action_archives_unknown() {
    let (store, snapshot, receipt) = setup();
    assert!(
        snapshot
            .section(receipt.checkpoint.target_section())
            .is_none()
    );
    let mut j = store.open();
    let review = j.local_review(&receipt, &snapshot).unwrap();
    assert_eq!(review.draft_text(), "Unconfirmed");
    assert!(j.get(receipt.checkpoint.scope()).unwrap().as_ref() == Some(&receipt));
    assert_eq!(archive_count(&j), 0);
    assert!(remove(&mut j, &review, &snapshot).unwrap());
    assert!(j.get(receipt.checkpoint.scope()).unwrap().is_none());
    let (body, observed, decision): (String, String, String) = j
        .conn
        .query_row(
            "SELECT body,observed,decision FROM local_reviews",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(body, receipt.body);
    assert_eq!(decision, "local_unlock_outcome_unknown");
    let observed: serde_json::Value = serde_json::from_str(&observed).unwrap();
    assert_eq!(observed.as_object().unwrap().len(), 3);
    let raw: Vec<Vec<u8>> = serde_json::from_value(observed["sections"].clone()).unwrap();
    assert!(
        raw.iter()
            .any(|s| s.windows(4).any(|v| v == [0xc8, 0x3e, 0x81, 0x00]))
    );
    drop(j);
    let mut reopened = store.open();
    assert_eq!(archive_count(&reopened), 1);
    assert!(reopened.has_local_review(&receipt).unwrap());
    assert_eq!(
        reopened
            .local_review_count(receipt.checkpoint.scope())
            .unwrap(),
        1
    );
    let mut other_scope = receipt.checkpoint.scope().clone();
    other_scope.file = "other-file".into();
    assert_eq!(reopened.local_review_count(&other_scope).unwrap(), 0);
    assert!(matches!(
        reopened.begin(receipt.checkpoint.clone()).unwrap(),
        Begin::Started(_)
    ));
}

#[test]
fn review_refuses_changed_canonical_owner_and_full_raw_inventory() {
    let (store, snapshot, receipt) = setup();
    let mut j = store.open();
    let review = j.local_review(&receipt, &snapshot).unwrap();
    let mut file = snapshot.file.clone();
    file.id = Some("other-file".into());
    let mut other =
        CanvasCollabSnapshot::from_responses(file, snapshot.init.clone(), snapshot.load.clone())
            .unwrap();
    // The structural fixture specializes its raw inventory after construction.
    // Keep that admitted inventory so rejection isolates canonical file identity.
    other.thread = snapshot.thread.clone();
    other.document = snapshot.document.clone();
    other.sections = snapshot.sections.clone();
    assert_eq!(other.file_id(), "other-file");
    assert!(Inventory::capture(&other).is_ok());
    assert!(j.local_review(&receipt, &other).is_err());
    let mut changed = snapshot.clone();
    let section = &mut changed.sections[0];
    let mut bytes = section.raw().raw().to_vec();
    bytes.extend_from_slice(&[0xd0, 0x3e, 1]);
    *section = crate::canvas::wire::Section::decode(&bytes).unwrap();
    assert!(remove(&mut j, &review, &changed).is_err());
    assert!(j.get(receipt.checkpoint.scope()).unwrap().is_some());
    assert_eq!(archive_count(&j), 0);
}

#[test]
fn stale_review_cannot_remove_replacement_or_reencoded_row() {
    let (store, snapshot, receipt) = setup();
    let mut j = store.open();
    let review = j.local_review(&receipt, &snapshot).unwrap();
    j.conn
        .execute("UPDATE receipts SET body=body || ' '", [])
        .unwrap();
    assert!(!remove(&mut j, &review, &snapshot).unwrap());
    assert_eq!(archive_count(&j), 0);
    let changed = j.get(receipt.checkpoint.scope()).unwrap().unwrap();
    assert!(j.complete(&changed).unwrap());
    assert!(matches!(
        j.begin(receipt.checkpoint.clone()).unwrap(),
        Begin::Started(_)
    ));
    assert!(!remove(&mut j, &review, &snapshot).unwrap());
}

#[test]
fn failed_archive_or_delete_commit_keeps_original_active_row() {
    let (store, snapshot, receipt) = setup();
    let mut j = store.open();
    let review = j.local_review(&receipt, &snapshot).unwrap();
    j.conn.execute_batch("CREATE TRIGGER refuse_local_delete BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'held deletion'); END;").unwrap();
    assert!(remove(&mut j, &review, &snapshot).is_err());
    assert_eq!(archive_count(&j), 0);
    assert!(j.get(receipt.checkpoint.scope()).unwrap().as_ref() == Some(&receipt));
    drop(j);
    assert!(
        store
            .open()
            .get(receipt.checkpoint.scope())
            .unwrap()
            .as_ref()
            == Some(&receipt)
    );
}

#[test]
fn failed_deferred_commit_rolls_back_archive_and_active_removal_together() {
    let (store, snapshot, receipt) = setup();
    let mut j = store.open();
    let review = j.local_review(&receipt, &snapshot).unwrap();
    j.conn.execute_batch("PRAGMA foreign_keys=ON;
      CREATE TABLE review_commit_parent(id INTEGER PRIMARY KEY);
      CREATE TABLE review_commit_child(parent INTEGER REFERENCES review_commit_parent(id) DEFERRABLE INITIALLY DEFERRED);
      CREATE TRIGGER fail_review_commit AFTER INSERT ON local_reviews BEGIN INSERT INTO review_commit_child VALUES(1); END;").unwrap();
    assert!(remove(&mut j, &review, &snapshot).is_err());
    assert_eq!(archive_count(&j), 0);
    assert!(j.get(receipt.checkpoint.scope()).unwrap().as_ref() == Some(&receipt));
    drop(j);
    let reopened = store.open();
    assert_eq!(archive_count(&reopened), 0);
    assert!(reopened.get(receipt.checkpoint.scope()).unwrap().as_ref() == Some(&receipt));
}

#[test]
fn archive_capacity_never_prunes_or_unblocks() {
    assert!(archive_admits(31, MAX_REVIEWED_BYTES as i64 - 1, 1));
    for (count, bytes, size) in [
        (32, 0, 1),
        (-1, 0, 1),
        (0, -1, 1),
        (0, MAX_REVIEWED_BYTES as i64, 1),
        (0, 1, usize::MAX),
    ] {
        assert!(!archive_admits(count, bytes, size));
    }
    let (store, snapshot, receipt) = setup();
    let mut j = store.open();
    let review = j.local_review(&receipt, &snapshot).unwrap();
    for n in 0..32 {
        j.conn.execute("INSERT INTO local_reviews VALUES('T','U','F',?1,'{}','{}',1,'local_unlock_outcome_unknown')", [n.to_string()]).unwrap();
    }
    assert!(remove(&mut j, &review, &snapshot).is_err());
    assert_eq!(archive_count(&j), 32);
    assert!(j.get(receipt.checkpoint.scope()).unwrap().is_some());
}

#[test]
fn version_one_migration_preserves_exact_active_body_and_account() {
    let (store, snapshot, receipt) = setup();
    {
        let j = store.open();
        j.conn
            .execute_batch("DROP TABLE local_reviews; PRAGMA user_version=1;")
            .unwrap();
    }
    let j = store.open();
    assert_eq!(
        j.conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert!(j.get(receipt.checkpoint.scope()).unwrap().as_ref() == Some(&receipt));
    assert_eq!(archive_count(&j), 0);
    assert!(j.local_review(&receipt, &snapshot).is_ok());
    assert!(Journal::open(&store.0, "00000000-0000-4000-8000-000000000993").is_err());
}

#[test]
fn concurrent_human_decisions_archive_once_and_old_review_cannot_cross_account() {
    let (store, snapshot, receipt) = setup();
    let barrier = std::sync::Barrier::new(2);
    let values = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let mut j = store.open();
                    let review = j.local_review(&receipt, &snapshot).unwrap();
                    barrier.wait();
                    remove(&mut j, &review, &snapshot).unwrap()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(values.into_iter().filter(|done| *done).count(), 1);
    assert_eq!(archive_count(&store.open()), 1);
    let (second, snapshot, receipt) = setup();
    let review = second.open().local_review(&receipt, &snapshot).unwrap();
    let other_store = Store::new();
    let mut other = Journal::open(&other_store.0, "00000000-0000-4000-8000-000000000993").unwrap();
    assert!(remove(&mut other, &review, &snapshot).is_err());
    assert!(
        second
            .open()
            .get(receipt.checkpoint.scope())
            .unwrap()
            .is_some()
    );
}
