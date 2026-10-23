use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-empty-enter-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
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
fn empty_v10_restart_review_rollback_and_unknown_archive_preserve_whole_pair() {
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for layout in [
            "no-following",
            "empty-final-following",
            "empty-nonfinal-following",
        ] {
            let s = source(mode, layout);
            let p = attempt(&s, source_id(mode));
            let c = p.checkpoint().unwrap();
            let bytes = serde_json::to_string(&c).unwrap();
            let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
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
            // The captured source is a valid current comparison even if neither
            // sent member has appeared. This does not infer either outcome.
            let review = journal.local_review(&restored, &s).unwrap();
            let end = review.end().unwrap();
            assert!(end.is_empty_enter());
            assert_eq!(end.parent_id(), source_id(mode));
            assert_eq!(end.child_id(), p.child_section());
            assert_eq!(end.reuses_child(), p.reuses_child());
            assert_eq!(end.original_text(), "");
            assert_eq!(end.parent_text(), "");
            assert_eq!(end.child_text(), "");
            assert!(!end.clears_child());
            let changed = replace(&s, source_id(mode), n(2, 2000));
            assert!(
                journal
                    .remove_local_lock(
                        &review,
                        &changed,
                        LocalDecision::RemoveEditLockOutcomeUnknown,
                        2_000_000
                    )
                    .is_err()
            );
            let conn = rusqlite::Connection::open(&store.0).unwrap();
            conn.execute_batch("CREATE TRIGGER fail_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
            assert!(
                journal
                    .remove_local_lock(
                        &review,
                        &s,
                        LocalDecision::RemoveEditLockOutcomeUnknown,
                        2_000_000
                    )
                    .is_err()
            );
            assert_eq!(journal.local_review_count(c.scope()).unwrap(), 0);
            assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&restored));
            conn.execute_batch("DROP TRIGGER fail_retirement;").unwrap();
            assert!(
                journal
                    .remove_local_lock(
                        &review,
                        &s,
                        LocalDecision::RemoveEditLockOutcomeUnknown,
                        2_000_000
                    )
                    .unwrap()
            );
            let archived: String = conn
                .query_row("SELECT body FROM local_reviews", [], |r| r.get(0))
                .unwrap();
            assert_eq!(archived, bytes);
            assert!(store.open().get(c.scope()).unwrap().is_none());
            assert!(store.open().has_local_review(&restored).unwrap());
        }
    }
}

#[test]
fn empty_v10_action_request_source_and_order_confusion_are_refused_without_legacy_drift() {
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for layout in ["no-following", "empty-final-following"] {
            let s = source(mode, layout);
            let p = attempt(&s, source_id(mode));
            let c = p.checkpoint().unwrap();
            let json = serde_json::to_value(&c).unwrap();
            for kind in 0..8 {
                let mut bad = json.clone();
                match kind {
                    0 => bad["version"] = 5.into(),
                    1 => bad["draft"] = " ".into(),
                    2 => bad["action"]["EmptyEnter"]["reused"] = (!p.reuses_child()).into(),
                    3 => bad["action"]["EmptyEnter"]["position"] = "zzz:temp".into(),
                    4 => {
                        bad["version"] = if p.reuses_child() { 5 } else { 4 }.into();
                        let name = if p.reuses_child() { "EndReuse" } else { "End" };
                        bad["action"] = serde_json::json!({name:{"child":p.child_section(),"position":p.intent.position,"offset_utf16":0}});
                    }
                    _ => {
                        let mut rows = p.data.messages(1).unwrap();
                        match kind {
                            5 => {
                                rows.pop();
                            }
                            6 => rows.swap(0, 1),
                            _ => {
                                let w = p.parent_wire.replace_fields(2, &[n(2, 7)]).unwrap();
                                rows[0] = rows[0].replace_fields(38, &[m(38, &w)]).unwrap();
                            }
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
                    Store::new().open().begin(bad).is_err(),
                    "{mode}/{layout}/corruption{kind}"
                );
            }
        }
    }
    // Existing nonempty v4 and v5 still decode and validate their original shape.
    let s = fixture("Keep this", true);
    let old = prepare(&s).checkpoint().unwrap();
    assert_eq!(serde_json::to_value(&old).unwrap()["version"], 4);
    assert!(!old.is_empty_enter());
    let decoded: Checkpoint = serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
    assert!(matches!(
        Store::new().open().begin(decoded).unwrap(),
        Begin::Started(_)
    ));
    let (s, _) = zero_reuse::source();
    let old = prepare(&s).checkpoint().unwrap();
    assert_eq!(serde_json::to_value(&old).unwrap()["version"], 5);
    assert!(!old.is_empty_enter());
    let decoded: Checkpoint = serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
    assert!(matches!(
        Store::new().open().begin(decoded).unwrap(),
        Begin::Started(_)
    ));
}
