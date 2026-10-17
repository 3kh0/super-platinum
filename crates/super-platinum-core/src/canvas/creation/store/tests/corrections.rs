use super::*;

#[test]
fn late_capture_survives_other_connection_discard_without_recreating_base() {
    let temp = Temp::new();
    let mut first = temp.store();
    let base = seed(&temp, &mut first);
    let mut other = temp.store();
    first.discard(&base).unwrap();
    assert!(other.draft(&temp.owner(), base.id()).unwrap().is_none());
    let captured = content("Captured after the close started");
    let saved = other
        .preserve_conflict(&base, &writer(), &writer(), captured.clone())
        .unwrap();
    assert!(saved.content() == &captured);
    assert!(other.active(&temp.owner()).unwrap().is_none());
    assert!(other.draft(&temp.owner(), base.id()).unwrap().is_none());
    let replacement = first
        .create(&temp.owner(), &writer(), content("Replacement"))
        .unwrap();
    assert!(matches!(
        first.reserve(&replacement, 100),
        Err(Fault::Blocked)
    ));
    drop(other);
    drop(first);
    let mut fresh = temp.store();
    assert!(fresh.conflicts(&temp.owner()).unwrap() == vec![saved.clone()]);
    fresh.remove_conflict(&saved).unwrap();
    assert!(fresh.reserve(&replacement, 100).is_ok());
}
#[test]
fn late_capture_refuses_other_account_and_does_not_overwrite_newer_live_base() {
    let temp = Temp::new();
    let mut store = temp.store();
    let base = seed(&temp, &mut store);
    let newer = store
        .save(&base, &writer(), content("Newer canonical"))
        .unwrap();
    let conflict = store
        .preserve_conflict(&base, &writer(), &writer(), content("Old visit local edit"))
        .unwrap();
    assert!(store.active(&temp.owner()).unwrap() == Some(newer));
    let foreign = Temp::new();
    let mut foreign_store = foreign.store();
    assert!(matches!(
        foreign_store.preserve_conflict(&base, &writer(), &writer(), content("wrong")),
        Err(Fault::Invalid)
    ));
    assert_eq!(count(&foreign_store.conn, "conflicts").unwrap(), 0);
    assert!(store.conflicts(&temp.owner()).unwrap() == vec![conflict]);
}
#[test]
fn disposed_operation_cannot_resurrect_from_second_process_or_restart() {
    let temp = Temp::new();
    let mut first = temp.store();
    let base = seed(&temp, &mut first);
    let mut other = temp.store();
    let op = writer();
    let w = writer();
    let local = content("Unsaved native capture");
    let saved = first
        .preserve_conflict(&base, &w, &op, local.clone())
        .unwrap();
    other.remove_conflict(&saved).unwrap();
    assert!(matches!(
        first.preserve_conflict(&base, &w, &op, local.clone()),
        Err(Fault::Retired)
    ));
    assert!(matches!(
        first.preserve_conflict(&base, &w, &op, content("Different")),
        Err(Fault::Retired)
    ));
    assert!(matches!(first.remove_conflict(&saved), Err(Fault::Retired)));
    assert!(first.conflicts(&temp.owner()).unwrap().is_empty());
    let tombstone: String = first
        .conn
        .query_row("SELECT body FROM dispositions WHERE id=?1", [&op], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(!tombstone.contains("Unsaved native capture") && !tombstone.contains("content"));
    drop(other);
    drop(first);
    let mut fresh = temp.store();
    assert!(matches!(
        fresh.preserve_conflict(&base, &w, &op, local),
        Err(Fault::Retired)
    ));
    assert!(fresh.reserve(&base, 100).is_ok());
}
#[test]
fn disposal_transaction_failure_keeps_live_conflict_and_can_retry() {
    let temp = Temp::new();
    let mut store = temp.store();
    let base = seed(&temp, &mut store);
    let conflict = store
        .preserve_conflict(&base, &writer(), &writer(), content("Retain me"))
        .unwrap();
    store.conn.execute_batch("CREATE TEMP TRIGGER fail_remove BEFORE DELETE ON conflicts BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(store.remove_conflict(&conflict).is_err());
    assert!(store.conflicts(&base.owner).unwrap() == vec![conflict.clone()]);
    assert_eq!(count(&store.conn, "dispositions").unwrap(), 0);
    store
        .conn
        .execute_batch("DROP TRIGGER fail_remove;")
        .unwrap();
    store.remove_conflict(&conflict).unwrap();
    assert!(store.conflicts(&base.owner).unwrap().is_empty());
}
#[test]
fn reviewed_conflicts_exceed_old_lifetime_cap_without_eviction() {
    let temp = Temp::new();
    let mut store = temp.store();
    let base = seed(&temp, &mut store);
    let first_op = writer();
    let w = writer();
    for index in 0..256 {
        let op = if index == 0 {
            first_op.clone()
        } else {
            writer()
        };
        let c = store
            .preserve_conflict(&base, &w, &op, content(&index.to_string()))
            .unwrap();
        store.remove_conflict(&c).unwrap();
    }
    assert_eq!(count(&store.conn, "dispositions").unwrap(), 256);
    assert!(matches!(
        store.preserve_conflict(&base, &w, &first_op, content("0")),
        Err(Fault::Retired)
    ));
    assert!(store.active(&base.owner).unwrap() == Some(base));
}
#[test]
fn deleted_scope_cannot_hide_started_attempt_or_unlock_new_creation() {
    let temp = Temp::new();
    let mut store = temp.store();
    let base = seed(&temp, &mut store);
    let reservation = store.reserve(&base, 100).unwrap();
    let started = store.admit(&reservation, 100).unwrap().consume();
    let external = Connection::open(temp.path()).unwrap();
    external.execute("DELETE FROM scopes", []).unwrap();
    assert!(matches!(store.active(&base.owner), Err(Fault::Invalid)));
    assert!(matches!(
        store.create(&base.owner, &writer(), content("Bypass")),
        Err(Fault::Invalid)
    ));
    assert!(matches!(
        store.reserve(&reservation.draft, 100),
        Err(Fault::Invalid)
    ));
    assert!(matches!(
        store.admit(&reservation, 100),
        Err(Fault::Invalid)
    ));
    assert_eq!(count(&store.conn, "attempts").unwrap(), 1);
    let raw = read::<Attempt>(&store.conn, &base.owner, &started.id)
        .unwrap()
        .unwrap();
    assert!(raw == started);
    assert!(Store::open(temp.path(), &temp.account).is_err());
}
#[test]
fn broken_active_or_attempt_link_refuses_admission_without_repair() {
    for corruption in [
        "scope-null",
        "missing-draft",
        "attempt-null",
        "forged-archive",
    ] {
        let temp = Temp::new();
        let mut store = temp.store();
        let base = seed(&temp, &mut store);
        let (draft, started) = start(&mut store, &base);
        store.settle(&started, Outcome::Uncertain).unwrap();
        match corruption {
            "scope-null" => {
                store
                    .conn
                    .execute("UPDATE scopes SET body=json_set(body,'$.active',NULL)", [])
                    .unwrap();
            }
            "missing-draft" => {
                store.conn.execute("DELETE FROM drafts", []).unwrap();
            }
            "attempt-null" => {
                store
                    .conn
                    .execute("UPDATE drafts SET body=json_set(body,'$.attempt',NULL)", [])
                    .unwrap();
            }
            _ => {
                let mut archived = draft.clone();
                archived.archived = true;
                archived.decision_revision = 2;
                put(&store.conn, &archived).unwrap();
                let mut scope = scope(&store.conn, &base.owner).unwrap().unwrap();
                scope.active = None;
                put(&store.conn, &scope).unwrap();
            }
        }
        let attempts_before: String = store
            .conn
            .query_row(
                "SELECT body FROM attempts WHERE id=?1",
                [&started.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            store
                .create(&base.owner, &writer(), content("Must remain blocked"))
                .is_err()
        );
        assert!(store.reserve(&draft, 100).is_err());
        let attempts_after: String = store
            .conn
            .query_row(
                "SELECT body FROM attempts WHERE id=?1",
                [&started.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(attempts_before, attempts_after);
        assert!(Store::open(temp.path(), &temp.account).is_err());
    }
}
#[test]
fn missing_archive_decision_cannot_legitimize_orphan_unresolved_attempt() {
    let temp = Temp::new();
    let mut store = temp.store();
    let base = seed(&temp, &mut store);
    let (draft, started) = start(&mut store, &base);
    store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &writer(),
            &writer(),
            100,
        )
        .unwrap();
    let empty = store.active(&base.owner).unwrap().unwrap();
    let next = store.save(&empty, &writer(), content("Next")).unwrap();
    store.conn.execute("DELETE FROM decisions", []).unwrap();
    assert!(matches!(store.reserve(&next, 100), Err(Fault::Invalid)));
    assert_eq!(count(&store.conn, "attempts").unwrap(), 1);
    assert!(Store::open(temp.path(), &temp.account).is_err());
}
