use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000786";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("canvas-end-{}.sqlite", uuid::Uuid::new_v4())))
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
fn started(
    store: &Store,
    c: Checkpoint,
) -> crate::canvas::edit::structure::recovery::journal::Receipt {
    match store.open().begin(c).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("fresh store"),
    }
}

#[test]
fn end_v4_sqlite_restart_review_preserves_whole_pair_and_is_inert() {
    for mask in 0..4 {
        let s = fixture("Café &amp; 日本😀", true);
        let p = prepare(&s);
        let store = Store::new();
        let checkpoint = p.checkpoint().unwrap();
        let value = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(value["version"], 4);
        assert_eq!(value["action"]["End"]["offset_utf16"], 11);
        assert_eq!(value["draft"], "Café &amp; 日本😀");
        let decoded: Checkpoint = serde_json::from_value(value).unwrap();
        assert!(decoded == checkpoint);
        let receipt = started(&store, decoded);
        let mut journal = store.open();
        let restored = journal.get(checkpoint.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        assert!(matches!(
            journal.begin(checkpoint.clone()).unwrap(),
            Begin::Blocked(_)
        ));
        let mut observed = readback(&s, &pair(&p, &s));
        observed.sections.retain(|r| {
            (r.id() != p.target_section() || mask & 1 != 0)
                && (r.id() != p.child_section() || mask & 2 != 0)
        });
        let review = journal.local_review(&restored, &observed).unwrap();
        assert!(
            review.is_end()
                && !review.is_split()
                && !review.is_replacement()
                && !review.is_delete()
        );
        let end = review.end().unwrap();
        assert_eq!(end.parent_id(), "item");
        assert_eq!(end.child_id(), p.child_section());
        assert_eq!(end.original_text(), "Café & 日本😀");
        assert_eq!(end.parent_text(), end.original_text());
        assert_eq!(end.child_text(), "");
        assert_eq!(journal.local_review_count(checkpoint.scope()).unwrap(), 0);
        assert!(journal.get(checkpoint.scope()).unwrap().is_some());
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &observed,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    123
                )
                .unwrap()
        );
        assert!(journal.get(checkpoint.scope()).unwrap().is_none());
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        let (stored, evidence, decision): (String, String, String) = conn
            .query_row(
                "SELECT body,observed,decision FROM local_reviews",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let archive: Checkpoint = serde_json::from_str(&stored).unwrap();
        assert!(archive == checkpoint);
        assert_eq!(decision, "local_unlock_outcome_unknown");
        let evidence: serde_json::Value = serde_json::from_str(&evidence).unwrap();
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(evidence["thread_raw"].clone()).unwrap(),
            observed.thread.raw()
        );
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(evidence["document_raw"].clone()).unwrap(),
            observed.document.raw()
        );
        assert_eq!(
            evidence["sections"].as_array().unwrap().len(),
            observed.sections.len()
        );
        assert!(store.open().has_local_review(&receipt).unwrap());
    }
}

#[test]
fn end_checkpoint_corrupt_pair_source_history_offset_or_title_cannot_reserve() {
    let s = fixture("Café &amp; 日本😀", true);
    let p = prepare(&s);
    let c = p.checkpoint().unwrap();
    let store = Store::new();
    let value = serde_json::to_value(&c).unwrap();
    for kind in 0..13 {
        let mut bad = value.clone();
        match kind {
            0 => bad["version"] = 3.into(),
            1 => bad["action"]["End"]["offset_utf16"] = 15.into(),
            2 => bad["draft"] = "changed".into(),
            3 => bad["action"]["End"]["child"] = "item".into(),
            4 => bad["action"]["End"]["position"] = "zzz:temp".into(),
            _ => {
                let mut data = p.data.clone();
                let mut rows = data.messages(1).unwrap();
                match kind {
                    5 => {
                        rows.pop();
                    }
                    6 => rows.reverse(),
                    7 => rows[1] = rows[0].clone(),
                    8 => {
                        rows[0] = rows[0]
                            .replace_fields(12, &[m(12, &body("Café &amp; 日本😀", true))])
                            .unwrap()
                    }
                    9 => {
                        let wire = rows[0].message(38).unwrap().unwrap();
                        let head = wire.string(8).unwrap().unwrap();
                        rows[0] = rows[0]
                            .replace_fields(
                                38,
                                &[m(38, &wire.replace_fields(12, &[t(12, head)]).unwrap())],
                            )
                            .unwrap();
                    }
                    10 => {
                        rows[1] = rows[1]
                            .replace_fields(12, &[m(12, &body("tail", false))])
                            .unwrap()
                    }
                    11 => rows[0] = rows[0].replace_fields(33, &[t(33, "foreign-0")]).unwrap(),
                    _ => {
                        data = data
                            .replace_fields(14, &[m(14, &msg(&[t(1, "wrong")]))])
                            .unwrap()
                    }
                }
                data = data
                    .replace_fields(1, &rows.iter().map(|r| m(1, r)).collect::<Vec<_>>())
                    .unwrap();
                bad["request"] = serde_json::to_value(data.raw()).unwrap();
            }
        }
        let decoded: Checkpoint = serde_json::from_value(bad).unwrap();
        assert!(store.open().begin(decoded).is_err(), "corruption {kind}");
        assert!(store.open().get(c.scope()).unwrap().is_none());
    }
}

#[test]
fn failed_end_review_retirement_and_stale_review_preserve_active_receipt() {
    let s = fixture("Keep this", true);
    let p = prepare(&s);
    let store = Store::new();
    let receipt = started(&store, p.checkpoint().unwrap());
    let mut journal = store.open();
    let review = journal.local_review(&receipt, &s).unwrap();
    let mut changed = s.clone();
    changed.thread = changed.thread.replace_fields(2, &[n(2, 30000)]).unwrap();
    assert!(
        journal
            .remove_local_lock(
                &review,
                &changed,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .is_err()
    );
    let conn = rusqlite::Connection::open(&store.0).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_end_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END;").unwrap();
    assert!(
        journal
            .remove_local_lock(
                &review,
                &s,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .is_err()
    );
    assert_eq!(
        journal
            .local_review_count(receipt.checkpoint().scope())
            .unwrap(),
        0
    );
    assert!(
        store
            .open()
            .get(receipt.checkpoint().scope())
            .unwrap()
            .as_ref()
            == Some(&receipt)
    );
}

#[test]
fn end_v4_preserves_existing_insert_replace_and_middle_split_versions() {
    let s = fixture("Keep this", false);
    let store = Store::new();
    let insert = StructureIntent::insert(&s, "item", Side::After, &TextDraft::default(), 100)
        .unwrap()
        .prepare(&s, 100)
        .unwrap()
        .checkpoint()
        .unwrap();
    let mut sole = s.clone();
    sole.sections.retain(|r| r.id() != "other");
    let replace = ReplacementIntent::main_canvas(&sole, "item", 100)
        .unwrap()
        .prepare(&sole, 100)
        .unwrap()
        .checkpoint()
        .unwrap();
    let middle = crate::canvas::edit::structure::split::tests::fixture();
    let split = crate::canvas::edit::structure::split::tests::prepare(&middle)
        .checkpoint()
        .unwrap();
    let end = prepare(&s).checkpoint().unwrap();
    for (version, c) in [(1, insert), (2, replace), (3, split), (4, end)] {
        let value = serde_json::to_value(&c).unwrap();
        assert_eq!(value["version"], version);
        let decoded: Checkpoint = serde_json::from_value(value).unwrap();
        assert!(decoded == c);
        let receipt = started(&store, decoded);
        assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&receipt));
        assert!(store.open().complete(&receipt).unwrap()); // Definitely never dispatched.
    }
}
