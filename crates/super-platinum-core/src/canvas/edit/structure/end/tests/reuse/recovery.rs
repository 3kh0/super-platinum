use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000787";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("canvas-end-reuse-{}.sqlite", uuid::Uuid::new_v4())))
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
fn reuse_v5_sqlite_roundtrip_and_review_preserve_existing_whole_pair() {
    for mask in 0..4 {
        let s = reused_fixture(true);
        let p = prepare(&s);
        let checkpoint = p.checkpoint().unwrap();
        assert!(checkpoint.is_end() && checkpoint.is_end_reuse());
        let value = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(value["version"], 5);
        assert_eq!(value["action"]["EndReuse"]["child"], "other");
        assert_eq!(value["action"]["EndReuse"]["position"], "aae");
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
        let mut observed = readback(&s, &pair(&p, &s));
        observed.sections.retain(|r| {
            (r.id() != "item" || mask & 1 != 0) && (r.id() != "other" || mask & 2 != 0)
        });
        let review = journal.local_review(&restored, &observed).unwrap();
        assert!(review.is_end());
        let end = review.end().unwrap();
        assert!(end.reuses_child());
        assert_eq!(end.parent_id(), "item");
        assert_eq!(end.child_id(), "other");
        assert_eq!(end.child_text(), "");
        assert_eq!(journal.local_review_count(checkpoint.scope()).unwrap(), 0);
        assert!(matches!(
            journal.begin(checkpoint.clone()).unwrap(),
            Begin::Blocked(_)
        ));
        let mut changed = observed.clone();
        changed.thread = changed.thread.replace_fields(2, &[n(2, 99999)]).unwrap();
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &changed,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    123
                )
                .is_err()
        );
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_reuse_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END;").unwrap();
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
        assert_eq!(journal.local_review_count(checkpoint.scope()).unwrap(), 0);
        assert!(store.open().get(checkpoint.scope()).unwrap().as_ref() == Some(&restored));
        conn.execute_batch("DROP TRIGGER reject_reuse_retirement;")
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
        let archived: Checkpoint = serde_json::from_str(&archived).unwrap();
        assert!(archived == checkpoint);
        assert!(store.open().has_local_review(&restored).unwrap());
        assert!(store.open().get(checkpoint.scope()).unwrap().is_none());
    }
}

#[test]
fn reuse_v5_corrupt_mode_identity_source_position_and_request_never_reserve() {
    let s = reused_fixture(true);
    let p = prepare(&s);
    let c = p.checkpoint().unwrap();
    let original = serde_json::to_value(&c).unwrap();
    let store = Store::new();
    for kind in 0..11 {
        let mut bad = original.clone();
        match kind {
            0 => bad["version"] = 4.into(),
            1 => {
                bad["version"] = 4.into();
                let action = bad["action"]["EndReuse"].clone();
                bad["action"] = serde_json::json!({"End":action});
            }
            2 => bad["action"]["EndReuse"]["child"] = "new-child".into(),
            3 => bad["action"]["EndReuse"]["position"] = "aae:temp".into(),
            4 => bad["action"]["EndReuse"]["offset_utf16"] = 15.into(),
            5 => {
                let sources = bad["sections"].as_array_mut().unwrap();
                sources.retain(|raw| {
                    Section::decode(&serde_json::from_value::<Vec<u8>>(raw.clone()).unwrap())
                        .unwrap()
                        .id()
                        != "other"
                });
            }
            _ => {
                let mut rows = p.data.messages(1).unwrap();
                match kind {
                    6 => {
                        rows.pop();
                    }
                    7 => rows[1] = rows[0].clone(),
                    8 => rows[1] = rows[1].replace_fields(8, &[t(8, "aae")]).unwrap(),
                    9 => {
                        rows[1] = rows[1]
                            .replace_fields(12, &[m(12, &body("", true))])
                            .unwrap()
                    }
                    _ => {
                        let wire = rows[1].message(38).unwrap().unwrap();
                        let head = wire.string(8).unwrap().unwrap();
                        rows[1] = rows[1]
                            .replace_fields(
                                38,
                                &[m(38, &wire.replace_fields(12, &[t(12, head)]).unwrap())],
                            )
                            .unwrap();
                    }
                }
                let data = p
                    .data
                    .replace_fields(1, &rows.iter().map(|r| m(1, r)).collect::<Vec<_>>())
                    .unwrap();
                bad["request"] = serde_json::to_value(data.raw()).unwrap();
            }
        }
        let c: Checkpoint = serde_json::from_value(bad).unwrap();
        assert!(store.open().begin(c).is_err(), "corruption {kind}");
        assert!(
            store
                .open()
                .get(p.checkpoint().unwrap().scope())
                .unwrap()
                .is_none()
        );
    }
    // Previously accepted v4 must still describe creation, never reinterpret an
    // existing-target request as a new child through an optional mode default.
    let new = prepare(&fixture("Keep this", true)).checkpoint().unwrap();
    assert!(new.is_end() && !new.is_end_reuse());
    let value = serde_json::to_value(&new).unwrap();
    assert_eq!(value["version"], 4);
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<Checkpoint>(value.clone()).unwrap()).unwrap(),
        value
    );
}
