use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-repeat-typing-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
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
fn repeat_typing_v13_restart_review_transaction_rollback_and_archive_retain_all_reports() {
    let e = evidence();
    for index in [0, 4, 8, 12, 16, 20, 24, 28, 32] {
        let v = &e["vectors"][index];
        let b = &e["bindings"][index];
        let s = source(b["priorVector"].as_u64().unwrap() as usize);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let updates = reports(v);
        let p = typed(&s, &id, &updates);
        let c = p.checkpoint().unwrap();
        let bytes = serde_json::to_string(&c).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
        assert!(decoded == c);
        assert!(decoded.is_repeat() && decoded.is_repeat_typing() && decoded.is_end());
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
        assert!(end.is_repeat() && end.is_repeat_typing() && !end.is_empty_enter());
        assert_eq!(end.parent_id(), id);
        assert_eq!(end.child_id(), p.child_section());
        assert_eq!(end.second_child_id(), p.second_child_section());
        assert_eq!(end.child_text(), "");
        assert_eq!(end.parent_text(), split::plain(&p.intent.seed).unwrap());
        let final_text = updates
            .last()
            .unwrap()
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<String>();
        assert_eq!(end.second_child_text(), Some(final_text.as_str()));
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &replace(&s, &id, n(2, 2000)),
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    2_000_000
                )
                .is_err()
        );
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_typing_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
        conn.execute_batch("DROP TRIGGER fail_typing_retirement;")
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
fn repeat_typing_v13_refuses_history_mode_version_and_report_confusion_preserves_v12() {
    let e = evidence();
    for index in [0, 1, 2, 3, 4, 5, 24, 25, 26, 27, 28, 29] {
        let v = &e["vectors"][index];
        let b = &e["bindings"][index];
        let s = source(b["priorVector"].as_u64().unwrap() as usize);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = typed(&s, &id, &reports(v));
        let json = serde_json::to_value(p.checkpoint().unwrap()).unwrap();
        for kind in 0..14 {
            let mut bad = json.clone();
            match kind {
                0 => bad["version"] = 12.into(),
                1 => bad["draft"] = "".into(),
                2 => {
                    bad["action"]["EndRepeatTyping"]["second_child_updates"] = serde_json::json!([])
                }
                3 => {
                    bad["action"]["EndRepeatTyping"]["second_child_updates"] =
                        serde_json::json!([""])
                }
                4 => {
                    let reports = bad["action"]["EndRepeatTyping"]["second_child_updates"]
                        .as_array_mut()
                        .unwrap();
                    reports.push(reports.last().unwrap().clone());
                }
                5 => bad["action"]["EndRepeatTyping"]["reused"] = (!p.reuses_child()).into(),
                6 => bad["action"]["EndRepeatTyping"]["second_child"] = p.child_section().into(),
                7 => bad["action"]["EndRepeatTyping"]["second_position"] = "zzz:temp".into(),
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    match kind {
                        8 => {
                            rows.pop();
                        }
                        9 => rows.swap(1, 2),
                        10 => rows[2] = rows[2].remove(12).unwrap(),
                        11 => {
                            let wire = rows[2].message(38).unwrap().unwrap();
                            let wire = wire
                                .replace_fields(6, &[t(6, wire.string(8).unwrap().unwrap())])
                                .unwrap();
                            rows[2] = rows[2].replace_fields(38, &[m(38, &wire)]).unwrap();
                        }
                        12 => {
                            let wire = rows[2]
                                .message(38)
                                .unwrap()
                                .unwrap()
                                .replace_fields(2, &[n(2, 1000)])
                                .unwrap();
                            rows[2] = rows[2].replace_fields(38, &[m(38, &wire)]).unwrap();
                        }
                        _ => {
                            rows[1] = rows[1]
                                .replace_fields(12, &[m(12, &body("changed", false))])
                                .unwrap()
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
                "{index}/tamper{kind}"
            );
        }
        let old = repeated(&s, &id).checkpoint().unwrap();
        assert!(!old.is_repeat_typing() && old.is_repeat());
        let bytes = serde_json::to_string(&old).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
        assert_eq!(serde_json::to_string(&decoded).unwrap(), bytes);
        assert_eq!(serde_json::to_value(&decoded).unwrap()["version"], 12);
        assert!(matches!(
            Store::new().open().begin(decoded).unwrap(),
            Begin::Started(_)
        ));
    }
}
