use super::*;

fn busy<T>(result: Result<T>) {
    assert!(
        matches!(result,Err(Fault::Sql(error)) if error.sqlite_error_code()==Some(rusqlite::ErrorCode::DatabaseBusy))
    );
}

#[test]
fn read_audit_and_projection_share_snapshot_while_archive_writer_waits() {
    let temp = Temp::new();
    let mut initial = temp.store();
    let base = seed(&temp, &mut initial);
    let (draft, started) = start(&mut initial, &base);
    drop(initial);
    let reader = temp.store();
    let mut writer_store = temp.store();
    writer_store.conn.busy_timeout(Duration::ZERO).unwrap();
    let operation = writer();
    let new_writer = writer();
    let observed = reader
        .read_snapshot(|conn| {
            // Validation already read the owner links. DELETE journaling must prevent
            // this independently valid transaction from committing before projection.
            assert!(!conn.is_autocommit());
            busy(writer_store.archive_and_new(
                &draft,
                &started,
                ArchiveReason::AbandonedOutcomeUnknown,
                &operation,
                &new_writer,
                100,
            ));
            integrity::validate(conn, &temp.account)?;
            active(conn, &temp.owner())
        })
        .unwrap();
    assert!(observed == Some(draft.clone()));
    assert!(reader.conn.is_autocommit());
    let receipt = writer_store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &operation,
            &new_writer,
            100,
        )
        .unwrap();
    let next = reader.active(&temp.owner()).unwrap().unwrap();
    assert_eq!(next.id(), receipt.new_draft_id());
    assert!(reader.archived(&temp.owner()).unwrap()[0].id() == draft.id());
}

#[test]
fn failed_projection_closes_read_snapshot_and_does_not_block_later_commit() {
    let temp = Temp::new();
    let mut setup = temp.store();
    let base = seed(&temp, &mut setup);
    let (draft, started) = start(&mut setup, &base);
    drop(setup);
    let reader = temp.store();
    let mut writer_store = temp.store();
    writer_store.conn.busy_timeout(Duration::ZERO).unwrap();
    let operation = writer();
    let new_writer = writer();
    let failed: Result<()> = reader.read_snapshot(|conn| {
        assert!(!conn.is_autocommit());
        busy(writer_store.archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &operation,
            &new_writer,
            100,
        ));
        Err(Fault::Stale)
    });
    assert!(matches!(failed, Err(Fault::Stale)));
    assert!(reader.conn.is_autocommit());
    writer_store
        .archive_and_new(
            &draft,
            &started,
            ArchiveReason::AbandonedOutcomeUnknown,
            &operation,
            &new_writer,
            100,
        )
        .unwrap();
    assert!(reader.active(&temp.owner()).unwrap().unwrap().id() != draft.id());
}

#[test]
fn composite_reservation_reads_attempt_and_active_draft_in_one_snapshot() {
    let temp = Temp::new();
    let mut setup = temp.store();
    let base = seed(&temp, &mut setup);
    let reservation = setup.reserve(&base, 100).unwrap();
    drop(setup);
    let reader = temp.store();
    let mut writer_store = temp.store();
    writer_store.conn.busy_timeout(Duration::ZERO).unwrap();
    // Same helper used by reservation(): interpose after reading its attempt.
    let observed = reader
        .read_snapshot(|conn| {
            let attempt =
                read_attempt(conn, &temp.owner(), reservation.attempt.id())?.ok_or(Fault::Stale)?;
            busy(writer_store.cancel_reserved(&reservation));
            let draft = active(conn, &temp.owner())?.ok_or(Fault::Stale)?;
            Ok(Reservation { draft, attempt })
        })
        .unwrap();
    assert!(observed == reservation);
    assert!(
        reader
            .reservation(&temp.owner(), reservation.attempt.id())
            .unwrap()
            == reservation
    );
    writer_store.cancel_reserved(&reservation).unwrap();
    assert!(matches!(
        reader.reservation(&temp.owner(), reservation.attempt.id()),
        Err(Fault::Blocked)
    ));
    assert!(reader.conn.is_autocommit());
}
