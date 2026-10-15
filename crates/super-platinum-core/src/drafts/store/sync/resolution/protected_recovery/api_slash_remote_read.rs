//! Named read admission bound to an exact terminal command, without a fake gap.
use super::*;
pub(in super::super) fn reserve(
    store: &DraftStore,
    stream: &Stream,
    expected: &Review,
    writer: &Writer,
    serial: u64,
    intent: &str,
    now: u64,
) -> Result<ReadTicket, AppError> {
    let tx = begin(store)?;
    disk_store::require(&tx, &expected.scope().account_id)?;
    disk_store::audit(&tx, &expected.scope().account_id)?;
    let current = exact(&tx, expected)?;
    let base = remote_idle(&current)?.clone();
    let (binding, raw, settlement, attestation) = known(&tx, current.scope(), intent)?;
    let original = restore(&binding.inventory)?;
    let old = original.row().state().acknowledged().ok_or_else(bad)?;
    if binding.enrollment != current.row().lineage().enrollment()
        || old.id != base.id
        || old.client_msg_id != base.client_msg_id
    {
        return Err(bad());
    }
    let receipt = full_writer(&tx, &current, writer, serial)?;
    inventory::acknowledged(&tx, &current.inner)?;
    let mut t = team(&tx, stream, current.scope())?;
    if t.pending.is_some()
        || readback::unresolved(&tx, stream.team())?
        || recovery::unresolved(&tx, stream.team())?
        || disk_store::unresolved(&tx, stream.team())?
    {
        return Err(bad());
    }
    let deadline = t
        .read_retry_at_ms
        .max(recovery::deadline(&tx, stream.team())?);
    if now < deadline {
        return Err(invalid("remote read cooldown"));
    }
    let floor = now.checked_add(READ_FLOOR_MS).ok_or_else(bad)?;
    sql_number(floor)?;
    let pending = next_pending(&mut t, PendingKind::Reconcile, &current.row().inner)?;
    t.read_retry_at_ms = t.read_retry_at_ms.max(floor);
    let job = Job {
        version: 1,
        revision: 1,
        id: pending.id.clone(),
        scope: current.scope().clone(),
        intent: intent.into(),
        enrollment: binding.enrollment,
        binding: raw,
        settlement,
        attestation,
        user: binding.dispatch.owner().user.clone(),
        base,
        original: Fence {
            view: capture(&current)?,
            writer: writer.id().into(),
            serial,
            receipt,
        },
        team: t.clone(),
        pending,
        floor,
        known_deadline: recovery::deadline(&tx, stream.team())?,
        stage: Stage::ReadReserved,
        read: None,
        decision: None,
        cleanup: None,
        result: None,
        reduced_before: None,
        reduced_after: None,
    };
    continuity::reserve(&tx, &job)?;
    storage::write_team(&tx, stream.team(), &t)?;
    let raw = disk_store::insert(&tx, &job)?;
    if !stream.is_live() {
        return Err(bad());
    }
    tx.commit()?;
    Ok(ReadTicket {
        raw,
        stream: stream.clone(),
    })
}
pub(in super::super) fn claim_read(
    store: &DraftStore,
    ticket: ReadTicket,
) -> Result<ReadClaim, AppError> {
    let tx = begin(store)?;
    let mut job = disk_store::exact(&tx, &ticket.raw)?;
    if job.stage != Stage::ReadReserved || team(&tx, &ticket.stream, &job.scope)? != job.team {
        return Err(bad());
    }
    exact(&tx, &restore(&job.original.view)?)?;
    writer_unchanged(&tx, &job.original)?;
    job.stage = Stage::ReadDispatched;
    let raw = disk_store::cas(&tx, &ticket.raw, &mut job)?;
    if !ticket.stream.is_live() {
        return Err(bad());
    }
    tx.commit()?;
    Ok(ReadClaim {
        raw,
        stream: ticket.stream,
    })
}
pub(in super::super) fn retain_read(
    store: &DraftStore,
    claim: &ReadClaim,
    report: &ReadReport,
) -> Result<String, AppError> {
    let tx = begin(store)?;
    let original: Job = wire::strict(&claim.raw)?;
    if original.id != report.id || original.stage != Stage::ReadDispatched {
        return Err(bad());
    }
    let (_, stored) = disk_store::get(&tx, &original.id)?;
    let mut job: Job = wire::strict(&stored)?;
    if job.stage == Stage::ReadDone
        && job.read.as_ref().is_some_and(|r| {
            serde_json::to_string(r).ok() == serde_json::to_string(&report.evidence).ok()
        })
    {
        tx.commit()?;
        return Ok(stored);
    }
    if stored != claim.raw {
        return Err(bad());
    }
    let evidence: ReadEvidence = wire::strict(&serde_json::to_string(&report.evidence)?)?;
    if evidence.user != job.user || (evidence.kind != ReadKind::Response && evidence.body.is_some())
    {
        return Err(bad());
    }
    sql_number(evidence.until)?;
    job.stage = Stage::ReadDone;
    job.read = Some(evidence);
    let raw = disk_store::cas(&tx, &stored, &mut job)?;
    tx.commit()?;
    Ok(raw)
}
