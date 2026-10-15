//! Explicit exact-identity cleanup. No report/choice constructor in production.
use super::*;
pub(in super::super) fn choose(store: &DraftStore, choice: &Choice) -> Result<(), AppError> {
    let tx = begin(store)?;
    let job = disk_store::exact(&tx, &choice.raw)?;
    validate_choice(&tx, &job, choice)?;
    tx.commit()
}
pub(super) fn validate_choice(
    conn: &Connection,
    job: &Job,
    choice: &Choice,
) -> Result<Review, AppError> {
    if job.stage != Stage::ReadDone || !valid_uuid(&choice.decision.id) {
        return Err(bad());
    }
    let (_, _, _, _) = known(conn, &job.scope, &job.intent)?;
    let (terminal, _) = completion::settlement_raw(conn, &job.scope, &job.intent)?;
    if matches!(
        terminal.phase(),
        CommandPhase::Completed(CommandReply::Rejected { .. })
    ) != choice.decision.action.rejected()
    {
        return Err(bad());
    }
    positive(job)?;
    let current = open_exact(conn, job, &choice.stream)?;
    let until = job.read.as_ref().ok_or_else(bad)?.until;
    // A known rate limit cannot accompany a positive choice. A fresh explicitly
    // admitted read after the durable deadline is required instead.
    if until != 0 {
        return Err(bad());
    }
    Ok(current)
}
pub(in super::super) fn reserve_cleanup(
    store: &DraftStore,
    choice: &Choice,
) -> Result<CleanupTicket, AppError> {
    let tx = begin(store)?;
    let mut job = disk_store::exact(&tx, &choice.raw)?;
    let current = validate_choice(&tx, &job, choice)?;
    if !choice.decision.action.cleanup() {
        return Err(bad());
    }
    let remote = positive(&job)?;
    let mut state = resolve_reviewed_remote(
        current.row().state(),
        &remote,
        current.row().state().desired().cloned(),
    )
    .map_err(|_| bad())?;
    let operation = state
        .reserve_slash_cleanup(&job.scope, &remote, &uuid::Uuid::new_v4().to_string())
        .ok_or_else(bad)?;
    let mut row = successor(&current.row().inner, state)?;
    row.ownership.basis = StoredBasis::Remote;
    row.ownership.verified_epoch = job.team.epoch;
    storage::write_row(&tx, &row)?;
    let after = views::review(&tx, &job.scope).map_err(|_| bad())?;
    job.cleanup = Some(Cleanup {
        operation,
        before: capture(&current)?,
        after: capture(&after)?,
    });
    job.decision = Some(Decision {
        id: choice.decision.id.clone(),
        action: choice.decision.action,
    });
    job.stage = Stage::CleanupReserved;
    let raw = disk_store::cas(&tx, &choice.raw, &mut job)?;
    if !choice.stream.is_live() {
        return Err(bad());
    }
    tx.commit()?;
    Ok(CleanupTicket {
        raw,
        stream: choice.stream.clone(),
    })
}
pub(in super::super) fn claim_cleanup(
    store: &DraftStore,
    ticket: CleanupTicket,
) -> Result<CleanupClaim, AppError> {
    let tx = begin(store)?;
    let mut job = disk_store::exact(&tx, &ticket.raw)?;
    known(&tx, &job.scope, &job.intent)?;
    if job.stage != Stage::CleanupReserved {
        return Err(bad());
    }
    let cleanup = job.cleanup.as_ref().ok_or_else(bad)?;
    exact(&tx, &restore(&cleanup.after)?)?;
    writer_unchanged(&tx, &job.original)?;
    let t = live_settled(&tx, &job, &ticket.stream)?;
    if t.pending.is_some()
        || t.sequence != job.pending.sequence
        || readback::unresolved(&tx, &job.scope.key.team)?
        || recovery::unresolved(&tx, &job.scope.key.team)?
    {
        return Err(bad());
    }
    if !history::active(&tx, &job.scope, &job.intent, &job.binding)? {
        return Err(bad());
    }
    job.stage = Stage::CleanupDispatched;
    let raw = disk_store::cas(&tx, &ticket.raw, &mut job)?;
    if !ticket.stream.is_live() {
        return Err(bad());
    }
    tx.commit()?;
    Ok(CleanupClaim {
        raw,
        stream: ticket.stream,
    })
}
/// Minimal supported positive envelope, derived from existing Delete decoder.
/// Additional metadata is retained as unknown, never guessed/coerced.
pub(super) fn acknowledged(job: &Job, e: &CleanupEvidence) -> Result<bool, AppError> {
    if e.kind != CleanupKind::Response {
        return Ok(false);
    }
    let Some(body) = &e.body else {
        return Ok(false);
    };
    let Ok(value) = crate::slash_commands::codec::decode(body.as_bytes(), 2 * 1024 * 1024) else {
        return Ok(false);
    };
    let Some(object) = value.as_object() else {
        return Ok(false);
    };
    if value.get("ok") != Some(&serde_json::Value::Bool(true))
        || value.get("error").is_some_and(|v| !v.is_null())
    {
        return Ok(false);
    }
    if object.keys().any(|k| {
        !matches!(
            k.as_str(),
            "ok" | "error" | "draft_id" | "client_msg_id" | "team_id" | "user" | "user_id"
        )
    }) {
        return Ok(false);
    }
    for (key, expected) in [
        ("draft_id", job.base.id.as_str()),
        ("client_msg_id", job.base.client_msg_id.as_str()),
        ("team_id", job.scope.key.team.as_str()),
        ("user", job.user.as_str()),
        ("user_id", job.user.as_str()),
    ] {
        if value.get(key).is_some_and(|v| v.as_str() != Some(expected)) {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(in super::super) fn retain_cleanup(
    store: &DraftStore,
    claim: &CleanupClaim,
    report: &CleanupReport,
) -> Result<String, AppError> {
    let tx = begin(store)?;
    let original: Job = wire::strict(&claim.raw)?;
    if original.id != report.id
        || original.stage != Stage::CleanupDispatched
        || original.cleanup.as_ref().ok_or_else(bad)?.operation.id != report.operation
    {
        return Err(bad());
    }
    let (_, stored) = disk_store::get(&tx, &original.id)?;
    let mut job: Job = wire::strict(&stored)?;
    if job.stage == Stage::CleanupDone
        && job.result.as_ref().is_some_and(|r| {
            serde_json::to_string(r).ok() == serde_json::to_string(&report.evidence).ok()
        })
    {
        tx.commit()?;
        return Ok(stored);
    }
    if stored != claim.raw {
        return Err(bad());
    }
    let evidence: CleanupEvidence = wire::strict(&serde_json::to_string(&report.evidence)?)?;
    if evidence.kind != CleanupKind::Response && evidence.body.is_some() {
        return Err(bad());
    }
    sql_number(evidence.until)?;
    recovery::persist_deadline(&tx, &job.scope, evidence.until)?;
    // Evidence belongs to the original operation even after account/view/row
    // replacement. Applying a reducer to the current row is a separate decision.
    if acknowledged(&job, &evidence)? {
        let view = views::local(&tx, &job.scope).map_err(|_| bad())?;
        if let Ok(current) = views::managed(view) {
            let op = &job.cleanup.as_ref().ok_or_else(bad)?.operation;
            let t = storage::team(&tx, &job.scope.key.team)?;
            if current.row().lineage().enrollment() == job.enrollment
                && current.row().state().pending() == Some(op)
                && current.row().state().acknowledged() == op.base.as_ref()
                && matches!(current.row().state().workflow(), Workflow::InFlight(_))
                && clean(&current).is_ok()
                && claim.stream.is_live()
                && claim.stream.0.matches(&t)
                && t == settled_team(&job)
                && recovery::deadline(&tx, &job.scope.key.team)? == job.known_deadline
            {
                let mut state = current.row().state().clone();
                if !state.confirm_deleted(&job.scope, &op.id, &job.base.id) {
                    return Err(bad());
                }
                let row = successor(&current.row().inner, state)?;
                storage::write_row(&tx, &row)?;
                let after = views::review(&tx, &job.scope).map_err(|_| bad())?;
                job.reduced_before = Some(capture(&current)?);
                job.reduced_after = Some(capture(&after)?);
            }
        }
    }
    job.stage = Stage::CleanupDone;
    job.result = Some(evidence);
    let raw = disk_store::cas(&tx, &stored, &mut job)?;
    tx.commit()?;
    Ok(raw)
}
