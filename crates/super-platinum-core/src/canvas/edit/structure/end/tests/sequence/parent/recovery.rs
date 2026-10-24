use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-parent-sequence-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000815").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn parent_sequence_v15_restart_review_rollback_and_archive_preserve_original_intended_and_all_reports()
 {
    let e = evidence();
    for index in [0, 2, 4, 6, 18, 30, 46] {
        let (source, p) = attempt(&e["vectors"][index]);
        let checkpoint = p.checkpoint().unwrap();
        let bytes = serde_json::to_string(&checkpoint).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
        assert!(decoded == checkpoint);
        let store = Store::new();
        let receipt = match store.open().begin(decoded).unwrap() {
            Begin::Started(r) => r,
            _ => panic!("fresh"),
        };
        let mut journal = store.open();
        let restored = journal.get(checkpoint.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        assert!(matches!(
            journal.begin(checkpoint.clone()).unwrap(),
            Begin::Blocked(_)
        ));
        let current = replace(
            &source,
            p.target_section(),
            m(12, &body("Current remote content", true)),
        );
        let review = journal.local_review(&restored, &current).unwrap();
        let end = review.end().unwrap();
        assert!(end.is_sequence());
        assert_eq!(end.original_text(), split::plain(&p.intent.seed).unwrap());
        assert_eq!(
            end.parent_text(),
            p.parent_draft()
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>()
        );
        assert_eq!(end.parent_report_count(), p.parent_updates().len());
        assert_eq!(
            end.parent_updates(),
            p.parent_updates()
                .iter()
                .map(|d| d.rtml().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(end.appended_members().len(), 3);
        for (view, member) in end.appended_members().zip(p.appended_members()) {
            assert_eq!(view.id(), member.id());
            assert_eq!(view.intended_rtml(), member.draft().rtml().unwrap());
            assert_eq!(view.report_count(), member.updates().len());
        }
        assert!(
            text::canonical_draft(
                review
                    .observed_sections()
                    .find(|s| s.id() == p.target_section())
                    .unwrap()
            )
            .unwrap()
                == draft("Current remote content")
        );
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &source,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    2_000_000
                )
                .is_err()
        );
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_parent_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &current,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    2_000_000
                )
                .is_err()
        );
        assert!(store.open().get(checkpoint.scope()).unwrap().as_ref() == Some(&restored));
        assert_eq!(journal.local_review_count(checkpoint.scope()).unwrap(), 0);
        conn.execute_batch("DROP TRIGGER fail_parent_retirement;")
            .unwrap();
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &current,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    2_000_000
                )
                .unwrap()
        );
        let archived: String = conn
            .query_row("SELECT body FROM local_reviews", [], |r| r.get(0))
            .unwrap();
        assert_eq!(archived, bytes);
        assert!(store.open().get(checkpoint.scope()).unwrap().is_none());
        assert!(store.open().has_local_review(&restored).unwrap());
    }
}

#[test]
fn parent_sequence_v15_refuses_downgrade_missing_reordered_or_forged_parent_reports_and_history() {
    let e = evidence();
    for index in [0, 2, 4, 18, 28, 46] {
        let (source, p) = attempt(&e["vectors"][index]);
        let json = serde_json::to_value(p.checkpoint().unwrap()).unwrap();
        for kind in 0..12 {
            let mut bad = json.clone();
            match kind {
                0 => bad["version"] = 14.into(),
                1 => bad["draft"] = "forged original".into(),
                2 => bad["action"]["EndSequenceParent"]["parent_updates"] = serde_json::json!([]),
                3 => {
                    bad["action"]["EndSequenceParent"]["parent_updates"] =
                        serde_json::json!(["forged final"])
                }
                4 => {
                    let updates = bad["action"]["EndSequenceParent"]["parent_updates"]
                        .as_array_mut()
                        .unwrap();
                    updates.push(updates.last().unwrap().clone());
                }
                5 => {
                    let members = bad["action"]["EndSequenceParent"]["members"].clone();
                    bad["action"] = serde_json::json!({"EndSequence":{"members":members}});
                    bad["version"] = 14.into();
                }
                10 | 11 => {
                    let updates = bad["action"]["EndSequenceParent"]["parent_updates"]
                        .as_array_mut()
                        .unwrap();
                    if updates.len() < 2 {
                        continue;
                    }
                    if kind == 10 {
                        let last = updates.last().unwrap().clone();
                        *updates = vec![last];
                    } else {
                        updates.reverse();
                    }
                }
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    match kind {
                        6 => rows[0] = rows[0].remove(12).unwrap(),
                        7 => {
                            rows[0] = rows[0]
                                .replace_fields(12, &[m(12, &body("wrong", true))])
                                .unwrap()
                        }
                        8 => {
                            rows[0] = rows[0]
                                .replace_fields(
                                    38,
                                    &[m(
                                        38,
                                        &source
                                            .section(p.target_section())
                                            .unwrap()
                                            .crdt()
                                            .unwrap()
                                            .unwrap(),
                                    )],
                                )
                                .unwrap()
                        }
                        _ => rows.swap(0, 1),
                    }
                    bad["request"] = serde_json::to_value(
                        p.data
                            .replace_fields(1, &rows.iter().map(|r| m(1, r)).collect::<Vec<_>>())
                            .unwrap()
                            .raw(),
                    )
                    .unwrap();
                }
            }
            let checkpoint: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                Store::new().open().begin(checkpoint).is_err(),
                "vector{index}/tamper{kind}"
            );
        }
        // Exact old v14 semantics and round-trip remain available for no parent reports.
        let intent = EndIntent::capture_sequence(&source, p.target_section(), 3, 100).unwrap();
        let before = intent.seed().clone();
        let members = p
            .appended_members()
            .map(|m| m.updates().to_vec())
            .collect::<Vec<_>>();
        let witness = EndWitness::sequence(
            &before,
            split::plain(&before).unwrap().encode_utf16().count(),
            &members,
        )
        .unwrap();
        let legacy = intent
            .prepare(&source, &witness, 100)
            .unwrap()
            .checkpoint()
            .unwrap();
        let serialized = serde_json::to_string(&legacy).unwrap();
        let old: Checkpoint = serde_json::from_str(&serialized).unwrap();
        assert_eq!(serde_json::to_string(&old).unwrap(), serialized);
        let mut old_json = serde_json::to_value(&old).unwrap();
        assert_eq!(old_json["version"], 14);
        assert!(matches!(
            Store::new().open().begin(old).unwrap(),
            Begin::Started(_)
        ));
        old_json["action"]["EndSequence"]["parent_updates"] = serde_json::json!(["injected"]);
        assert!(serde_json::from_value::<Checkpoint>(old_json).is_err());
    }
}
