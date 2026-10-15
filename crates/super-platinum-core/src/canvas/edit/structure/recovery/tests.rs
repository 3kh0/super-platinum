use super::*;
use journal::{Begin, Journal};
const ACCOUNT: &str = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("canvas-recovery-{}.sqlite", uuid::Uuid::new_v4())))
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
fn prepared(delete: bool) -> PreparedStructureEdit {
    let snapshot = super::super::tests::fixture();
    let intent = if delete {
        StructureIntent::delete(&snapshot, "item", 100).unwrap()
    } else {
        StructureIntent::insert(
            &snapshot,
            "item",
            Side::After,
            &TextDraft::from_rtml("&amp;&lt;<br>😀<b>X</b>").unwrap(),
            100,
        )
        .unwrap()
    };
    intent.prepare(&snapshot, 100).unwrap()
}
fn started(j: &mut Journal, c: Checkpoint) -> journal::Receipt {
    match j.begin(c).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("unexpected prior receipt"),
    }
}

#[test]
fn checkpoint_retains_exact_request_inventory_and_actor_without_credentials() {
    for delete in [false, true] {
        let p = prepared(delete);
        let c = p.checkpoint().unwrap();
        assert_eq!(c.request, p.document_data().raw());
        assert_eq!(c.session, p.session_id());
        assert_eq!(c.sequence, 1);
        assert_eq!(c.target, p.target_section());
        assert_eq!(c.is_delete(), delete);
        let original = super::super::tests::fixture();
        for raw in &c.sections {
            let s = Section::decode(raw).unwrap();
            assert_eq!(original.section(s.id()).unwrap().raw().raw(), raw);
        }
        assert!(
            c.sections
                .iter()
                .any(|v| v.windows(4).any(|b| b == [0xc8, 0x3e, 0x81, 0x00]))
        );
        let json = serde_json::to_value(&c).unwrap();
        let mut keys = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            [
                "action", "document", "draft", "request", "scope", "sections", "sequence",
                "session", "target", "thread", "version"
            ]
        );
    }
}
#[test]
fn reopen_keeps_uncertainty_and_blocks_new_intent_until_exact_completion() {
    let store = Store::new();
    let c = prepared(false).checkpoint().unwrap();
    let receipt = {
        let mut j = store.open();
        started(&mut j, c.clone())
    };
    let mut j = store.open();
    let restored = j.get(c.scope()).unwrap().unwrap();
    assert_eq!(restored.operation_id(), receipt.operation_id());
    assert!(restored.checkpoint() == &c);
    assert!(matches!(
        j.begin(prepared(true).checkpoint().unwrap()).unwrap(),
        Begin::Blocked(_)
    ));
    assert!(j.complete(&restored).unwrap());
    assert!(j.get(c.scope()).unwrap().is_none());
}
#[test]
fn old_receipt_cannot_remove_replacement_and_raw_body_cas_detects_change() {
    let store = Store::new();
    let mut j = store.open();
    let c = prepared(false).checkpoint().unwrap();
    let old = started(&mut j, c.clone());
    assert!(j.complete(&old).unwrap());
    let replacement = started(&mut j, c.clone());
    assert!(!j.complete(&old).unwrap());
    let conn = rusqlite::Connection::open(&store.0).unwrap();
    conn.execute("UPDATE receipts SET body=body || ' '", [])
        .unwrap();
    assert!(!j.complete(&replacement).unwrap());
    assert!(j.get(c.scope()).unwrap().is_some());
}
#[test]
fn account_scope_and_malformed_storage_fail_closed() {
    let store = Store::new();
    let mut j = store.open();
    let c = prepared(true).checkpoint().unwrap();
    started(&mut j, c.clone());
    assert!(Journal::open(&store.0, "8c70f9ba-f6e9-4120-8847-e1c064fd7f27").is_err());
    let mut other = c.scope.clone();
    other.user = "U_OTHER".into();
    assert!(j.get(&other).unwrap().is_none());
    let conn = rusqlite::Connection::open(&store.0).unwrap();
    conn.execute("UPDATE receipts SET body='{}'", []).unwrap();
    assert!(j.get(c.scope()).is_err());
    assert!(j.begin(c).is_err());
}
#[test]
fn invalid_checkpoint_and_failed_disk_reservation_cannot_grant_dispatch() {
    let store = Store::new();
    let mut j = store.open();
    let mut c = prepared(false).checkpoint().unwrap();
    c.session = "wrong".into();
    assert!(j.begin(c).is_err());
    let conn = rusqlite::Connection::open(&store.0).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_receipt BEFORE INSERT ON receipts BEGIN SELECT RAISE(ABORT,'test disk failure'); END;").unwrap();
    let c = prepared(false).checkpoint().unwrap();
    assert!(j.begin(c.clone()).is_err());
    assert!(j.get(c.scope()).unwrap().is_none());
}
#[test]
fn missing_target_after_restart_never_retires_the_durable_receipt() {
    let store = Store::new();
    let c = prepared(true).checkpoint().unwrap();
    {
        let mut j = store.open();
        started(&mut j, c.clone());
    }
    let mut snapshot = super::super::tests::fixture();
    snapshot.sections.retain(|s| s.id() != c.target);
    assert!(snapshot.section(&c.target).is_none());
    // There is intentionally no receipt-to-Accepted or matching-read settlement
    // API. Even complete, matching deletion absence leaves the persisted lock.
    let j = store.open();
    assert!(j.get(c.scope()).unwrap().is_some());
}

#[test]
fn failed_retirement_commit_preserves_the_exact_restart_lock() {
    let store = Store::new();
    let mut j = store.open();
    let c = prepared(false).checkpoint().unwrap();
    let receipt = started(&mut j, c.clone());
    let conn = rusqlite::Connection::open(&store.0).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_remove BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'test disk failure'); END;").unwrap();
    assert!(j.complete(&receipt).is_err());
    drop(j);
    let reopened = store.open().get(c.scope()).unwrap().unwrap();
    assert_eq!(reopened.operation_id(), receipt.operation_id());
    assert!(reopened.checkpoint() == &c);
}
