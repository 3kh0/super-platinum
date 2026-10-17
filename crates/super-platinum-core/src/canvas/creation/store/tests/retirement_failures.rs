use super::*;
fn selection(a: &Attempt) -> RetirementSelection {
    RetirementSelection::new(a, None).unwrap()
}
#[test]
fn stale_review_never_deletes_after_newer_text_or_conflict_arrives() {
    let temp = Temp::new();
    let mut a = temp.store();
    let base = seed(&temp, &mut a);
    let r = a.reserve(&base, 100).unwrap();
    let canceled = a.cancel_reserved(&r).unwrap();
    let review = a.retirement_review(&temp.owner()).unwrap();
    let mut b = temp.store();
    let newer = b.save(&r.draft, &writer(), content("Newer")).unwrap();
    assert!(matches!(
        a.retire_terminal(&review, &[selection(&canceled)]),
        Err(Fault::Stale)
    ));
    assert!(a.attempt(&temp.owner(), canceled.id()).unwrap().is_some());
    let review = a.retirement_review(&temp.owner()).unwrap();
    b.preserve_conflict(&newer, &writer(), &writer(), content("Separate local"))
        .unwrap();
    assert!(matches!(
        a.retire_terminal(&review, &[selection(&canceled)]),
        Err(Fault::Stale)
    ));
    assert!(a.active(&temp.owner()).unwrap() == Some(newer));
}
#[test]
fn unresolved_reserved_and_active_created_are_not_retireable() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let r = s.reserve(&base, 100).unwrap();
    assert!(RetirementSelection::new(&r.attempt, None).is_err());
    let a = s.admit(&r, 100).unwrap().consume();
    assert!(RetirementSelection::new(&a, None).is_err());
    let unknown = s.settle(&a, Outcome::Uncertain).unwrap();
    assert!(RetirementSelection::new(&unknown, None).is_err());
    let created = s
        .settle(
            &unknown,
            Outcome::Created {
                file_id: "F1".into(),
            },
        )
        .unwrap();
    let review = s.retirement_review(&temp.owner()).unwrap();
    let selected = RetirementSelection::new(&created, Some("F1")).unwrap();
    assert!(s.retire_terminal(&review, &[selected]).is_err());
    assert_eq!(count(&s.conn, "retired_attempts").unwrap(), 0);
    assert!(s.active(&temp.owner()).unwrap() == Some(r.draft));
}
#[test]
fn content_deletion_failure_rolls_back_tombstone_and_association() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let r = s.reserve(&base, 100).unwrap();
    let canceled = s.cancel_reserved(&r).unwrap();
    let review = s.retirement_review(&temp.owner()).unwrap();
    s.conn.execute_batch("CREATE TEMP TRIGGER fail_retire BEFORE DELETE ON attempts BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(s.retire_terminal(&review, &[selection(&canceled)]).is_err());
    assert_eq!(count(&s.conn, "retired_attempts").unwrap(), 0);
    assert!(s.active(&temp.owner()).unwrap() == Some(r.draft.clone()));
    s.conn.execute_batch("DROP TRIGGER fail_retire;").unwrap();
    s.retire_terminal(&review, &[selection(&canceled)]).unwrap();
    assert!(s.admit(&r, 100).is_err());
}
#[test]
fn migration_keeps_v2_dispositions_byte_exact_and_fences_delayed_retry() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let op = writer();
    let w = writer();
    let c = s
        .preserve_conflict(&base, &w, &op, content("Disposed"))
        .unwrap();
    s.remove_conflict(&c).unwrap();
    let before: String = s
        .conn
        .query_row("SELECT body FROM dispositions WHERE id=?1", [&op], |r| {
            r.get(0)
        })
        .unwrap();
    // Construct the exact prior schema from unchanged predecessor tables; no live storage.
    s.conn.execute_batch("DROP TABLE retired_attempts;DROP TABLE retired_decisions;DROP TABLE evidence_dispositions;DROP TABLE late_evidence;DROP INDEX active_scope_ids;PRAGMA user_version=2;").unwrap();
    drop(s);
    let mut upgraded = temp.store();
    let after: String = upgraded
        .conn
        .query_row("SELECT body FROM dispositions WHERE id=?1", [&op], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    assert!(matches!(
        upgraded.preserve_conflict(&base, &w, &op, content("Disposed")),
        Err(Fault::Retired)
    ));
    assert_eq!(
        upgraded
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
}
#[test]
fn invalid_prior_association_prevents_migration_without_partial_tables() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let _ = start(&mut s, &base);
    s.conn.execute_batch("DROP TABLE retired_attempts;DROP TABLE retired_decisions;DROP TABLE evidence_dispositions;DROP TABLE late_evidence;DROP INDEX active_scope_ids;PRAGMA user_version=2;DELETE FROM scopes;").unwrap();
    drop(s);
    assert!(Store::open(temp.path(), &temp.account).is_err());
    let conn = Connection::open(temp.path()).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='retired_attempts'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(count(&conn, "attempts").unwrap(), 1);
}
#[test]
fn coherent_read_holds_retirement_until_its_projection_finishes() {
    let temp = Temp::new();
    let mut setup = temp.store();
    let base = seed(&temp, &mut setup);
    let r = setup.reserve(&base, 100).unwrap();
    let canceled = setup.cancel_reserved(&r).unwrap();
    drop(setup);
    let reader = temp.store();
    let mut other = temp.store();
    other.conn.busy_timeout(Duration::ZERO).unwrap();
    let review = other.retirement_review(&temp.owner()).unwrap();
    let observed=reader.read_snapshot(|conn| {
        let result=other.retire_terminal(&review,&[selection(&canceled)]);
        assert!(matches!(result,Err(Fault::Sql(e)) if e.sqlite_error_code()==Some(rusqlite::ErrorCode::DatabaseBusy)));
        read_attempt(conn,&temp.owner(),canceled.id())
    }).unwrap();
    assert!(observed == Some(canceled.clone()));
    other
        .retire_terminal(&review, &[selection(&canceled)])
        .unwrap();
    assert!(
        reader
            .attempt(&temp.owner(), canceled.id())
            .unwrap()
            .is_none()
    );
}
#[test]
fn addressed_malformed_terminal_fails_closed_without_scanning_unrelated_history() {
    let temp = Temp::new();
    let mut s = temp.store();
    let base = seed(&temp, &mut s);
    let id = writer();
    s.conn
        .execute(
            "INSERT INTO retired_attempts(id,body) VALUES(?1,'{}')",
            [&id],
        )
        .unwrap();
    // Historical diagnostics are local to the addressed row; no hidden live attempt exists.
    assert!(s.active(&temp.owner()).unwrap() == Some(base));
    assert!(s.retired_attempt(&temp.owner(), &id).is_err());
}
