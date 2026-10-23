use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("middle-typing-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000796").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn rewrite_request(value: &mut Value, rows: &[RawMessage]) {
    let bytes = value["request"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    let original = RawMessage::decode(&bytes).unwrap();
    let mut fields = rows.iter().map(|r| m(1, r)).collect::<Vec<_>>();
    fields.push(m(14, &original.message(14).unwrap().unwrap()));
    let raw = msg(&fields);
    value["request"] = serde_json::to_value(raw.raw()).unwrap();
}
fn legacy_parent(row: &RawMessage) -> RawMessage {
    let wire = row.message(38).unwrap().unwrap();
    let heads = processed(&wire);
    assert_eq!(heads.len(), 3);
    assert_eq!(heads[0], heads[2]);
    // Old update() appended processed6, current12, then SplitInfo10.
    // Merely dropping the third6 from the new wire leaves a different order.
    let mut bytes = Vec::new();
    for field in wire
        .fields()
        .iter()
        .filter(|f| !matches!(f.tag(), 6 | 12 | 10))
    {
        bytes.extend_from_slice(wire.encoded_field(field).unwrap());
    }
    for field in [
        t(6, &heads[0]),
        t(6, &heads[1]),
        t(12, wire.string(12).unwrap().unwrap()),
        m(10, &wire.message(10).unwrap().unwrap()),
    ] {
        bytes.extend_from_slice(field.raw());
    }
    row.replace_fields(38, &[m(38, &RawMessage::decode(&bytes).unwrap())])
        .unwrap()
}

#[test]
fn original_native_v3_receipt_bytes_survive_new_decoder_and_restart() {
    // Exact unresolved checkpoint from the accepted offline middle-split run,
    // not reconstructed with the new encoder. Provenance is in core/fix-c.
    let original = include_str!("legacy-v3.json");
    let c: Checkpoint = serde_json::from_str(original).unwrap();
    assert!(c.is_split() && !c.is_split_typing());
    let store = Store::new();
    let receipt = match store.open().begin(c.clone()).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("fresh"),
    };
    assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&receipt));
    let conn = rusqlite::Connection::open(&store.0).unwrap();
    let body: String = conn
        .query_row("SELECT body FROM receipts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(body, original);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        serde_json::from_str::<Value>(original).unwrap()
    );
    assert!(serde_json::from_str::<Checkpoint>(&body).unwrap() == c);
}

#[test]
fn v8_roundtrip_restart_review_retains_original_tail_final_empty_and_whole_pair() {
    for v in vectors()["vectors"].as_array().unwrap() {
        let (s, p) = attempt(v);
        let c = p.checkpoint().unwrap();
        assert!(c.is_split() && c.is_split_typing());
        let value = serde_json::to_value(&c).unwrap();
        assert_eq!(value["version"], 8);
        assert_eq!(value["action"]["SplitTyping"]["child_draft"], v["tail"]);
        assert_eq!(value["action"]["SplitTyping"]["child_updates"], v["states"]);
        let decoded: Checkpoint = serde_json::from_value(value).unwrap();
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
        let observed = readback(&s, &pair(&p, &s)[..1]);
        let review = journal.local_review(&restored, &observed).unwrap();
        let split = review.split().unwrap();
        assert!(split.has_child_updates());
        assert_eq!(
            split.initial_child_text(),
            plain(p.initial_child_draft()).unwrap()
        );
        assert_eq!(
            split.child_text(),
            p.child_draft()
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>()
        );
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &observed,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    123
                )
                .is_err()
        );
        assert_eq!(journal.local_review_count(c.scope()).unwrap(), 0);
        assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&restored));
        conn.execute_batch("DROP TRIGGER reject_retirement;")
            .unwrap();
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &observed,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    123
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

#[test]
fn fresh_untyped_parent_uses_original_projection_and_old_v3_request_still_decodes() {
    // Four matched original baselines cover both parent cohorts and corpora.
    for index in [0, 3, 6, 9] {
        let mut v = vectors()["vectors"][index].clone();
        v["states"] = serde_json::json!([]);
        let (s, p) = attempt(&v);
        let c = p.checkpoint().unwrap();
        assert!(!c.is_split_typing());
        let mut value = serde_json::to_value(&c).unwrap();
        assert_eq!(value["version"], 3);
        let mut rows = p.document_data().messages(1).unwrap();
        assert_eq!(processed(&rows[0].message(38).unwrap().unwrap()).len(), 3);
        rows[0] = legacy_parent(&rows[0]);
        rewrite_request(&mut value, &rows);
        let legacy: Checkpoint = serde_json::from_value(value.clone()).unwrap();
        let store = Store::new();
        let receipt = match store.open().begin(legacy.clone()).unwrap() {
            Begin::Started(r) => r,
            _ => panic!("fresh"),
        };
        let restored = store.open().get(legacy.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        let review = store.open().local_review(&restored, &s).unwrap();
        assert!(!review.split().unwrap().has_child_updates());
        assert_eq!(serde_json::to_value(&legacy).unwrap(), value); // original durable bytes untouched
    }
}

#[test]
fn v8_rejects_old_parent_projection_action_confusion_reports_and_history_tampering() {
    let (s, p) = attempt(&vectors()["vectors"][0]);
    let c = p.checkpoint().unwrap();
    let original = serde_json::to_value(&c).unwrap();
    for kind in 0..11 {
        let mut bad = original.clone();
        match kind {
            0 => bad["version"] = 3.into(),
            1 => bad["action"]["SplitTyping"]["child_updates"] = serde_json::json!([]),
            2 => {
                bad["action"]["SplitTyping"]["child_updates"] = serde_json::json!(["New paragraph"])
            }
            3 => {
                bad["action"]["SplitTyping"]["child_updates"] =
                    serde_json::json!(["New", "New", "New draftx", "New draft", "New paragraph"])
            }
            4 => bad["action"]["SplitTyping"]["child_draft"] = "New paragraph".into(),
            5 => bad["session"] = "000000000000".into(),
            _ => {
                let mut rows = p.document_data().messages(1).unwrap();
                match kind {
                    6 => rows[0] = legacy_parent(&rows[0]),
                    7 => {
                        rows.pop();
                    }
                    8 => rows[1] = rows[0].clone(),
                    9 => {
                        let fake =
                            crdt::insert(p.child_section(), p.child_draft(), p.session_id(), 100)
                                .unwrap();
                        rows[1] = rows[1].replace_fields(38, &[m(38, &fake)]).unwrap();
                    }
                    _ => rows[1] = rows[1].remove(12).unwrap(),
                }
                rewrite_request(&mut bad, &rows);
            }
        }
        let decoded: Checkpoint = serde_json::from_value(bad).unwrap();
        assert!(Store::new().open().begin(decoded).is_err(), "tamper {kind}");
    }
    assert!(p.validate_snapshot(&s, 100).is_ok());
}
