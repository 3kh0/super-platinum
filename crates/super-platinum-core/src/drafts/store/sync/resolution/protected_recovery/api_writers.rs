//! Registered input origins and exact idempotent writer receipts. One retained
//! receipt per writer; a processed number alone never proves matching content.
use super::*;
use crate::drafts::sync::ConsumeIntent;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    version: u32,
    scope: Scope,
    writer: String,
    pub(super) origin: WriterOrigin,
    pub(super) processed: u64,
    #[serde(deserialize_with = "required_option")]
    last: Option<Accepted>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Accepted {
    serial: u64,
    local: LocalDraft,
    #[serde(deserialize_with = "required_option")]
    direct: Option<ConsumeIntent>,
    #[serde(deserialize_with = "required_option")]
    continuation: Option<Continuation>,
    preserved: bool,
    result: views::InventoryRecord,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Continuation {
    pub(super) generation: String,
    pub(super) predecessor: u64,
    pub(super) expected: LocalDraft,
}
fn same_continuation(a: Option<&Continuation>, b: Option<&Continuation>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.generation == b.generation
                && a.predecessor == b.predecessor
                && same_local(&a.expected, &b.expected)
        }
        _ => false,
    }
}
pub(super) fn valid_origin(origin: &WriterOrigin) -> bool {
    match origin {
        WriterOrigin::Enrollment(id) | WriterOrigin::LocalGeneration(id) => valid_uuid(id),
        WriterOrigin::Legacy => true,
    }
}
pub(super) fn records(conn: &Connection) -> Result<BTreeMap<String, Record>, AppError> {
    adapter_schema::require(conn)?;
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(writer_id AS BLOB))+LENGTH(CAST(destination AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_writer_lineage",
    )?;
    let mut q = conn.prepare("SELECT writer_id,destination,content FROM composer_draft_writer_lineage ORDER BY writer_id")?;
    let mut out = BTreeMap::new();
    for raw in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, key, raw) = raw?;
        let shape: serde_json::Value = wire::strict(&raw)?;
        if shape.get("last").is_none() {
            return Err(invalid("missing writer receipt guard"));
        }
        if shape
            .get("last")
            .is_some_and(|v| !v.is_null() && v.get("direct").is_none())
        {
            return Err(invalid("missing direct input guard"));
        }
        let record: Record = wire::strict(&raw)?;
        if record.version != 1
            || record.writer != id
            || !valid_uuid(&id)
            || !valid_uuid(&record.scope.account_id)
            || serde_json::from_str::<DraftKey>(&key)? != record.scope.key
            || !valid_origin(&record.origin)
            || record.origin == WriterOrigin::Legacy
            || (record.processed == 0) != record.last.is_none()
        {
            return Err(invalid("invalid writer lineage"));
        }
        sql_number(record.processed)?;
        if let Some(last) = &record.last {
            let result = last.result.restore()?;
            if last.continuation.as_ref().is_some_and(|c| {
                !valid_uuid(&c.generation) || c.predecessor >= last.serial || last.direct.is_some()
            }) {
                return Err(invalid("invalid local continuation receipt"));
            }
            if last.serial != record.processed
                || result.scope() != &record.scope
                || last
                    .direct
                    .as_ref()
                    .is_some_and(|intent| intent.scope != record.scope)
            {
                return Err(invalid("writer receipt identity changed"));
            }
            if last.preserved
                && !result.alternates().iter().any(|a| {
                    a.writer_id == id
                        && a.revision == last.serial
                        && same_local(&a.draft, &last.local)
                })
            {
                return Err(invalid("preserved receipt lost submitted input"));
            }
        }
        if out.insert(id, record).is_some() {
            return Err(invalid("duplicate writer identity"));
        }
    }
    Ok(out)
}
pub(super) fn write(conn: &Connection, record: &Record) -> Result<(), AppError> {
    conn.execute("INSERT INTO composer_draft_writer_lineage VALUES(?1,?2,?3) ON CONFLICT(writer_id) DO UPDATE SET content=excluded.content",
        params![record.writer,serde_json::to_string(&record.scope.key)?,serde_json::to_string(record)?])?;
    records(conn)?;
    Ok(())
}
pub(super) fn origins(
    conn: &Connection,
    scope: &Scope,
    alternates: &[LocalAlternate],
) -> Result<Vec<VersionOrigin>, AppError> {
    let records = records(conn)?;
    alternates
        .iter()
        .map(|a| {
            let origin = if let Some(r) = records.get(&a.writer_id) {
                if r.scope != *scope {
                    return Err(invalid("alternate writer destination changed"));
                }
                r.origin.clone()
            } else {
                WriterOrigin::Legacy
            };
            Ok(VersionOrigin {
                writer: a.writer_id.clone(),
                origin,
            })
        })
        .collect()
}
pub(super) fn fresh(
    conn: &Connection,
    scope: &Scope,
    writer: &LocalWriter,
    origin: WriterOrigin,
) -> Result<(), AppError> {
    if !valid_origin(&origin)
        || origin == WriterOrigin::Legacy
        || records(conn)?.contains_key(writer.id())
    {
        return Err(invalid("invalid new writer origin"));
    }
    write(
        conn,
        &Record {
            version: 1,
            scope: scope.clone(),
            writer: writer.id().into(),
            origin,
            processed: 0,
            last: None,
        },
    )
}
pub(super) fn register(store: &DraftStore, expected: &Review) -> Result<Writer, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    let current = views::exact(&tx, expected)?;
    let writer = LocalWriter::default();
    fresh(
        &tx,
        current.scope(),
        &writer,
        WriterOrigin::Enrollment(current.row.lineage().enrollment().into()),
    )
    .map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    Ok(Writer {
        scope: current.scope().clone(),
        enrollment: current.row.lineage().enrollment().into(),
        writer,
    })
}
pub(super) fn floor(
    conn: &Connection,
    scope: &Scope,
    writer: &LocalWriter,
) -> Result<u64, AppError> {
    let raw: Option<i64> = conn.query_row("SELECT revision FROM composer_draft_reviewed_writers WHERE destination=?1 AND writer_id=?2",
        params![serde_json::to_string(&scope.key)?,writer.id()], |r| r.get(0)).optional()?;
    match raw {
        None => Ok(0),
        Some(n) if n > 0 => Ok(n as u64),
        _ => Err(invalid("invalid reviewed writer floor")),
    }
}
pub(super) fn matching(conn: &Connection, writer: &Writer) -> Result<Record, Error> {
    let record = records(conn)
        .map_err(|_| Error::Storage)?
        .remove(writer.id())
        .ok_or(Error::Stale)?;
    if record.scope != writer.scope
        || record.origin != WriterOrigin::Enrollment(writer.enrollment.clone())
    {
        return Err(Error::Stale);
    }
    Ok(record)
}
pub(super) enum Retry {
    New,
    Applied(LocalInventory),
    Preserved(LocalInventory),
    Retired,
}
pub(super) fn retry(
    conn: &Connection,
    record: &Record,
    writer: &LocalWriter,
    serial: u64,
    local: &LocalDraft,
    direct: Option<&ConsumeIntent>,
    continuation: Option<&Continuation>,
) -> Result<Retry, Error> {
    if serial == 0 || serial > i64::MAX as u64 {
        return Err(Error::Stale);
    }
    if serial <= floor(conn, &record.scope, writer).map_err(|_| Error::Storage)? {
        return Ok(Retry::Retired);
    }
    if serial > record.processed {
        return Ok(Retry::New);
    }
    let last = record.last.as_ref().ok_or(Error::Stale)?;
    if serial != last.serial
        || !same_local(&last.local, local)
        || last.direct.as_ref() != direct
        || !same_continuation(last.continuation.as_ref(), continuation)
    {
        return Err(Error::Stale);
    }
    let result = last.result.restore().map_err(|_| Error::Storage)?;
    if last.preserved {
        Ok(Retry::Preserved(result))
    } else {
        Ok(Retry::Applied(result))
    }
}
pub(super) fn checkpoint(
    store: &DraftStore,
    writer: &Writer,
    serial: u64,
    expected: Option<&Review>,
    local: &LocalDraft,
    direct: Option<&ConsumeIntent>,
) -> Result<WriteResult, Error> {
    if direct.is_some_and(|intent| intent.scope != writer.scope) {
        return Err(Error::Stale);
    }
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    let mut record = matching(&tx, writer)?;
    match retry(&tx, &record, &writer.writer, serial, local, direct, None)? {
        Retry::Applied(result) => return Ok(WriteResult::Applied(views::managed(result)?)),
        Retry::Preserved(result) => return Ok(WriteResult::Preserved(result)),
        Retry::Retired => return Ok(WriteResult::Retired),
        Retry::New => (),
    }
    let current = views::local(&tx, &writer.scope).map_err(|_| Error::Storage)?;
    let expected = expected.filter(|e| {
        e.scope() == &writer.scope && e.row.lineage().enrollment() == writer.enrollment
    });
    if expected.is_some_and(|e| e.uncommitted.is_some()) {
        return Err(Error::Pending);
    }
    let applied = if let (Some(expected), Some(row)) = (expected, current.row()) {
        row.lineage().enrollment() == writer.enrollment
            && views::managed(current.clone())?.same(expected)
    } else {
        false
    };
    if applied {
        storage::checkpoint(store, &expected.unwrap().inner, local, direct)
            .map_err(|_| Error::Storage)?;
    } else {
        // A stale enrollment is never rebased to its successor. Its original
        // writer and serial remain visible as a version the user can review.
        preserve_owned_alternate(&tx, &writer.scope, &writer.writer, serial, local)
            .map_err(|_| Error::Storage)?;
    }
    let result = views::local(&tx, &writer.scope).map_err(|_| Error::Storage)?;
    if !applied
        && !result.alternates().iter().any(|a| {
            a.writer_id == writer.id() && a.revision == serial && same_local(&a.draft, local)
        })
    {
        return Err(Error::Stale);
    }
    record.processed = serial;
    record.last = Some(Accepted {
        serial,
        local: local.clone(),
        direct: direct.cloned(),
        continuation: None,
        preserved: !applied,
        result: views::InventoryRecord::capture(&result).map_err(|_| Error::Storage)?,
    });
    write(&tx, &record).map_err(|_| Error::Storage)?;
    let result = if applied {
        WriteResult::Applied(views::managed(result)?)
    } else {
        WriteResult::Preserved(result)
    };
    faults::before_commit(faults::FaultPoint::WriterCommit).map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    Ok(result)
}

/// Validate the immutable writer receipt retained by a protected slash binding.
/// This is historical evidence, never a renewed writer or dispatch capability.
pub(super) fn slash_receipt_matches(
    raw: &str,
    writer: &str,
    serial: u64,
    expected: &Review,
) -> Result<bool, AppError> {
    let record: Record = wire::strict(raw)?;
    let Some(last) = record.last.as_ref() else {
        return Ok(false);
    };
    let result = views::managed(last.result.restore()?)
        .map_err(|_| invalid("retained slash writer has no managed result"))?;
    Ok(record.version == 1
        && record.scope == *expected.scope()
        && record.writer == writer
        && valid_uuid(writer)
        && serial > 0
        && record.origin == WriterOrigin::Enrollment(expected.row().lineage().enrollment().into())
        && record.processed == serial
        && last.serial == serial
        && last.direct.is_none()
        && last.continuation.is_none()
        && !last.preserved
        && same_local(&last.local, expected.local())
        && result.same(expected))
}

pub(super) fn accepted(
    conn: &Connection,
    record: &mut Record,
    serial: u64,
    local: &LocalDraft,
    preserved: bool,
    result: &LocalInventory,
    continuation: Continuation,
) -> Result<(), AppError> {
    record.processed = serial;
    record.last = Some(Accepted {
        serial,
        local: local.clone(),
        direct: None,
        continuation: Some(continuation),
        preserved,
        result: views::InventoryRecord::capture(result)?,
    });
    write(conn, record)
}
pub(super) fn local_record(conn: &Connection, lease: &LocalWriterLease) -> Result<Record, Error> {
    let record = records(conn)
        .map_err(|_| Error::Storage)?
        .remove(lease.writer.id())
        .ok_or(Error::Stale)?;
    if record.scope != lease.scope {
        return Err(Error::Stale);
    }
    Ok(record)
}
