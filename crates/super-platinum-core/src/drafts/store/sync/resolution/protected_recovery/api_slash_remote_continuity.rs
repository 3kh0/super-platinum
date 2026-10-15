//! The named remote read participates in the existing Reconcile ledger.
use super::*;

fn link(job: &Job, record: &ReadRecord) -> Result<(), AppError> {
    if record.team != job.scope.key.team
        || Some(&record.lease_id) != job.team.lease_id.as_ref()
        || record.epoch != job.team.epoch
        || record.enrollment_id != job.enrollment
        || record.sequence != job.pending.sequence
        || record.floor_ms != job.floor
    {
        return Err(bad());
    }
    let known = job.read.as_ref().filter(|r| r.kind != ReadKind::Unknown);
    match record.state {
        ReadState::SupersededUnknown => {
            if record.completion != Some(ReadCompletion::SupersededUnknown)
                || record.retry_at_ms < job.floor.max(known.map_or(0, |r| r.until))
            {
                return Err(bad());
            }
        }
        ReadState::Unresolved => {
            if known.is_some() || record.completion.is_some() || record.retry_at_ms != job.floor {
                return Err(bad());
            }
        }
        ReadState::Finished => {
            let known = known.ok_or_else(bad)?;
            if record.completion != Some(ReadCompletion::RetainedSlashRemote { version: 1 })
                || record.retry_at_ms < job.floor.max(known.until)
            {
                return Err(bad());
            }
        }
    }
    Ok(())
}

pub(super) fn validate_job(conn: &Connection, job: &Job) -> Result<(), AppError> {
    let record = inventory::reads(conn)?.remove(&job.id).ok_or_else(bad)?;
    link(job, &record)
}

// Called by the generic ledger decoder. Deliberately does NOT invoke reads(),
// job get(), full audit, or storage::team: those would recurse into the decoder.
pub(in super::super) fn validate_retained(
    conn: &Connection,
    id: &str,
    record: &ReadRecord,
) -> Result<(), AppError> {
    let (version, account): (i64, String) = conn.query_row(
        "SELECT version,account FROM composer_draft_slash_format WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if version != 3 {
        return Err(bad());
    }
    let (intent,team,raw):(String,String,String)=conn.query_row(
        "SELECT intent,team,CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_remote_jobs WHERE id=?1",
        [id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let job: Job = wire::strict(&raw)?;
    if job.version != 1
        || job.id != id
        || job.intent != intent
        || job.scope.key.team != team
        || job.scope.account_id != account
        || job.pending.id != id
        || job.pending.enrollment_id != job.enrollment
        || job.pending.kind != PendingKind::Reconcile
        || !matches!(
            job.stage,
            Stage::ReadDone
                | Stage::CleanupReserved
                | Stage::CleanupDispatched
                | Stage::CleanupDone
        )
        || record.completion != Some(ReadCompletion::RetainedSlashRemote { version: 1 })
    {
        return Err(bad());
    }
    link(&job, record)
}

pub(super) fn reserve(conn: &Connection, job: &Job) -> Result<(), AppError> {
    let record = ReadRecord {
        team: job.scope.key.team.clone(),
        lease_id: job.team.lease_id.clone().ok_or_else(bad)?,
        epoch: job.team.epoch,
        enrollment_id: job.enrollment.clone(),
        sequence: job.pending.sequence,
        floor_ms: job.floor,
        state: ReadState::Unresolved,
        retry_at_ms: job.floor,
        completion: None,
    };
    // Reservation is insertion-only; never replace an existing admission UUID.
    conn.execute(
        "INSERT INTO composer_draft_continuity_reads(id,team,content) VALUES(?1,?2,?3)",
        params![job.id, record.team, serde_json::to_string(&record)?],
    )?;
    Ok(())
}

// Runs only within the exact ReadDispatched -> ReadDone CAS transaction, after
// the old job/ledger audit and before the new job plus final joint audit.
pub(super) fn retain(conn: &Connection, old: &Job, next: &Job) -> Result<(), AppError> {
    let mut record = inventory::reads(conn)?.remove(&old.id).ok_or_else(bad)?;
    link(old, &record)?;
    let evidence = next.read.as_ref().ok_or_else(bad)?;
    recovery::persist_deadline(conn, &old.scope, evidence.until)?;
    let mut team = storage::team(conn, &old.scope.key.team)?;
    team.read_retry_at_ms = team.read_retry_at_ms.max(old.floor).max(evidence.until);
    if evidence.kind != ReadKind::Unknown {
        record.retry_at_ms = record.retry_at_ms.max(team.read_retry_at_ms);
        if record.state == ReadState::Unresolved {
            record.state = ReadState::Finished;
            record.completion = Some(ReadCompletion::RetainedSlashRemote { version: 1 });
            if team.mode == Mode::Open
                && team.epoch == old.team.epoch
                && team.lease_id == old.team.lease_id
                && team.pending.as_ref() == Some(&old.pending)
            {
                team.pending = None;
            }
        }
        // SupersededUnknown keeps its original recovery provenance and cannot
        // clear a replacement stream's work. Unknown keeps retry exactly floor.
        readback::write_record(conn, &old.id, &record)?;
    }
    storage::write_team(conn, &old.scope.key.team, &team)?;
    Ok(())
}
