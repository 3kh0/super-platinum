use super::*;
use std::time::Duration;

fn count(j: &Journal, table: &str) -> i64 {
    j.conn
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn replace_state(j: &Journal, value: &Value, scope: &Scope, ts: &str) {
    j.conn
        .execute(
            "UPDATE delete_state SET body=?2 WHERE owner=?1",
            params![
                key(scope, ts).unwrap(),
                serde_json::to_string(value).unwrap()
            ],
        )
        .unwrap();
}

#[test]
fn corrupt_state_history_or_reset_never_becomes_an_empty_inventory() {
    for fault in [
        "json",
        "unknown-field",
        "phase",
        "history",
        "owner",
        "orphan",
        "reset",
        "oversized",
        "oversized-history",
    ] {
        let temp = Temp::new();
        let (_, record, _) = fixture();
        let scope = scope_for(&record).unwrap();
        let mut j = temp.open();
        let aid = id();
        let pending = begin(&mut j, &record, &aid);
        let owner = key(&scope, record.ts()).unwrap();
        let mut raw = serde_json::to_value(&pending).unwrap();
        match fault {
            "json" => {
                j.conn
                    .execute("UPDATE delete_state SET body='{' WHERE owner=?1", [&owner])
                    .unwrap();
            }
            "unknown-field" => {
                raw["extra"] = json!(true);
                replace_state(&j, &raw, &scope, record.ts());
            }
            "phase" => {
                raw["attempt"]["phase"] = json!("Confirmed");
                replace_state(&j, &raw, &scope, record.ts());
            }
            "history" => {
                j.conn
                    .execute("DELETE FROM delete_history WHERE id=?1", [&aid])
                    .unwrap();
            }
            "owner" => {
                j.conn
                    .execute(
                        "UPDATE delete_history SET owner='foreign' WHERE id=?1",
                        [&aid],
                    )
                    .unwrap();
            }
            "orphan" => {
                j.conn
                    .execute("DELETE FROM delete_state WHERE owner=?1", [&owner])
                    .unwrap();
            }
            "reset" => {
                raw["revision"] = json!(0);
                raw["attempt"] = Value::Null;
                replace_state(&j, &raw, &scope, record.ts());
            }
            "oversized" => {
                j.conn
                    .execute(
                        "UPDATE delete_state SET body=zeroblob(?2) WHERE owner=?1",
                        params![owner, (super::super::super::MAX_BODY + 1) as i64],
                    )
                    .unwrap();
            }
            "oversized-history" => {
                j.conn
                    .execute(
                        "UPDATE delete_history SET body=zeroblob(?2) WHERE id=?1",
                        params![aid, (super::super::super::MAX_BODY + 1) as i64],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(j.delete_inventory(&scope, record.ts()).is_err(), "{fault}");
        assert!(j.delete_begin(&pending, &id(), &record).is_err(), "{fault}");
        drop(j);
        let j = temp.open();
        assert!(
            j.delete_inventory(&scope, record.ts()).is_err(),
            "reopen {fault}"
        );
    }
}

#[test]
fn readback_and_record_corruption_refuse_even_when_state_and_history_agree() {
    for fault in [
        "live-readback",
        "foreign-readback",
        "record",
        "id",
        "deadline",
    ] {
        let temp = Temp::new();
        let (page, record, _) = fixture();
        let scope = scope_for(&record).unwrap();
        let mut j = temp.open();
        let aid = id();
        begin(&mut j, &record, &aid);
        let observed = deleted(&page, record.ts());
        let terminal = j
            .delete_resolve(
                &scope,
                record.ts(),
                &aid,
                DeleteResolution::Observed(&observed),
                0,
            )
            .unwrap();
        let mut raw = serde_json::to_value(&terminal).unwrap();
        match fault {
            "live-readback" => {
                raw["attempt"]["readback"]["raw"]["messages"] = page.raw()["messages"].clone();
            }
            "foreign-readback" => {
                raw["attempt"]["readback"]["raw"]["messages"].as_array_mut().unwrap().push(json!({"ts":record.ts(),"thread_ts":record.target().root_ts(),"user":"UOTHER","subtype":"tombstone"}));
            }
            "record" => {
                raw["attempt"]["record"]["user"] = json!("UOTHER");
            }
            "id" => {
                raw["attempt"]["id"] = json!("invalid-id");
            }
            "deadline" => {
                raw["attempt"]["retry_at"] = json!(99);
            }
            _ => unreachable!(),
        }
        replace_state(&j, &raw, &scope, record.ts());
        j.conn
            .execute(
                "UPDATE delete_history SET body=?2 WHERE id=?1",
                params![aid, serde_json::to_string(&raw["attempt"]).unwrap()],
            )
            .unwrap();
        assert!(j.delete_inventory(&scope, record.ts()).is_err(), "{fault}");
        drop(j);
        assert!(
            temp.open().delete_inventory(&scope, record.ts()).is_err(),
            "reopen {fault}"
        );
    }
}

#[test]
fn row_history_and_byte_limits_roll_back_the_whole_begin() {
    for limit in ["state", "history", "bytes"] {
        let temp = Temp::new();
        let (_, record, _) = fixture();
        let scope = scope_for(&record).unwrap();
        let mut j = temp.open();
        let initial = j.delete_inventory(&scope, record.ts()).unwrap();
        j.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        match limit {
            "state" => {
                for n in 0..MAX_ROWS {
                    j.conn
                        .execute(
                            "INSERT INTO delete_state(owner,body) VALUES(?1,'{}')",
                            [format!("other-{n}")],
                        )
                        .unwrap();
                }
            }
            "history" => {
                for n in 0..MAX_ATTEMPTS {
                    j.conn
                        .execute(
                            "INSERT INTO delete_history(id,owner,body) VALUES(?1,'other','{}')",
                            [format!("other-{n}")],
                        )
                        .unwrap();
                }
            }
            "bytes" => {
                // SQL creates bounded fixture pages without large Rust copies.
                for n in 0..32 {
                    j.conn
                        .execute(
                            "INSERT INTO delete_state(owner,body) VALUES(?1,zeroblob(?2))",
                            params![format!("other-{n}"), 1024 * 1024],
                        )
                        .unwrap();
                }
            }
            _ => unreachable!(),
        }
        j.conn.execute_batch("COMMIT").unwrap();
        let counts = (count(&j, "delete_state"), count(&j, "delete_history"));
        let aid = id();
        assert!(j.delete_begin(&initial, &aid, &record).is_err(), "{limit}");
        assert_eq!(
            (count(&j, "delete_state"), count(&j, "delete_history")),
            counts
        );
        assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), initial);
        assert!(
            j.delete_attempt(&scope, record.ts(), &aid)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn oversized_receipt_and_revision_overflow_leave_pending_evidence_unchanged() {
    let temp = Temp::new();
    let (_, record, _) = fixture();
    let scope = scope_for(&record).unwrap();
    let mut j = temp.open();
    let aid = id();
    let pending = begin(&mut j, &record, &aid);
    let huge = json!({"opaque":"x".repeat(super::super::super::MAX_BODY)});
    assert!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Uncertain(Some(&huge)),
            0
        )
        .is_err()
    );
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), pending);
    assert_eq!(
        j.delete_attempt(&scope, record.ts(), &aid)
            .unwrap()
            .as_ref(),
        pending.attempt()
    );
    let mut exhausted = pending.clone();
    exhausted.revision = i64::MAX as u64;
    replace_state(
        &j,
        &serde_json::to_value(&exhausted).unwrap(),
        &scope,
        record.ts(),
    );
    assert!(
        j.delete_resolve(&scope, record.ts(), &aid, DeleteResolution::NotRequested, 0)
            .is_err()
    );
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), exhausted);
    assert_eq!(
        j.delete_attempt(&scope, record.ts(), &aid)
            .unwrap()
            .as_ref(),
        pending.attempt()
    );
}

#[test]
fn real_exclusive_lock_refuses_begin_and_receipt_until_exact_local_retry() {
    let temp = Temp::new();
    let (_, record, accepted) = fixture();
    let scope = scope_for(&record).unwrap();
    let mut j = temp.open();
    j.conn.busy_timeout(Duration::from_millis(1)).unwrap();
    let other = Connection::open(temp.db()).unwrap();
    let initial = j.delete_inventory(&scope, record.ts()).unwrap();
    let aid = id();
    other.execute_batch("BEGIN EXCLUSIVE").unwrap();
    assert!(j.delete_begin(&initial, &aid, &record).is_err());
    other.execute_batch("ROLLBACK").unwrap();
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), initial);
    assert_eq!(count(&j, "delete_history"), 0);
    let pending = j.delete_begin(&initial, &aid, &record).unwrap();
    other.execute_batch("BEGIN EXCLUSIVE").unwrap();
    assert!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0
        )
        .is_err()
    );
    other.execute_batch("ROLLBACK").unwrap();
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), pending);
    let confirmed = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0,
        )
        .unwrap();
    assert_eq!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0
        )
        .unwrap(),
        confirmed
    );
    assert_eq!(count(&j, "delete_history"), 1);
}

#[test]
fn failure_after_history_write_rolls_back_both_tables_and_preserves_ack_retry() {
    let temp = Temp::new();
    let (_, record, accepted) = fixture();
    let scope = scope_for(&record).unwrap();
    let mut j = temp.open();
    let aid = id();
    let pending = begin(&mut j, &record, &aid);
    j.conn.execute_batch("CREATE TRIGGER refuse_delete_settle BEFORE UPDATE ON delete_state BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END;").unwrap();
    assert!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0
        )
        .is_err()
    );
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), pending);
    assert_eq!(
        j.delete_attempt(&scope, record.ts(), &aid)
            .unwrap()
            .as_ref(),
        pending.attempt()
    );
    j.conn
        .execute_batch("DROP TRIGGER refuse_delete_settle")
        .unwrap();
    let confirmed = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0,
        )
        .unwrap();
    drop(j);
    assert_eq!(
        temp.open().delete_inventory(&scope, record.ts()).unwrap(),
        confirmed
    );
    assert!(Journal::open(temp.db(), "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee").is_err());
}
