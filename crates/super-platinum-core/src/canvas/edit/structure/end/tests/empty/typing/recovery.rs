use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-empty-typing-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
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
fn empty_typing_v11_restart_review_rollback_and_archive_keep_ordered_pair() {
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for layout in ["no-following", "empty-final-following"] {
            for values in [
                vec![
                    "New",
                    "New 日本😀 &amp; draftx",
                    "New 日本😀 &amp; draft",
                    "New 日本😀 &amp; paragraph",
                ],
                vec!["x", ""],
                vec!["x", "x draftx", "x", ""],
            ] {
                let s = fixture_from_reference(mode, layout);
                let updates = values.iter().map(|s| draft(s)).collect::<Vec<_>>();
                let p = typed(&s, source_id(mode), &updates);
                let c = p.checkpoint().unwrap();
                let bytes = serde_json::to_string(&c).unwrap();
                let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
                assert!(decoded == c);
                let json = serde_json::to_value(&c).unwrap();
                assert_eq!(json["version"], 11);
                assert_eq!(json["draft"], "");
                assert_eq!(
                    json["action"]["EmptyTyping"]["child_updates"],
                    serde_json::json!(values)
                );
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
                // Current source snapshot proves neither remote success nor failure.
                let review = journal.local_review(&restored, &s).unwrap();
                let end = review.end().unwrap();
                assert!(end.is_empty_enter() && end.is_empty_typing() && !end.clears_child());
                assert_eq!(end.parent_id(), source_id(mode));
                assert_eq!(end.child_id(), p.child_section());
                assert_eq!(end.reuses_child(), p.reuses_child());
                assert_eq!(end.original_text(), "");
                assert_eq!(end.parent_text(), "");
                assert_eq!(
                    end.child_text(),
                    updates
                        .last()
                        .unwrap()
                        .runs
                        .iter()
                        .map(|r| r.text.as_str())
                        .collect::<String>()
                );
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
                conn.execute_batch("CREATE TRIGGER fail_empty_typing_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
                conn.execute_batch("DROP TRIGGER fail_empty_typing_retirement;")
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
    }
}

#[test]
fn empty_typing_v11_rejects_action_report_and_request_confusion_preserves_v6_v10() {
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for layout in ["no-following", "empty-final-following"] {
            let s = fixture_from_reference(mode, layout);
            let p = typed(
                &s,
                source_id(mode),
                &[draft("x"), draft("x draftx"), draft("x"), draft("")],
            );
            let c = p.checkpoint().unwrap();
            let json = serde_json::to_value(&c).unwrap();
            for kind in 0..12 {
                let mut bad = json.clone();
                match kind {
                    0 => bad["version"] = 10.into(),
                    1 => bad["draft"] = "x".into(),
                    2 => bad["action"]["EmptyTyping"]["reused"] = (!p.reuses_child()).into(),
                    3 => bad["action"]["EmptyTyping"]["child_updates"] = serde_json::json!([]),
                    4 => {
                        bad["action"]["EmptyTyping"]["child_updates"] = serde_json::json!(["x", ""])
                    }
                    5 => {
                        bad["action"]["EmptyTyping"]["child_updates"] =
                            serde_json::json!(["x", "x", "x draftx", "x", ""])
                    }
                    6 => {
                        bad["version"] = 10.into();
                        bad["action"] = serde_json::json!({"EmptyEnter":{"child":p.child_section(),"position":p.intent.position,"reused":p.reuses_child()}});
                    }
                    7 => {
                        bad["version"] = 6.into();
                        bad["action"] = serde_json::json!({"EndTyping":{"child":p.child_section(),"position":p.intent.position,"reused":p.reuses_child(),"offset_utf16":0,"child_updates":["x","x draftx","x",""]}});
                    }
                    _ => {
                        let mut rows = p.data.messages(1).unwrap();
                        match kind {
                            8 => {
                                rows.pop();
                            }
                            9 => rows.swap(0, 1),
                            10 => {
                                rows[1] = rows[1].remove(12).unwrap();
                            }
                            _ => {
                                let wire = p.child_wire.replace_fields(2, &[n(2, 7)]).unwrap();
                                rows[1] = rows[1].replace_fields(38, &[m(38, &wire)]).unwrap();
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
            let old = super::super::attempt(&s, source_id(mode))
                .checkpoint()
                .unwrap();
            let bytes = serde_json::to_string(&old).unwrap();
            let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
            assert!(decoded == old && decoded.is_empty_enter() && !decoded.is_empty_typing());
            assert_eq!(serde_json::to_value(&old).unwrap()["version"], 10);
            assert!(matches!(
                Store::new().open().begin(decoded).unwrap(),
                Begin::Started(_)
            ));
        }
    }
    let s = fixture("Keep this", true);
    let intent = EndIntent::capture(&s, "item", 100).unwrap();
    let w = witness("Keep this")
        .with_child_updates(&[draft("x"), draft("")])
        .unwrap();
    let old = intent.prepare(&s, &w, 100).unwrap().checkpoint().unwrap();
    let bytes = serde_json::to_string(&old).unwrap();
    let decoded: Checkpoint = serde_json::from_str(&bytes).unwrap();
    assert!(decoded == old && !decoded.is_empty_enter());
    assert_eq!(serde_json::to_value(&old).unwrap()["version"], 6);
    assert!(matches!(
        Store::new().open().begin(decoded).unwrap(),
        Begin::Started(_)
    ));
}
