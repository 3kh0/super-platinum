use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000976";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-start-typing-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
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

#[test]
fn v9_restart_review_preserves_initial_final_and_empty_before_with_atomic_unknown_archive() {
    for v in vectors()
        .into_iter()
        .filter(|v| v["sourceMode"] == "retained")
    {
        let (s, p) = attempt(&v);
        let checkpoint = p.checkpoint().unwrap();
        let serialized = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(serialized["version"], 9);
        assert_eq!(serialized["draft"], v["full"]);
        assert_eq!(
            serialized["action"]["StartTyping"]["original_updates"],
            v["states"]
        );
        let store = Store::new();
        let receipt = match store.open().begin(checkpoint.clone()).unwrap() {
            Begin::Started(r) => r,
            _ => panic!("fresh"),
        };
        let mut j = store.open();
        let restored = j.get(checkpoint.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        assert_eq!(
            serde_json::to_value(restored.checkpoint()).unwrap(),
            serialized
        );
        let v1 = StructureIntent::insert(&s, "item", Side::Before, &TextDraft::default(), 100)
            .unwrap()
            .prepare(&s, 100)
            .unwrap()
            .checkpoint()
            .unwrap();
        assert!(matches!(j.begin(v1).unwrap(), Begin::Blocked(_)));
        let review = j.local_review(&restored, &s).unwrap();
        assert!(review.is_start_typing());
        assert!(!review.is_split() && !review.is_end());
        assert!(review.insertion_anchor().is_none());
        let typed = review.start_typing().unwrap();
        assert_eq!(typed.original_id(), "item");
        assert_eq!(typed.child_id(), p.child_section());
        assert_eq!(typed.child_text(), "");
        let plain = |s: &TextDraft| s.runs.iter().map(|r| r.text.as_str()).collect::<String>();
        assert_eq!(typed.original_text(), plain(p.original_draft()));
        assert_eq!(typed.parent_text(), plain(p.parent_draft()));
        let drift = readback(&s, &[change(s.section("item").unwrap(), n(1002, 1))]);
        assert!(
            j.remove_local_lock(
                &review,
                &drift,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .is_err()
        );
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_start_unlock BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(
            j.remove_local_lock(
                &review,
                &s,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .is_err()
        );
        assert_eq!(j.local_review_count(checkpoint.scope()).unwrap(), 0);
        assert!(j.get(checkpoint.scope()).unwrap().is_some());
        conn.execute_batch("DROP TRIGGER reject_start_unlock;")
            .unwrap();
        drop(conn);
        assert!(
            j.remove_local_lock(
                &review,
                &s,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                124
            )
            .unwrap()
        );
        drop(j);
        let reopened = store.open();
        assert!(reopened.get(checkpoint.scope()).unwrap().is_none());
        assert!(reopened.has_local_review(&receipt).unwrap());
        assert_eq!(reopened.local_review_count(checkpoint.scope()).unwrap(), 1);
    }
}

#[test]
fn v9_validation_rejects_action_report_order_history_projection_and_source_confusion() {
    let (s, p) = attempt(&vectors().remove(6));
    let checkpoint = p.checkpoint().unwrap();
    let original = serde_json::to_value(&checkpoint).unwrap();
    let store = Store::new();
    let reject = |value: Value| {
        let parsed: Checkpoint = serde_json::from_value(value).unwrap();
        assert!(store.open().begin(parsed).is_err());
    };
    for version in [1, 8] {
        let mut value = original.clone();
        value["version"] = version.into();
        reject(value);
    }
    let mut value = original.clone();
    value["action"]["StartTyping"]["original_updates"] = serde_json::json!([]);
    reject(value);
    let mut value = original.clone();
    value["action"]["StartTyping"]["position"] = "aad:temp".into();
    reject(value);
    let mut value = original.clone();
    value["draft"] = "Changed source".into();
    reject(value);
    let mut value = original.clone();
    value["action"]["StartTyping"]["original_updates"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reject(value);
    for tag in [6, 5] {
        let mut rows = p.data.messages(1).unwrap();
        let wire = rows[0].message(38).unwrap().unwrap();
        let bad = if tag == 6 {
            wire.replace_fields(6, &[t(6, wire.string(12).unwrap().unwrap())])
                .unwrap()
        } else {
            wire.replace_fields(5, &[t(5, "invalid")]).unwrap()
        };
        rows[0] = rows[0].replace_fields(38, &[m(38, &bad)]).unwrap();
        let data = msg(&[
            m(1, &rows[0]),
            m(1, &rows[1]),
            m(14, &p.data.message(14).unwrap().unwrap()),
        ]);
        let mut value = original.clone();
        value["request"] = serde_json::to_value(data.raw()).unwrap();
        reject(value);
    }
    // Existing insertion stays v1 and independently round-trips unchanged.
    let legacy = StructureIntent::insert(&s, "item", Side::Before, &TextDraft::default(), 100)
        .unwrap()
        .prepare(&s, 100)
        .unwrap()
        .checkpoint()
        .unwrap();
    let json = serde_json::to_value(&legacy).unwrap();
    assert_eq!(json["version"], 1);
    let receipt = match store.open().begin(legacy).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("fresh"),
    };
    assert_eq!(
        serde_json::to_value(
            store
                .open()
                .get(receipt.checkpoint().scope())
                .unwrap()
                .unwrap()
                .checkpoint()
        )
        .unwrap(),
        json
    );
}
