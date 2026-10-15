use super::*;
struct FileDb {
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl FileDb {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("slash-journal-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("owned.sqlite");
        Self { dir, path }
    }
    fn open(&self) -> Connection {
        let conn = Connection::open(&self.path).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        conn.pragma_update(None, "synchronous", "FULL").unwrap();
        conn
    }
}
impl Drop for FileDb {
    fn drop(&mut self) {
        // Only exact owned files; never recurse or touch the user's database.
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.dir.join("owned.sqlite-journal"));
        let _ = std::fs::remove_dir(&self.dir);
    }
}
#[test]
fn committed_dispatch_survives_connection_loss_and_restore_never_replays() {
    let file = FileDb::new();
    let p = prepared("T1", "C1");
    let d = armed(&p);
    {
        let mut conn = file.open();
        install(&mut conn, ACCOUNT).unwrap();
        apply(&mut conn, None, &p, &now(0)).unwrap();
        apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
    }
    let mut conn = file.open();
    let retained = only(&mut conn);
    assert!(retained == d);
    assert!(Controller::restore(retained).unwrap().recovery_required());
    assert_eq!(
        apply(&mut conn, Some(&p), &d, &now(2)),
        Err(Failure::NoReplay)
    );
}
#[test]
fn independent_connections_serialize_competing_dispatches() {
    let file = FileDb::new();
    let a = prepared("T1", "C1");
    let b = prepared("T1", "C2");
    {
        let mut conn = file.open();
        install(&mut conn, ACCOUNT).unwrap();
        apply(&mut conn, None, &a, &now(0)).unwrap();
        apply(&mut conn, None, &b, &now(0)).unwrap();
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [a, b]
        .into_iter()
        .map(|record| {
            let path = file.path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut conn = Connection::open(path).unwrap();
                conn.busy_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
                conn.pragma_update(None, "synchronous", "FULL").unwrap();
                barrier.wait();
                apply(&mut conn, Some(&record), &armed(&record), &now(1))
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|v| v.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|v| v.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|v| **v == Err(Failure::NoReplay))
            .count(),
        1
    );
    let mut conn = file.open();
    assert_eq!(
        load(&mut conn, ACCOUNT)
            .unwrap()
            .iter()
            .filter(|v| matches!(v.phase(), Phase::DispatchPossible))
            .count(),
        1
    );
}
#[test]
fn sqlite_failure_does_not_partially_advance_receipt_version() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    apply(&mut conn, None, &p, &now(0)).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_update BEFORE UPDATE ON sp_slash_intents BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert_eq!(
        apply(&mut conn, Some(&p), &armed(&p), &now(1)),
        Err(Failure::Persistence)
    );
    assert!(only(&mut conn) == p);
}
#[test]
fn no_automatic_install_and_unsafe_synchronous_mode_cannot_admit() {
    let mut empty = Connection::open_in_memory().unwrap();
    assert!(load(&mut empty, ACCOUNT).is_err());
    assert_eq!(
        empty
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let mut conn = db();
    conn.pragma_update(None, "synchronous", "OFF").unwrap();
    assert_eq!(
        apply(&mut conn, None, &prepared("T1", "C1"), &now(0)),
        Err(Failure::Persistence)
    );
}

#[test]
fn disk_database_with_disabled_rollback_journal_cannot_admit() {
    let file = FileDb::new();
    let mut conn = file.open();
    install(&mut conn, ACCOUNT).unwrap();
    conn.pragma_update(None, "journal_mode", "OFF").unwrap();
    assert_eq!(
        apply(&mut conn, None, &prepared("T1", "C1"), &now(0)),
        Err(Failure::Persistence)
    );
}
#[test]
fn queue_blocks_new_admission_for_pending_input_but_preserves_terminal_receipt() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    let (reply, receive) = tokio::sync::oneshot::channel();
    Command::Compare {
        account: ACCOUNT.into(),
        old: None,
        next: p.clone(),
        reply,
    }
    .execute(Ok(&mut conn), true);
    assert_eq!(receive.blocking_recv().unwrap(), Err(Failure::Stale));
    assert!(load(&mut conn, ACCOUNT).unwrap().is_empty());
    let d = armed(&p);
    let a = ack(&d, None);
    apply(&mut conn, None, &p, &now(0)).unwrap();
    apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
    let (reply, receive) = tokio::sync::oneshot::channel();
    Command::Compare {
        account: ACCOUNT.into(),
        old: Some(d),
        next: a.clone(),
        reply,
    }
    .execute(Ok(&mut conn), true);
    assert_eq!(receive.blocking_recv().unwrap(), Ok(()));
    assert!(only(&mut conn) == a);
}
#[test]
fn dropping_queue_reply_cannot_cancel_a_committed_dispatch_or_authorize_replay() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    let tick = stamp().unwrap();
    apply(&mut conn, None, &p, &tick).unwrap();
    let d = armed(&p);
    let (reply, receive) = tokio::sync::oneshot::channel();
    drop(receive);
    Command::Compare {
        account: ACCOUNT.into(),
        old: Some(p.clone()),
        next: d.clone(),
        reply,
    }
    .execute(Ok(&mut conn), false);
    assert!(only(&mut conn) == d);
    assert_eq!(
        compare(&mut conn, ACCOUNT, Some(&p), &d),
        Err(Failure::NoReplay)
    );
}
#[test]
fn bounded_inventory_rejects_oversized_corruption_before_next_write() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    apply(&mut conn, None, &p, &now(0)).unwrap();
    conn.execute("UPDATE sp_slash_intents SET body=zeroblob(1048577)", [])
        .unwrap();
    assert!(load(&mut conn, ACCOUNT).is_err());
    assert!(apply(&mut conn, None, &prepared("T2", "C2"), &now(1)).is_err());
}
