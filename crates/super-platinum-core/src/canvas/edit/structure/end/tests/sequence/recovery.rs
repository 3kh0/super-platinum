use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-end-sequence-{}.sqlite",
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
fn sequence_v14_restart_review_rollback_and_archive_keep_all_stages_and_current_separate() {
    let e = evidence();
    check_restart(&e, &[0, 11, 24, 35, 38, 41, 43, 44, 47]);
}
pub(super) fn check_restart(e: &serde_json::Value, indices: &[usize]) {
    for &index in indices {
        let v = &e["vectors"][index];
        let s = source(&e["bindings"][index]);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = attempt(&s, &id, &reports(v));
        let c = p.checkpoint().unwrap();
        let bytes = serde_json::to_string(&c).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
        assert!(decoded == c);
        assert_eq!(serde_json::to_string(&decoded).unwrap(), bytes);
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
        let current = replace(&s, &id, m(12, &body("Current remote paragraph", true)));
        let review = journal.local_review(&restored, &current).unwrap();
        let end = review.end().unwrap();
        assert!(
            end.is_sequence() && !end.is_repeat() && !end.is_empty_enter() && !end.clears_child()
        );
        assert_eq!(end.original_text(), split::plain(&p.intent.seed).unwrap());
        assert_eq!(end.parent_text(), end.original_text());
        assert_eq!(end.appended_members().len(), p.appended_members().len());
        assert!(
            text::canonical_draft(review.observed_sections().find(|r| r.id() == id).unwrap())
                .unwrap()
                == draft("Current remote paragraph")
        );
        for (view, member) in end.appended_members().zip(p.appended_members()) {
            assert_eq!(view.id(), member.id());
            assert_eq!(view.position(), member.position());
            assert_eq!(view.reused(), member.reused());
            assert_eq!(
                view.original_rtml(),
                if member.reused() { Some("") } else { None }
            );
            assert_eq!(view.intended_rtml(), member.draft().rtml().unwrap());
            assert_eq!(view.report_count(), member.updates().len());
        }
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
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_sequence_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
        assert_eq!(journal.local_review_count(c.scope()).unwrap(), 0);
        assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&restored));
        conn.execute_batch("DROP TRIGGER fail_sequence_retirement;")
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
        assert!(store.open().get(c.scope()).unwrap().is_none());
        assert!(store.open().has_local_review(&restored).unwrap());
    }
}
#[test]
fn sequence_v14_refuses_member_report_request_and_version_confusion_without_reinterpreting_v12_v13()
{
    let e = evidence();
    for index in [24, 35, 38, 41] {
        let v = &e["vectors"][index];
        let s = source(&e["bindings"][index]);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = attempt(&s, &id, &reports(v));
        let json = serde_json::to_value(p.checkpoint().unwrap()).unwrap();
        for kind in 0..13 {
            let mut bad = json.clone();
            match kind {
                0 => bad["version"] = 13.into(),
                1 => bad["draft"] = "".into(),
                2 => bad["action"]["EndSequence"]["members"] = serde_json::json!([]),
                3 => {
                    bad["action"]["EndSequence"]["members"][1]["id"] =
                        bad["action"]["EndSequence"]["members"][0]["id"].clone()
                }
                4 => bad["action"]["EndSequence"]["members"][1]["position"] = "zzz:temp".into(),
                5 => bad["action"]["EndSequence"]["members"][1]["reused"] = true.into(),
                6 => bad["action"]["EndSequence"]["members"][0]["updates"] = serde_json::json!([]),
                7 => {
                    let reports = bad["action"]["EndSequence"]["members"][0]["updates"]
                        .as_array_mut()
                        .unwrap();
                    reports.push(reports.last().unwrap().clone());
                }
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    match kind {
                        8 => {
                            rows.pop();
                        }
                        9 => rows.swap(1, 2),
                        10 => rows[1] = rows[1].remove(12).unwrap(),
                        11 => {
                            let w = rows[1]
                                .message(38)
                                .unwrap()
                                .unwrap()
                                .replace_fields(2, &[n(2, 123)])
                                .unwrap();
                            rows[1] = rows[1].replace_fields(38, &[m(38, &w)]).unwrap();
                        }
                        _ => {
                            rows[0] = rows[0]
                                .replace_fields(12, &[m(12, &body("changed", false))])
                                .unwrap()
                        }
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
            let bad: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                Store::new().open().begin(bad).is_err(),
                "vector{index}/tamper{kind}"
            );
        }
        for typed in [false, true] {
            let intent = EndIntent::capture_repeat(&s, &id, 100).unwrap();
            let before = intent.seed().clone();
            let w = EndWitness::repeat(
                &before,
                split::plain(&before).unwrap().encode_utf16().count(),
                &before,
                &TextDraft::default(),
                &TextDraft::default(),
            )
            .unwrap();
            let w = if typed {
                w.with_second_child_updates(&[draft("old third")]).unwrap()
            } else {
                w
            };
            let old = intent.prepare(&s, &w, 100).unwrap().checkpoint().unwrap();
            assert!(!old.is_sequence() && old.is_repeat());
            let bytes = serde_json::to_string(&old).unwrap();
            let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
            assert_eq!(serde_json::to_string(&decoded).unwrap(), bytes);
            assert_eq!(
                serde_json::to_value(&decoded).unwrap()["version"],
                if typed { 13 } else { 12 }
            );
            assert!(matches!(
                Store::new().open().begin(decoded).unwrap(),
                Begin::Started(_)
            ));
        }
    }
}
