use super::*;

#[test]
fn exact_owner_schema_and_corrupt_records_fail_without_repair() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    drop(store);
    assert!(Store::open(temp.path(), &writer()).is_err());
    let conn = Connection::open(temp.path()).unwrap();
    conn.execute_batch("PRAGMA user_version=99;").unwrap();
    drop(conn);
    assert!(Store::open(temp.path(), &temp.account).is_err());
    let conn = Connection::open(temp.path()).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
    conn.execute_batch("PRAGMA user_version=3;").unwrap();
    let mut store = temp.store();
    conn.execute("UPDATE drafts SET body='broken' WHERE id=?1", [draft.id()])
        .unwrap();
    drop(conn);
    assert!(Store::open(temp.path(), &temp.account).is_err());
    assert!(store.active(&temp.owner()).is_err());
    assert!(
        store
            .create(
                &temp.owner(),
                &writer(),
                content("Must not replace corruption")
            )
            .is_err()
    );
    assert_eq!(
        store
            .conn
            .query_row("SELECT body FROM drafts WHERE id=?1", [draft.id()], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
        "broken"
    );
}
#[test]
fn version_zero_foreign_tables_are_not_claimed_or_repaired() {
    let temp = Temp::new();
    let conn = Connection::open(temp.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE foreign_data(value TEXT);INSERT INTO foreign_data VALUES('retained');",
    )
    .unwrap();
    drop(conn);
    assert!(Store::open(temp.path(), &temp.account).is_err());
    let conn = Connection::open(temp.path()).unwrap();
    assert_eq!(
        conn.query_row("SELECT value FROM foreign_data", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "retained"
    );
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn oversize_and_scope_capacity_leave_existing_drafts_unchanged() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    assert!(matches!(
        Content::new("x".repeat(MAX_TITLE_BYTES + 1), String::new()),
        Err(Fault::Capacity)
    ));
    assert!(matches!(
        Content::new(String::new(), "x".repeat(MAX_MARKDOWN_BYTES + 1)),
        Err(Fault::Capacity)
    ));
    for index in 1..MAX_SCOPES {
        let owner = Owner::new(&temp.account, &format!("T{}", index + 1), "U1").unwrap();
        store.create(&owner, &writer(), Content::empty()).unwrap();
    }
    let owner = Owner::new(&temp.account, "T999", "U1").unwrap();
    assert!(matches!(
        store.create(&owner, &writer(), content("Too many")),
        Err(Fault::Capacity)
    ));
    assert!(store.active(&temp.owner()).unwrap() == Some(draft));
}
#[test]
fn bounded_before_parse_rejects_oversize_row_and_preserves_it() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    store
        .conn
        .execute(
            "UPDATE drafts SET body=zeroblob(?2) WHERE id=?1",
            params![draft.id(), (MAX_ROW_BYTES + 1) as i64],
        )
        .unwrap();
    assert!(store.active(&temp.owner()).is_err());
    assert_eq!(
        store
            .conn
            .query_row(
                "SELECT length(body) FROM drafts WHERE id=?1",
                [draft.id()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        (MAX_ROW_BYTES + 1) as i64
    );
}
#[test]
fn failed_reserve_and_settlement_are_atomic_and_retry_only_storage() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    store.conn.execute_batch("CREATE TEMP TRIGGER fail_attempt BEFORE INSERT ON attempts BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(store.reserve(&draft, 100).is_err());
    assert!(store.active(&temp.owner()).unwrap() == Some(draft.clone()));
    assert_eq!(count(&store.conn, "attempts").unwrap(), 0);
    store
        .conn
        .execute_batch("DROP TRIGGER fail_attempt;")
        .unwrap();
    let (_, started) = start(&mut store, &draft);
    store.conn.execute_batch("CREATE TEMP TRIGGER fail_settlement BEFORE UPDATE ON attempts BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(
        store
            .settle(
                &started,
                Outcome::Created {
                    file_id: "F1".into()
                }
            )
            .is_err()
    );
    assert!(store.attempt(&temp.owner(), started.id()).unwrap() == Some(started.clone()));
    store
        .conn
        .execute_batch("DROP TRIGGER fail_settlement;")
        .unwrap();
    store
        .settle(
            &started,
            Outcome::Created {
                file_id: "F1".into(),
            },
        )
        .unwrap();
    assert_eq!(count(&store.conn, "attempts").unwrap(), 1);
}
#[test]
fn conflict_idempotency_and_conflict_after_reservation_do_not_launder_permission() {
    let temp = Temp::new();
    let mut store = temp.store();
    let draft = seed(&temp, &mut store);
    let reservation = store.reserve(&draft, 100).unwrap();
    let op = writer();
    let w = writer();
    let conflict = store
        .preserve_conflict(&draft, &w, &op, content("Native ahead"))
        .unwrap();
    assert!(
        store
            .preserve_conflict(&draft, &w, &op, content("Native ahead"))
            .unwrap()
            == conflict
    );
    assert!(matches!(
        store.preserve_conflict(&draft, &w, &op, content("Different version")),
        Err(Fault::Stale)
    ));
    assert!(matches!(
        store.admit(&reservation, 100),
        Err(Fault::Blocked)
    ));
    store.cancel_reserved(&reservation).unwrap();
    assert!(matches!(
        store.discard(&reservation.draft),
        Err(Fault::Blocked)
    ));
    assert!(store.conflicts(&draft.owner).unwrap() == vec![conflict]);
}
