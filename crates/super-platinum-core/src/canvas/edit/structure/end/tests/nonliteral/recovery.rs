use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("canvas-clearing-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000799").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn clearing_v7_restart_review_keeps_original_invisible_content_and_atomic_retirement() {
    for shape in ["   ", "\u{a0}", "<br>"] {
        for zero in [false, true] {
            for updates in [&[][..], &["x"][..], &["x", ""][..]] {
                let (s, id) = source(shape, zero);
                let p = attempt(&s, updates);
                let c = p.checkpoint().unwrap();
                let value = serde_json::to_value(&c).unwrap();
                assert_eq!(value["version"], 7);
                assert_eq!(
                    value["action"]["EndClearing"]["child_updates"]
                        .as_array()
                        .unwrap()
                        .len(),
                    updates.len()
                );
                let decoded: Checkpoint = serde_json::from_value(value).unwrap();
                assert!(decoded == c);
                for mask in 0..4 {
                    let store = Store::new();
                    let receipt = match store.open().begin(decoded.clone()).unwrap() {
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
                    observed.sections.retain(|r| {
                        (r.id() != "item" || mask & 1 != 0) && (r.id() != id || mask & 2 != 0)
                    });
                    let review = journal.local_review(&restored, &observed).unwrap();
                    let end = review.end().unwrap();
                    assert!(end.clears_child() && end.reuses_child());
                    assert_eq!(end.original_child_rtml(), Some(shape));
                    assert_eq!(end.child_id(), id);
                    assert_eq!(end.child_text(), updates.last().copied().unwrap_or(""));
                    let conn = rusqlite::Connection::open(&store.0).unwrap();
                    conn.execute_batch("CREATE TRIGGER reject_clear_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
                    conn.execute_batch("DROP TRIGGER reject_clear_retirement;")
                        .unwrap();
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
                }
            }
        }
    }
}

#[test]
fn clearing_recovery_rejects_old_action_reinterpretation_and_missing_source_or_report() {
    let (s, id) = source("<br>", true);
    for updates in [&[][..], &["x"][..], &["x", ""][..]] {
        let p = attempt(&s, updates);
        let c = p.checkpoint().unwrap();
        let original = serde_json::to_value(&c).unwrap();
        for kind in 0..7 {
            let mut bad = original.clone();
            match kind {
                0 => bad["version"] = 6.into(),
                1 => {
                    let mut action = bad["action"]["EndClearing"].clone();
                    action["reused"] = true.into();
                    bad["action"] = serde_json::json!({"EndTyping":action});
                    bad["version"] = 6.into();
                }
                2 => {
                    let mut action = bad["action"]["EndClearing"].clone();
                    action.as_object_mut().unwrap().remove("child_updates");
                    bad["action"] = serde_json::json!({"EndReuse":action});
                    bad["version"] = 5.into();
                }
                3 => bad["sections"].as_array_mut().unwrap().retain(|v| {
                    let raw: Vec<u8> = serde_json::from_value(v.clone()).unwrap();
                    Section::decode(&raw).unwrap().id() != id
                }),
                4 => bad["action"]["EndClearing"]["child_updates"] = serde_json::json!([""]),
                5 => {
                    let mut rows = p.data.messages(1).unwrap();
                    rows.pop();
                    let data = p
                        .data
                        .replace_fields(1, &rows.iter().map(|r| m(1, r)).collect::<Vec<_>>())
                        .unwrap();
                    bad["request"] = serde_json::to_value(data.raw()).unwrap();
                }
                _ => {
                    bad["action"]["EndClearing"]["child_updates"] = serde_json::json!(["Changed"]);
                }
            }
            let bad: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(Store::new().open().begin(bad).is_err(), "corruption={kind}");
        }
    }
}
