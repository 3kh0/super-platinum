//! Real local history transitions and deterministic validation-work assertions.
//! No timing claim, native action, HTTP or unchecked history construction.
use super::*;
fn history(entries: usize) -> (Fixture, Vec<String>) {
    let mut f = Fixture::new();
    c::install(&f.store, &f.scope.account_id).unwrap();
    h::migrate(&f.store, &f.scope.account_id).unwrap();
    let mut ids = Vec::new();
    for n in 0..entries {
        let serial = 2 * n as u64 + 1;
        if n > 0 {
            f.review = checkpoint(&f, serial, &local("/example next"));
            (f.prior, f.next) = records(&f.review);
            slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
        }
        let claimed = claim(
            &f.store, &f.stream, &f.review, &f.writer, serial, &f.prior, &f.next,
        )
        .unwrap();
        let target = c::target(&claimed);
        finish(
            &f,
            &target,
            Delivery::Reply(json!({"ok":true,"keep_input":true})),
        );
        let local = current(&f).local().clone();
        checkpoint(&f, serial + 1, &local);
        let reviewed = inspected(&f, serial + 1);
        a::archive_known(&f.store, &reviewed).unwrap();
        ids.push(f.next.id().to_owned());
    }
    (f, ids)
}
#[test]
fn full_archive_audit_validates_each_historical_binding_once_at_four_sizes() {
    for entries in [1, 4, 8, 16] {
        let (f, ids) = history(entries);
        // Count the actual expensive historical binding validation body, not
        // elapsed time or a synthetic index API. Every retained row is audited.
        super::super::super::bindings::take_validation_work();
        evidence::audit(&f.store.0, &f.scope.account_id).unwrap();
        assert_eq!(
            super::super::super::bindings::take_validation_work(),
            entries
        );
        assert_eq!(count(&f, "composer_draft_slash_archives"), entries as i64);
        assert_eq!(count(&f, "composer_draft_slash_bindings"), 0);
        assert_eq!(ids.len(), entries);
    }
}
#[test]
fn exact_lookup_still_rejects_unrelated_binding_and_completion_corruption() {
    let (f, ids) = history(3);
    for table in [
        "composer_draft_slash_archives",
        "composer_draft_slash_completions",
    ] {
        let column = if table.ends_with("archives") {
            "binding"
        } else {
            "content"
        };
        let sql = format!("SELECT {column} FROM {table} WHERE intent=?1");
        let raw: String = f.store.0.query_row(&sql, [&ids[0]], |r| r.get(0)).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        if column == "binding" {
            value["writer"] = json!(uuid::Uuid::new_v4().to_string());
        } else {
            value["binding"] = json!("{}");
        }
        f.store.0.execute_batch("SAVEPOINT corrupt").unwrap();
        f.store
            .0
            .execute(
                &format!("UPDATE {table} SET {column}=?1 WHERE intent=?2"),
                params![value.to_string(), ids[0]],
            )
            .unwrap();
        // Looking up a DIFFERENT valid receipt must still audit all history.
        assert!(c::settlement_raw(&f.store.0, &f.scope, &ids[2]).is_err());
        assert!(evidence::audit(&f.store.0, &f.scope.account_id).is_err());
        f.store
            .0
            .execute_batch("ROLLBACK TO corrupt; RELEASE corrupt")
            .unwrap();
        c::settlement_raw(&f.store.0, &f.scope, &ids[2]).unwrap();
        evidence::audit(&f.store.0, &f.scope.account_id).unwrap();
    }
}
#[test]
fn fresh_audit_rejects_changed_attestation_archive_and_duplicate_history() {
    let (f, ids) = history(3);
    for which in 0..3 {
        evidence::audit(&f.store.0, &f.scope.account_id).unwrap();
        f.store.0.execute_batch("SAVEPOINT corrupt").unwrap();
        match which {
            0 => {
                f.store
                    .0
                    .execute(
                        "UPDATE composer_draft_slash_attestations SET content='{}' WHERE intent=?1",
                        [&ids[0]],
                    )
                    .unwrap();
            }
            1 => {
                let raw: String = f
                    .store
                    .0
                    .query_row(
                        "SELECT content FROM composer_draft_slash_archives WHERE intent=?1",
                        [&ids[0]],
                        |r| r.get(0),
                    )
                    .unwrap();
                let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
                value["serial"] = json!(999);
                f.store
                    .0
                    .execute(
                        "UPDATE composer_draft_slash_archives SET content=?1 WHERE intent=?2",
                        params![value.to_string(), ids[0]],
                    )
                    .unwrap();
            }
            _ => {
                f.store.0.execute("INSERT INTO composer_draft_slash_bindings SELECT intent,destination,enrollment,binding FROM composer_draft_slash_archives WHERE intent=?1", [&ids[0]]).unwrap();
            }
        }
        assert!(evidence::audit(&f.store.0, &f.scope.account_id).is_err());
        f.store
            .0
            .execute_batch("ROLLBACK TO corrupt; RELEASE corrupt")
            .unwrap();
        evidence::audit(&f.store.0, &f.scope.account_id).unwrap();
    }
}

#[path = "api_slash_archive_read_phase_tests.rs"]
mod read_phase_tests;
