use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(
            std::env::temp_dir().join(format!("canvas-whitespace-{}.sqlite", uuid::Uuid::new_v4())),
        )
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
fn whitespace_v7_restart_preserves_exact_source_and_reports_without_new_action() {
    let data = vectors();
    for (i, case) in data["cases"].as_array().unwrap().iter().enumerate() {
        let seed = &data["seeds"][case["seed"].as_u64().unwrap() as usize];
        let (s, id) = original_source(seed);
        let reports = case["reports"].as_array().unwrap();
        let updates = reports[1..]
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        let p = attempt(&s, &updates);
        let checkpoint = p.checkpoint().unwrap();
        let value = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(value["version"], 7);
        assert_eq!(
            value["action"]["EndClearing"]["child_updates"],
            serde_json::json!(updates)
        );
        let decoded: Checkpoint = serde_json::from_value(value).unwrap();
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
        let mut observed = readback(&s, &pair(&p, &s));
        // Cycle full/partial/missing observed pairs. Local review never asserts
        // server outcome from the inert checkpoint or rendered whitespace.
        observed
            .sections
            .retain(|r| (r.id() != "item" || i & 1 != 0) && (r.id() != id || i & 2 != 0));
        let review = journal.local_review(&restored, &observed).unwrap();
        let end = review.end().unwrap();
        assert!(end.clears_child() && end.reuses_child());
        assert_eq!(end.original_child_rtml(), seed["rtml"].as_str());
        assert_eq!(end.child_text(), updates.last().copied().unwrap_or(""));
        assert_eq!(end.child_id(), id);
        let db = rusqlite::Connection::open(&store.0).unwrap();
        db.execute_batch("CREATE TRIGGER reject_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
        assert_eq!(journal.local_review_count(checkpoint.scope()).unwrap(), 0);
        assert!(store.open().get(checkpoint.scope()).unwrap().as_ref() == Some(&restored));
        db.execute_batch("DROP TRIGGER reject_retirement;").unwrap();
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
        let archived: String = db
            .query_row("SELECT body FROM local_reviews", [], |r| r.get(0))
            .unwrap();
        assert!(serde_json::from_str::<Checkpoint>(&archived).unwrap() == checkpoint);
    }
}

#[test]
fn whitespace_v7_refuses_legacy_reinterpretation_source_loss_and_skipped_clear_history() {
    let data = vectors();
    for seed in data["seeds"].as_array().unwrap() {
        let (s, id) = original_source(seed);
        let p = attempt(&s, &["x"]);
        let value = serde_json::to_value(p.checkpoint().unwrap()).unwrap();
        for corruption in 0..4 {
            let mut bad = value.clone();
            match corruption {
                0 => {
                    let mut action = bad["action"]["EndClearing"].clone();
                    action["reused"] = true.into();
                    bad["action"] = serde_json::json!({"EndTyping":action});
                    bad["version"] = 6.into();
                }
                1 => {
                    let mut action = bad["action"]["EndClearing"].clone();
                    action.as_object_mut().unwrap().remove("child_updates");
                    bad["action"] = serde_json::json!({"EndReuse":action});
                    bad["version"] = 5.into();
                }
                2 => {
                    for row in bad["sections"].as_array_mut().unwrap() {
                        let bytes: Vec<u8> = serde_json::from_value(row.clone()).unwrap();
                        let section = Section::decode(&bytes).unwrap();
                        if section.id() == id {
                            *row = serde_json::to_value(
                                change(&section, m(12, &body("", false))).raw().raw(),
                            )
                            .unwrap();
                        }
                    }
                }
                _ => bad["action"]["EndClearing"]["child_updates"] = serde_json::json!([]),
            }
            let bad: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                Store::new().open().begin(bad).is_err(),
                "seed={},corruption={corruption}",
                seed["shape"]
            );
        }
        let source = s.section(&id).unwrap();
        let wire = source.crdt().unwrap().unwrap();
        // Same final x, but no mandatory intermediate empty report. Reuse the
        // real patch planner to avoid a malformed-history negative fixture.
        let skipped = super::super::super::typing::build_reports(
            &wire,
            true,
            &[draft("x")],
            p.session_id(),
            100,
        )
        .unwrap();
        let request = super::super::super::typing::project(
            s.section("item").unwrap(),
            Some(source),
            &id,
            &p.intent.position,
            p.session_id(),
            &p.parent_wire,
            &skipped,
            p.intent.title.rich_text(),
            &draft("x"),
        )
        .unwrap();
        let mut bad = value;
        bad["request"] = serde_json::to_value(request.raw()).unwrap();
        let bad: Checkpoint = serde_json::from_value(bad).unwrap();
        assert!(
            Store::new().open().begin(bad).is_err(),
            "skipped clear seed={}",
            seed["shape"]
        );
    }
}
