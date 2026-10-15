use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal},
};
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000781";
pub(super) struct Store(pub(super) std::path::PathBuf);
impl Store {
    pub(super) fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("canvas-pair-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        Self(path)
    }
    pub(super) fn open(&self) -> Journal {
        Journal::open(&self.0, ACCOUNT).unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(super) fn started(j: &mut Journal, c: Checkpoint) -> recovery::journal::Receipt {
    match j.begin(c).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("occupied"),
    }
}
fn checkpoint() -> Checkpoint {
    prepared(&fixture()).checkpoint().unwrap()
}
fn payload(mut value: serde_json::Value, variant: usize) -> Checkpoint {
    let request: Vec<u8> = serde_json::from_value(value["request"].clone()).unwrap();
    let data = RawMessage::decode(&request).unwrap();
    let mut updates = data.messages(1).unwrap();
    match variant {
        0 => {
            updates.pop();
        }
        1 => updates.reverse(),
        2 => updates[1] = updates[0].clone(),
        3 => {
            updates[1] = updates[1]
                .replace_fields(33, &[t(33, "different-0")])
                .unwrap()
        }
        4 => updates[1] = updates[1].remove(38).unwrap(),
        5 => updates[1] = updates[1].replace_fields(8, &[t(8, "aae:temp")]).unwrap(),
        _ => updates[0] = updates[0].replace_fields(3, &[n(3, 0)]).unwrap(),
    }
    let changed = data
        .replace_fields(1, &updates.iter().map(|u| m(1, u)).collect::<Vec<_>>())
        .unwrap();
    value["request"] = serde_json::json!(changed.raw());
    serde_json::from_value(value).unwrap()
}

#[test]
fn v2_checkpoint_roundtrip_preserves_raw_inventory_and_one_pair_request() {
    let p = prepared(&fixture());
    let c = p.checkpoint().unwrap();
    let value = serde_json::to_value(&c).unwrap();
    assert_eq!(value["version"], 2);
    let stored: Vec<u8> = serde_json::from_value(value["request"].clone()).unwrap();
    assert_eq!(stored, p.document_data().raw());
    let sections: Vec<Vec<u8>> = serde_json::from_value(value["sections"].clone()).unwrap();
    assert!(
        sections
            .iter()
            .any(|s| s.windows(4).any(|b| b == [0xc8, 0x3e, 0x81, 0x00]))
    );
    let restored: Checkpoint = serde_json::from_value(value).unwrap();
    assert!(restored == c);
    let store = Store::new();
    let receipt = started(&mut store.open(), restored);
    let j = store.open();
    let read = j.get(c.scope()).unwrap().unwrap();
    assert!(read == receipt);
    assert!(j.local_review(&read, &fixture()).unwrap().is_replacement());
    assert_eq!(j.local_review_count(c.scope()).unwrap(), 0);
    assert!(j.get(c.scope()).unwrap().is_some());
}

#[test]
fn v2_partial_reordered_foreign_history_and_version_confusion_refuse_reservation() {
    let c = checkpoint();
    let value = serde_json::to_value(&c).unwrap();
    let store = Store::new();
    let mut j = store.open();
    for variant in 0..7 {
        assert!(
            j.begin(payload(value.clone(), variant)).is_err(),
            "variant {variant}"
        );
        assert!(j.get(c.scope()).unwrap().is_none());
    }
    for version in [0, 1, 3] {
        let mut v = value.clone();
        v["version"] = serde_json::json!(version);
        assert!(j.begin(serde_json::from_value(v).unwrap()).is_err());
    }
    let mut oversize = value.clone();
    oversize["request"] = serde_json::json!(vec![0u8; crate::canvas::wire::MAX_BYTES + 1]);
    assert!(j.begin(serde_json::from_value(oversize).unwrap()).is_err());
    let mut same_id = value;
    same_id["action"]["Replace"]["replacement"] = same_id["target"].clone();
    assert!(j.begin(serde_json::from_value(same_id).unwrap()).is_err());
}

#[test]
fn v1_database_migration_retains_exact_old_body_and_v2_reopen_is_one_file_lock() {
    let store = Store::new();
    let s = fixture();
    let old = StructureIntent::insert(&s, "item", Side::After, &TextDraft::default(), 100)
        .unwrap()
        .prepare(&s, 100)
        .unwrap()
        .checkpoint()
        .unwrap();
    let old_receipt = started(&mut store.open(), old.clone());
    let before: String;
    {
        let db = rusqlite::Connection::open(&store.0).unwrap();
        before = db
            .query_row("SELECT body FROM receipts", [], |row| row.get(0))
            .unwrap();
        db.execute_batch("DROP TABLE local_reviews; PRAGMA user_version=1;")
            .unwrap();
    }
    let c = checkpoint();
    let receipt = {
        let mut j = store.open();
        assert!(j.get(old.scope()).unwrap().unwrap() == old_receipt);
        assert!(matches!(j.begin(c.clone()).unwrap(), Begin::Blocked(_)));
        // Fixture never dispatched: explicit non-dispatch retirement only.
        assert!(j.complete(&old_receipt).unwrap());
        started(&mut j, c.clone())
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&before).unwrap()["version"],
        1
    );
    let mut j = store.open();
    assert!(j.get(c.scope()).unwrap().unwrap() == receipt);
    assert!(matches!(j.begin(old).unwrap(), Begin::Blocked(_)));
    assert!(!j.complete(&old_receipt).unwrap());
    assert!(j.get(c.scope()).unwrap().is_some());
    assert!(Journal::open(&store.0, "00000000-0000-4000-8000-000000000782").is_err());
}

#[test]
fn journal_failures_keep_pair_uncertainty_and_do_not_publish_partial_reservations() {
    let store = Store::new();
    let c = checkpoint();
    let mut j = store.open();
    {
        let db = rusqlite::Connection::open(&store.0).unwrap();
        db.execute_batch("CREATE TRIGGER reject_pair BEFORE INSERT ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    }
    assert!(j.begin(c.clone()).is_err());
    assert!(j.get(c.scope()).unwrap().is_none());
    {
        let db = rusqlite::Connection::open(&store.0).unwrap();
        db.execute_batch("DROP TRIGGER reject_pair;").unwrap();
    }
    let receipt = started(&mut j, c.clone());
    {
        let db = rusqlite::Connection::open(&store.0).unwrap();
        db.execute_batch("CREATE TRIGGER reject_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    }
    assert!(j.complete(&receipt).is_err());
    drop(j);
    assert!(store.open().get(c.scope()).unwrap().unwrap() == receipt);
}

#[test]
fn malformed_persisted_pair_refuses_read_without_erasing_the_file_lock() {
    let store = Store::new();
    let c = checkpoint();
    started(&mut store.open(), c.clone());
    let body = serde_json::to_string(&payload(serde_json::to_value(&c).unwrap(), 0)).unwrap();
    {
        let db = rusqlite::Connection::open(&store.0).unwrap();
        db.execute("UPDATE receipts SET body=?1", [&body]).unwrap();
    }
    let mut j = store.open();
    assert!(j.get(c.scope()).is_err());
    assert!(j.begin(c).is_err());
    drop(j);
    let db = rusqlite::Connection::open(&store.0).unwrap();
    let retained: String = db
        .query_row("SELECT body FROM receipts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(retained, body);
}
