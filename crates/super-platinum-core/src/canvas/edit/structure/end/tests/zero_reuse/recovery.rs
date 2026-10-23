use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(
            std::env::temp_dir().join(format!("canvas-zero-reuse-{}.sqlite", uuid::Uuid::new_v4())),
        )
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000798").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn zero_reuse_v5_v6_restart_and_unknown_outcome_review_keep_the_same_pair() {
    for updates in [
        &[][..],
        &["New", "New draftx", "New draft", "New paragraph"][..],
        &["x", ""][..],
    ] {
        for mask in 0..4 {
            let (s, id) = source();
            let p = attempt(&s, updates);
            let c = p.checkpoint().unwrap();
            assert!(c.is_end() && c.is_end_reuse());
            let json = serde_json::to_value(&c).unwrap();
            assert_eq!(json["version"], if updates.is_empty() { 5 } else { 6 });
            let decoded: Checkpoint = serde_json::from_value(json).unwrap();
            assert!(decoded == c);
            let store = Store::new();
            let receipt = match store.open().begin(decoded).unwrap() {
                Begin::Started(r) => r,
                _ => panic!("fresh"),
            };
            let mut journal = store.open();
            let restored = journal.get(c.scope()).unwrap().unwrap();
            assert!(restored == receipt);
            assert!(matches!(
                journal.begin(c.clone()).unwrap(),
                Begin::Blocked(_)
            ));
            let mut observed = readback(&s, &pair(&p, &s));
            observed
                .sections
                .retain(|r| (r.id() != "item" || mask & 1 != 0) && (r.id() != id || mask & 2 != 0));
            let review = journal.local_review(&restored, &observed).unwrap();
            let end = review.end().unwrap();
            assert!(end.reuses_child());
            assert_eq!(end.child_id(), id);
            assert_eq!(end.child_text(), updates.last().copied().unwrap_or(""));
            let conn = rusqlite::Connection::open(&store.0).unwrap();
            conn.execute_batch("CREATE TRIGGER reject_zero_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
            assert!(
                journal
                    .remove_local_lock(
                        &review,
                        &observed,
                        LocalDecision::RemoveEditLockOutcomeUnknown,
                        2_000_000
                    )
                    .is_err()
            );
            assert_eq!(journal.local_review_count(c.scope()).unwrap(), 0);
            assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&restored));
            conn.execute_batch("DROP TRIGGER reject_zero_retirement;")
                .unwrap();
            // Review is inert and still possible after source leases expired.
            assert!(
                journal
                    .remove_local_lock(
                        &review,
                        &observed,
                        LocalDecision::RemoveEditLockOutcomeUnknown,
                        2_000_000
                    )
                    .unwrap()
            );
            let archived: String = conn
                .query_row("SELECT body FROM local_reviews", [], |r| r.get(0))
                .unwrap();
            assert!(serde_json::from_str::<Checkpoint>(&archived).unwrap() == c);
            assert!(store.open().get(c.scope()).unwrap().is_none());
            assert!(store.open().has_local_review(&restored).unwrap());
        }
    }
}

#[test]
fn zero_reuse_recovery_rejects_source_unbound_zero_and_action_confusion() {
    for updates in [&[][..], &["New", "New paragraph"][..], &["x", ""][..]] {
        let (s, id) = source();
        let p = attempt(&s, updates);
        let c = p.checkpoint().unwrap();
        let original = serde_json::to_value(&c).unwrap();
        let store = Store::new();
        for kind in 0..6 {
            let mut bad = original.clone();
            match kind {
                0 => {
                    // Claim a positive-base source but keep zero outgoing. The
                    // typed validator must derive permission from the source.
                    let rows = bad["sections"].as_array_mut().unwrap();
                    for value in rows {
                        let raw: Vec<u8> = serde_json::from_value(value.clone()).unwrap();
                        let section = Section::decode(&raw).unwrap();
                        if section.id() == id {
                            let wire = section
                                .crdt()
                                .unwrap()
                                .unwrap()
                                .replace_fields(2, &[n(2, 1000)])
                                .unwrap();
                            *value =
                                serde_json::to_value(change(&section, m(38, &wire)).raw().raw())
                                    .unwrap();
                        }
                    }
                }
                1 => bad["sections"].as_array_mut().unwrap().retain(|v| {
                    let raw: Vec<u8> = serde_json::from_value(v.clone()).unwrap();
                    Section::decode(&raw).unwrap().id() != id
                }),
                2 => {
                    if updates.is_empty() {
                        bad["version"] = 4.into();
                        let action = bad["action"]["EndReuse"].clone();
                        bad["action"] = serde_json::json!({"End": action});
                    } else {
                        bad["action"]["EndTyping"]["reused"] = false.into();
                    }
                }
                3 => bad["version"] = 1.into(),
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    if kind == 4 {
                        rows.pop();
                    } else {
                        let wire = p.child_wire.replace_fields(2, &[n(2, 1000)]).unwrap();
                        rows[1] = rows[1].replace_fields(38, &[m(38, &wire)]).unwrap();
                    }
                    let data = p
                        .data
                        .replace_fields(1, &rows.iter().map(|r| m(1, r)).collect::<Vec<_>>())
                        .unwrap();
                    bad["request"] = serde_json::to_value(data.raw()).unwrap();
                }
            }
            let bad: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                store.open().begin(bad).is_err(),
                "typed={},corruption={kind}",
                !updates.is_empty()
            );
            assert!(store.open().get(c.scope()).unwrap().is_none());
        }
    }
}
