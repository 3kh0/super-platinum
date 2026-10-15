//! Remote-specific immutable archive; local v1 archive codec stays unchanged.
use super::*;
pub(in super::super) fn preserve(
    store: &DraftStore,
    choice: &Choice,
) -> Result<Finished, AppError> {
    let tx = begin(store)?;
    let prior: Job = wire::strict(&choice.raw)?;
    let request = serde_json::to_string(&(&choice.raw, &choice.decision))?;
    if let Some(result) = disk_store::archived(&tx, &prior.scope, &prior.intent, &request)? {
        tx.commit()?;
        return Ok(result);
    }
    let mut job = disk_store::exact(&tx, &choice.raw)?;
    let current = cleanup::validate_choice(&tx, &job, choice)?;
    if choice.decision.action.cleanup() {
        return Err(bad());
    }
    let remote = positive(&job)?;
    if current.row().state().desired() != Some(&remote.snapshot) {
        return Err(bad());
    }
    let state = resolve_reviewed_remote(
        current.row().state(),
        &remote,
        current.row().state().desired().cloned(),
    )
    .map_err(|_| bad())?;
    let mut row = successor(&current.row().inner, state)?;
    row.ownership.basis = StoredBasis::Remote;
    row.ownership.verified_epoch = job.team.epoch;
    job.decision = Some(Decision {
        id: choice.decision.id.clone(),
        action: choice.decision.action,
    });
    let raw = disk_store::cas(&tx, &choice.raw, &mut job)?;
    let fence = Fence {
        view: capture(&current)?,
        writer: job.original.writer.clone(),
        serial: job.original.serial,
        receipt: job.original.receipt.clone(),
    };
    let result = write(&tx, &job, &raw, request, fence, &row, &choice.stream)?;
    if !choice.stream.is_live() {
        return Err(bad());
    }
    tx.commit()?;
    Ok(result)
}
pub(in super::super) fn archive_cleanup(
    store: &DraftStore,
    stream: &Stream,
    expected: &Review,
    writer: &Writer,
    serial: u64,
    job_raw: &str,
) -> Result<Finished, AppError> {
    let tx = begin(store)?;
    let prior: Job = wire::strict(job_raw)?;
    let request = serde_json::to_string(&(job_raw, capture(expected)?, writer.id(), serial))?;
    if let Some(result) = disk_store::archived(&tx, &prior.scope, &prior.intent, &request)? {
        tx.commit()?;
        return Ok(result);
    }
    let job = disk_store::exact(&tx, job_raw)?;
    if job.stage != Stage::CleanupDone
        || !cleanup::acknowledged(&job, job.result.as_ref().ok_or_else(bad)?)?
    {
        return Err(bad());
    }
    let applied = restore(job.reduced_after.as_ref().ok_or_else(bad)?)?;
    let current = exact(&tx, expected)?;
    clean(&current)?;
    if current.row().lineage().enrollment() != job.enrollment
        || !current
            .row()
            .state()
            .matches_slash_cleanup_followup(applied.row().state())
        || current.row().revision() < applied.row().revision()
        || current.row().inner.ownership != applied.row().inner.ownership
    {
        return Err(bad());
    }
    let receipt = full_writer(&tx, &current, writer, serial)?;
    let t = team(&tx, stream, &job.scope)?;
    cleanup_archive_team(&job, &t)?;
    if t.pending.is_some()
        || readback::unresolved(&tx, stream.team())?
        || recovery::unresolved(&tx, stream.team())?
    {
        return Err(bad());
    }
    inventory::local_identity(&tx, &current.inner)?;
    let fence = Fence {
        view: capture(&current)?,
        writer: writer.id().into(),
        serial,
        receipt,
    };
    let row = successor(&current.row().inner, current.row().state().clone())?;
    let result = write(&tx, &job, job_raw, request, fence, &row, stream)?;
    if !stream.is_live() {
        return Err(bad());
    }
    tx.commit()?;
    Ok(result)
}
fn write(
    conn: &Connection,
    job: &Job,
    job_raw: &str,
    request: String,
    current: Fence,
    row: &ProtectedRow,
    stream: &Stream,
) -> Result<Finished, AppError> {
    if !history::active(conn, &job.scope, &job.intent, &job.binding)? {
        return Err(bad());
    }
    let (terminal, _) = completion::settlement_raw(conn, &job.scope, &job.intent)?;
    let retired = slash_journal::protected_retire(conn, &job.scope.account_id, &terminal)
        .map_err(|_| bad())?;
    storage::write_row(conn, row)?;
    let after = views::review(conn, &job.scope).map_err(|_| bad())?;
    let a = Archive {
        version: 1,
        scope: job.scope.clone(),
        intent: job.intent.clone(),
        enrollment: job.enrollment.clone(),
        binding: job.binding.clone(),
        job: job_raw.into(),
        request,
        current,
        team: team(conn, stream, &job.scope)?,
        after: capture(&after)?,
        retired,
    };
    let raw = serde_json::to_string(&a)?;
    if raw.len() > 4 * 1024 * 1024 {
        return Err(bad());
    }
    conn.execute(
        "INSERT INTO composer_draft_slash_remote_archives VALUES(?1,?2,?3,?4,?5)",
        params![
            a.intent,
            serde_json::to_string(&a.scope.key)?,
            a.enrollment,
            a.binding,
            raw
        ],
    )?;
    if conn.execute("DELETE FROM composer_draft_slash_bindings WHERE intent=?1 AND destination=?2 AND enrollment=?3 AND content=?4",params![a.intent,serde_json::to_string(&a.scope.key)?,a.enrollment,a.binding])?!=1{return Err(bad())}
    disk_store::audit(conn, &job.scope.account_id)?;
    Ok(Finished { raw })
}
