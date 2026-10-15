//! Local-only continuation cannot borrow an archived enrollment as authority.
use super::*;

fn install(
    conn: &Connection,
    scope: &Scope,
    writer: &LocalWriter,
    generation: &str,
    serial: u64,
) -> Result<(), AppError> {
    adapter_schema::touch_local_if_installed(conn, &scope.key)?;
    let changed = conn.execute("UPDATE composer_draft_local_clock SET generation=?2,writer_id=?3,serial=?4 WHERE destination=?1",
        params![serde_json::to_string(&scope.key)?,generation,writer.id(),sql_number(serial)?])?;
    if changed != 1 {
        return Err(invalid("local continuation clock missing"));
    }
    Ok(())
}
fn available(view: &LocalInventory) -> Result<(), Error> {
    if view.uncommitted.is_some() {
        return Err(Error::Pending);
    }
    if view.row().is_some() {
        return Err(Error::Stale);
    }
    if view.blocked().is_some() || !view.alternates().is_empty() {
        return Err(Error::ReviewRequired);
    }
    Ok(())
}
pub(super) fn renew(
    store: &DraftStore,
    expected: &LocalInventory,
) -> Result<LocalWriterLease, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    available(expected)?;
    let current = views::local(&tx, expected.scope()).map_err(|_| Error::Storage)?;
    if !current.same(expected) {
        return Err(Error::Stale);
    }
    available(&current)?;
    let writer = LocalWriter::default();
    let generation = uuid::Uuid::new_v4().to_string();
    writers::fresh(
        &tx,
        current.scope(),
        &writer,
        WriterOrigin::LocalGeneration(generation.clone()),
    )
    .map_err(|_| Error::Storage)?;
    install(&tx, current.scope(), &writer, &generation, 0).map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    Ok(LocalWriterLease {
        scope: current.scope().clone(),
        writer,
        generation,
        initial_serial: 0,
    })
}
pub(super) fn start(
    store: &DraftStore,
    release: &Release,
    writer: &Writer,
) -> Result<LocalWriterLease, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    let release = &release.0;
    if release.scope != writer.scope || release.enrollment != writer.enrollment {
        return Err(Error::Stale);
    }
    let record = writers::matching(&tx, writer)?;
    let current = views::local(&tx, &writer.scope).map_err(|_| Error::Storage)?;
    available(&current)?;
    if !same_local(current.local(), &release.local) {
        return Err(Error::Stale);
    }
    let key = serde_json::to_string(&writer.scope.key).map_err(|_| Error::Storage)?;
    let head: Option<String> = tx
        .query_row(
            "SELECT enrollment FROM composer_draft_release_head WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()
        .map_err(|_| Error::Storage)?;
    if head.as_deref() != Some(writer.enrollment()) {
        return Err(Error::Stale);
    }
    // Head removal and lease creation are one transaction; a lost reply cannot
    // reuse the Release, and an explicit renewal revokes the unseen lease.
    tx.execute(
        "DELETE FROM composer_draft_release_head WHERE destination=?1",
        [&key],
    )
    .map_err(|_| Error::Storage)?;
    let generation = uuid::Uuid::new_v4().to_string();
    install(
        &tx,
        &writer.scope,
        &writer.writer,
        &generation,
        record.processed,
    )
    .map_err(|_| Error::Storage)?;
    faults::before_commit(faults::FaultPoint::StartLocalCommit).map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    Ok(LocalWriterLease {
        scope: writer.scope.clone(),
        writer: writer.writer.clone(),
        generation,
        initial_serial: record.processed,
    })
}
pub(super) fn save(
    store: &DraftStore,
    lease: &LocalWriterLease,
    predecessor: u64,
    serial: u64,
    expected: &LocalDraft,
    input: &LocalDraft,
) -> Result<LocalWriteResult, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    if serial <= lease.initial_serial || serial <= predecessor {
        return Err(Error::Stale);
    }
    let mut record = writers::local_record(&tx, lease)?;
    let continuation = writers::Continuation {
        generation: lease.generation.clone(),
        predecessor,
        expected: expected.clone(),
    };
    match writers::retry(
        &tx,
        &record,
        &lease.writer,
        serial,
        input,
        None,
        Some(&continuation),
    )? {
        writers::Retry::Applied(view) => return Ok(LocalWriteResult::Applied(view)),
        writers::Retry::Preserved(view) => return Ok(LocalWriteResult::Preserved(view)),
        writers::Retry::Retired => return Ok(LocalWriteResult::Retired),
        writers::Retry::New => (),
    }
    let before = views::local(&tx, &lease.scope).map_err(|_| Error::Storage)?;
    let (_, generation, writer, committed) =
        views::clock(&tx, &lease.scope).map_err(|_| Error::Storage)?;
    let applied = generation.as_deref() == Some(&lease.generation)
        && writer.as_deref() == Some(lease.writer.id())
        && committed == predecessor
        && record.processed == predecessor
        && before.row().is_none()
        && before.blocked().is_none()
        && before.alternates().is_empty()
        && same_local(before.local(), expected);
    if applied {
        write_local(&tx, &lease.scope.key, input).map_err(|_| Error::Storage)?;
        // The canonical write invalidated the lease through the common helper
        // (and SQL trigger). Reinstate only the exact validated predecessor.
        install(&tx, &lease.scope, &lease.writer, &lease.generation, serial)
            .map_err(|_| Error::Storage)?;
    } else {
        preserve_owned_alternate(&tx, &lease.scope, &lease.writer, serial, input)
            .map_err(|_| Error::Storage)?;
    }
    let after = views::local(&tx, &lease.scope).map_err(|_| Error::Storage)?;
    if !applied
        && !after.alternates().iter().any(|a| {
            a.writer_id == lease.writer.id() && a.revision == serial && same_local(&a.draft, input)
        })
    {
        return Err(Error::Stale);
    }
    writers::accepted(
        &tx,
        &mut record,
        serial,
        input,
        !applied,
        &after,
        continuation,
    )
    .map_err(|_| Error::Storage)?;
    faults::before_commit(faults::FaultPoint::LocalCommit).map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    Ok(if applied {
        LocalWriteResult::Applied(after)
    } else {
        LocalWriteResult::Preserved(after)
    })
}
