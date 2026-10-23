use super::*;
use crate::canvas::edit::structure::{PreparedSplitEdit, split::tests as source};
use crate::canvas::wire::{EncodedField as F, RawMessage, Section};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000784";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-split-review-{}.sqlite",
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
fn setup() -> (Store, CanvasCollabSnapshot, PreparedSplitEdit, Receipt) {
    let snapshot = source::fixture();
    let prepared = source::prepare(&snapshot);
    let store = Store::new();
    let receipt = match store.open().begin(prepared.checkpoint().unwrap()).unwrap() {
        Begin::Started(receipt) => receipt,
        _ => panic!("fresh store"),
    };
    (store, snapshot, prepared, receipt)
}
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn t(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn change(section: &Section, field: F) -> Section {
    Section::from_raw(
        section
            .raw()
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn observe(
    snapshot: &CanvasCollabSnapshot,
    prepared: &PreparedSplitEdit,
    mask: u8,
) -> CanvasCollabSnapshot {
    let mut seen = snapshot.clone();
    seen.sections
        .retain(|s| s.id() != prepared.target_section());
    for (index, update) in prepared
        .document_data()
        .messages(1)
        .unwrap()
        .into_iter()
        .enumerate()
    {
        if mask & (1 << index) == 0 {
            continue;
        }
        let update = Section::from_raw(update).unwrap();
        let mut section = snapshot
            .section(update.id())
            .map_or_else(|| update.clone(), |s| s.merge_partial(&update).unwrap());
        let position = section
            .raw()
            .string(8)
            .unwrap()
            .unwrap()
            .trim_end_matches(":temp")
            .to_owned();
        for field in [
            n(2, 20000),
            t(6, snapshot.thread_id()),
            t(7, snapshot.document_id()),
            t(8, &position),
            t(21, &position),
        ] {
            section = change(&section, field);
        }
        // Authored canonical observation, not an ACK-derived acceptance token.
        let history = section
            .crdt()
            .unwrap()
            .unwrap()
            .remove(12)
            .unwrap()
            .replace_fields(3, &[n(3, 20000)])
            .unwrap()
            .replace_fields(4, &[n(4, 1_000_000)])
            .unwrap();
        section = change(&section, F::message(38, &history).unwrap());
        seen.sections.push(section);
    }
    seen
}
fn remove(
    j: &mut Journal,
    review: &LocalReview,
    current: &CanvasCollabSnapshot,
) -> Result<bool, AppError> {
    j.remove_local_lock(
        review,
        current,
        LocalDecision::RemoveEditLockOutcomeUnknown,
        123,
    )
}
fn assert_untouched(j: &Journal, receipt: &Receipt) {
    assert!(j.get(receipt.checkpoint.scope()).unwrap().as_ref() == Some(receipt));
    assert!(!j.has_local_review(receipt).unwrap());
    assert_eq!(j.local_review_count(receipt.checkpoint.scope()).unwrap(), 0);
}

#[test]
fn reopened_pair_review_and_cancel_are_read_only_and_present_both_halves() {
    let (store, snapshot, prepared, receipt) = setup();
    let mut j = store.open();
    let restored = j.get(receipt.checkpoint.scope()).unwrap().unwrap();
    assert!(restored == receipt);
    let review = j.local_review(&restored, &snapshot).unwrap();
    assert!(review.is_split());
    assert!(!review.is_replacement() && !review.is_delete());
    let pair = review.split().unwrap();
    assert_eq!(pair.parent_id(), prepared.target_section());
    assert_eq!(pair.child_id(), prepared.child_section());
    assert_eq!(pair.original_text(), "Keep thisSecond paragraph");
    assert_eq!(pair.parent_text(), "Keep this");
    assert_eq!(pair.child_text(), "Second paragraph");
    assert_eq!(review.observed_sections().count(), snapshot.sections.len());
    let positions = review
        .observed_sections()
        .map(|s| s.raw().string(8).unwrap().unwrap())
        .collect::<Vec<_>>();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    drop(review); // Cancel does not mutate the journal.
    assert_untouched(&j, &receipt);
    assert!(matches!(
        j.begin(prepared.checkpoint().unwrap()).unwrap(),
        Begin::Blocked(_)
    ));
    drop(j);
    assert_untouched(&store.open(), &receipt);
}

#[test]
fn zero_one_or_both_observed_members_archive_whole_pair_as_unknown_only() {
    for mask in 0..4 {
        let (store, snapshot, prepared, receipt) = setup();
        let observed = observe(&snapshot, &prepared, mask);
        let mut j = store.open();
        let review = j.local_review(&receipt, &observed).unwrap();
        assert_eq!(review.observed_sections().count(), observed.sections.len());
        assert_untouched(&j, &receipt);
        assert!(remove(&mut j, &review, &observed).unwrap());
        assert!(j.get(receipt.checkpoint.scope()).unwrap().is_none());
        let (body, observed_json, decision): (String, String, String) = j
            .conn
            .query_row(
                "SELECT body,observed,decision FROM local_reviews WHERE operation=?1",
                [&receipt.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(body, receipt.body);
        assert_eq!(decision, "local_unlock_outcome_unknown");
        let evidence: serde_json::Value = serde_json::from_str(&observed_json).unwrap();
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(evidence["thread_raw"].clone()).unwrap(),
            observed.thread.raw()
        );
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(evidence["document_raw"].clone()).unwrap(),
            observed.document.raw()
        );
        let bytes: Vec<Vec<u8>> = serde_json::from_value(evidence["sections"].clone()).unwrap();
        assert_eq!(bytes.len(), observed.sections.len());
        for bytes in bytes {
            let section = Section::decode(&bytes).unwrap();
            assert_eq!(observed.section(section.id()).unwrap().raw().raw(), bytes);
        }
        assert!(!remove(&mut j, &review, &observed).unwrap());
        drop(j);
        let reopened = store.open();
        assert!(reopened.has_local_review(&receipt).unwrap());
        assert_eq!(
            reopened
                .local_review_count(receipt.checkpoint.scope())
                .unwrap(),
            1
        );
        assert!(reopened.local_review(&receipt, &observed).is_err());
    }
}

#[test]
fn tombstoned_pair_member_is_visible_review_evidence_not_a_success_signal() {
    let (store, snapshot, prepared, receipt) = setup();
    let mut observed = observe(&snapshot, &prepared, 3);
    let parent = observed
        .sections
        .iter_mut()
        .find(|s| s.id() == prepared.target_section())
        .unwrap();
    *parent = change(parent, n(3, 1));
    let j = store.open();
    let review = j.local_review(&receipt, &observed).unwrap();
    assert!(
        review
            .observed_sections()
            .any(|s| s.id() == prepared.target_section() && s.deleted().unwrap() == Some(true))
    );
    assert_untouched(&j, &receipt);
}

#[test]
fn changed_full_inventory_thread_document_and_account_cannot_use_old_review() {
    let (store, snapshot, prepared, receipt) = setup();
    let current = observe(&snapshot, &prepared, 3);
    let mut j = store.open();
    let review = j.local_review(&receipt, &current).unwrap();
    for kind in 0..4 {
        let mut changed = current.clone();
        match kind {
            0 => changed.sections[0] = change(&changed.sections[0], n(1002, 17)),
            1 => changed.thread = changed.thread.replace_fields(1002, &[n(1002, 17)]).unwrap(),
            2 => {
                changed.document = changed
                    .document
                    .replace_fields(1002, &[n(1002, 17)])
                    .unwrap()
            }
            _ => {
                changed
                    .sections
                    .retain(|s| s.id() != prepared.child_section());
            }
        }
        assert!(
            remove(&mut j, &review, &changed).is_err(),
            "stale input {kind}"
        );
        assert_untouched(&j, &receipt);
    }
    let other = Store::new();
    let mut other_account =
        Journal::open(&other.0, "00000000-0000-4000-8000-000000000785").unwrap();
    assert!(remove(&mut other_account, &review, &current).is_err());
    assert_untouched(&j, &receipt);
}

#[test]
fn exact_row_reencoding_or_replacement_cannot_be_archived_by_old_pair_review() {
    let (store, snapshot, prepared, receipt) = setup();
    let mut j = store.open();
    let review = j.local_review(&receipt, &snapshot).unwrap();
    j.conn
        .execute("UPDATE receipts SET body=body || ' '", [])
        .unwrap();
    assert!(!remove(&mut j, &review, &snapshot).unwrap());
    assert_eq!(j.local_review_count(receipt.checkpoint.scope()).unwrap(), 0);
    let changed = j.get(receipt.checkpoint.scope()).unwrap().unwrap();
    // Fixture never dispatched; exact non-dispatch removal makes a fresh row.
    assert!(j.complete(&changed).unwrap());
    let replacement = match j.begin(prepared.checkpoint().unwrap()).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("new row"),
    };
    assert!(!remove(&mut j, &review, &snapshot).unwrap());
    assert_untouched(&j, &replacement);
}

#[test]
fn archive_insert_delete_and_deferred_commit_failures_leave_exact_pair_active() {
    for mode in 0..3 {
        let (store, snapshot, _, receipt) = setup();
        let mut j = store.open();
        let review = j.local_review(&receipt, &snapshot).unwrap();
        let fail = match mode {
            0 => {
                "CREATE TRIGGER refuse_archive BEFORE INSERT ON local_reviews BEGIN SELECT RAISE(ABORT,'archive failure'); END;"
            }
            1 => {
                "CREATE TRIGGER refuse_remove BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'delete failure'); END;"
            }
            _ => {
                "PRAGMA foreign_keys=ON; CREATE TABLE split_commit_parent(id INTEGER PRIMARY KEY); CREATE TABLE split_commit_child(parent INTEGER REFERENCES split_commit_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER refuse_commit AFTER INSERT ON local_reviews BEGIN INSERT INTO split_commit_child VALUES(1); END;"
            }
        };
        j.conn.execute_batch(fail).unwrap();
        assert!(remove(&mut j, &review, &snapshot).is_err());
        assert_untouched(&j, &receipt);
        drop(j);
        assert_untouched(&store.open(), &receipt);
    }
}
