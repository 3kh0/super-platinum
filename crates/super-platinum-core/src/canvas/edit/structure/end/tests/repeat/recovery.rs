use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("canvas-repeat-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000800").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn repeat_v12_restart_review_rollback_and_exact_unknown_archive_keep_three_members() {
    for index in [0, 2, 6, 8, 12, 14] {
        let s = source(index);
        let mode = original()["vectors"][index]["mode"]
            .as_str()
            .unwrap()
            .to_owned();
        let id = bound_id("paragraph", &mode);
        let p = repeated(&s, &id);
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
        let review = journal.local_review(&restored, &s).unwrap();
        let end = review.end().unwrap();
        assert!(end.is_repeat() && !end.is_empty_enter() && !end.clears_child());
        assert_eq!(end.parent_id(), id);
        assert_eq!(end.child_id(), p.child_section());
        assert_eq!(end.second_child_id(), p.second_child_section());
        assert_eq!(end.original_text(), "Keep this");
        assert_eq!(end.parent_text(), "Keep this");
        assert_eq!(end.child_text(), "");
        assert_eq!(end.second_child_text(), Some(""));
        assert_eq!(end.reuses_child(), p.reuses_child());
        let changed = replace(&s, &id, n(2, 2000));
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
        conn.execute_batch("CREATE TRIGGER fail_repeat_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
        conn.execute_batch("DROP TRIGGER fail_repeat_retirement;")
            .unwrap();
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

#[test]
fn repeat_v12_refuses_partial_reordered_identity_and_action_confusion_preserves_old_actions() {
    for index in [0, 2, 12, 14] {
        let s = source(index);
        let mode = original()["vectors"][index]["mode"]
            .as_str()
            .unwrap()
            .to_owned();
        let p = repeated(&s, &bound_id("paragraph", &mode));
        let json = serde_json::to_value(p.checkpoint().unwrap()).unwrap();
        for kind in 0..14 {
            let mut bad = json.clone();
            match kind {
                0 => bad["version"] = 11.into(),
                1 => bad["draft"] = "".into(),
                2 => bad["action"]["EndRepeat"]["reused"] = (!p.reuses_child()).into(),
                3 => bad["action"]["EndRepeat"]["second_child"] = p.child_section().into(),
                4 => bad["action"]["EndRepeat"]["second_position"] = "zzz:temp".into(),
                5 => bad["action"]["EndRepeat"]["position"] = "zzz:temp".into(),
                6 => bad["action"]["EndRepeat"]["child"] = p.target_section().into(),
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    match kind {
                        7 => {
                            rows.pop();
                        }
                        8 => {
                            rows.remove(1);
                        }
                        9 => rows.swap(1, 2),
                        10 => rows[2] = rows[2].remove(12).unwrap(),
                        11 => {
                            rows[2] = rows[2]
                                .replace_fields(1, &[t(1, p.child_section())])
                                .unwrap()
                        }
                        12 => {
                            let wire = rows[0]
                                .message(38)
                                .unwrap()
                                .unwrap()
                                .replace_fields(2, &[n(2, 7)])
                                .unwrap();
                            rows[0] = rows[0].replace_fields(38, &[m(38, &wire)]).unwrap();
                        }
                        _ => {
                            let wire = rows[2]
                                .message(38)
                                .unwrap()
                                .unwrap()
                                .replace_fields(6, &[t(6, &"0".repeat(64))])
                                .unwrap();
                            rows[2] = rows[2].replace_fields(38, &[m(38, &wire)]).unwrap();
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
                "{index}/corruption{kind}"
            );
        }
    }
    // Newly captured legacy receipts retain their previous version/action bytes.
    let s = fixture("Keep this", true);
    let ordinary = prepare(&s).checkpoint().unwrap();
    let mut e = fixture("", false);
    e.sections.retain(|r| r.id() != "other");
    let untouched = EndIntent::capture_empty(&e, "item", 100)
        .unwrap()
        .prepare(&e, &EndWitness::empty(), 100)
        .unwrap()
        .checkpoint()
        .unwrap();
    let typed = EndIntent::capture_empty(&e, "item", 100)
        .unwrap()
        .prepare(
            &e,
            &EndWitness::empty()
                .with_child_updates(&[draft("x"), draft("")])
                .unwrap(),
            100,
        )
        .unwrap()
        .checkpoint()
        .unwrap();
    for (c, version) in [(ordinary, 4), (untouched, 10), (typed, 11)] {
        assert!(!c.is_repeat());
        let bytes = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::to_value(&c).unwrap()["version"], version);
        let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
        assert_eq!(serde_json::to_string(&decoded).unwrap(), bytes);
        assert!(matches!(
            Store::new().open().begin(decoded).unwrap(),
            Begin::Started(_)
        ));
    }
}
