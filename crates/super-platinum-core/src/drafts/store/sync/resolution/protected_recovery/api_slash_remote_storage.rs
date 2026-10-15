//! Explicit v3 migration, bounded immutable evidence and full-byte job CAS.
use super::*;
pub(in super::super) fn enabled(conn: &Connection) -> Result<bool, AppError> {
    let v: i64 = conn.query_row(
        "SELECT version FROM composer_draft_slash_format WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    Ok(v == 3)
}
pub(super) fn require(conn: &Connection, account: &str) -> Result<(), AppError> {
    history::require(conn, account)?;
    if !enabled(conn)? {
        return Err(bad());
    }
    Ok(())
}
pub(in super::super) fn migrate(store: &DraftStore, account: &str) -> Result<(), AppError> {
    let tx = begin(store)?;
    history::require(&tx, account)?;
    archive_storage::audit(&tx, account)?;
    if !enabled(&tx)? {
        tx.execute_batch("CREATE TABLE composer_draft_slash_remote_jobs(id TEXT PRIMARY KEY,intent TEXT NOT NULL,team TEXT NOT NULL,content TEXT NOT NULL); CREATE TABLE composer_draft_slash_remote_archives(intent TEXT PRIMARY KEY,destination TEXT NOT NULL,enrollment TEXT NOT NULL,binding TEXT NOT NULL,content TEXT NOT NULL); UPDATE composer_draft_slash_format SET version=3 WHERE singleton=1 AND version=2;")?;
    }
    audit(&tx, account)?;
    tx.commit()
}
fn encode<T: Serialize>(value: &T) -> Result<String, AppError> {
    let s = serde_json::to_string(value)?;
    if s.len() > 4 * 1024 * 1024 {
        return Err(bad());
    }
    Ok(s)
}
fn valid_fence(f: &Fence, scope: &Scope, enrollment: &str) -> Result<Review, AppError> {
    let view = restore(&f.view)?;
    if view.scope() != scope
        || view.row().lineage().enrollment() != enrollment
        || !writers::slash_receipt_matches(&f.receipt, &f.writer, f.serial, &view)?
    {
        return Err(bad());
    }
    clean(&view)?;
    Ok(view)
}
fn validate_job(conn: &Connection, j: &Job) -> Result<(), AppError> {
    let (binding, raw) = bindings::exact(conn, &j.scope, &j.intent)?;
    let (terminal, settlement) = completion::settlement_raw(conn, &j.scope, &j.intent)?;
    let original = valid_fence(&j.original, &j.scope, &j.enrollment)?;
    let base = remote_idle(&original)?;
    if j.version != 1
        || j.revision == 0
        || !valid_uuid(&j.id)
        || j.id != j.pending.id
        || j.pending.kind != PendingKind::Reconcile
        || j.pending.enrollment_id != j.enrollment
        || j.team.pending.as_ref() != Some(&j.pending)
        || j.team.mode != Mode::Open
        || j.team.sequence != j.pending.sequence
        || j.team.epoch == 0
        || j.team.lease_id.as_deref().is_none_or(|v| !valid_uuid(v))
        || j.team.read_retry_at_ms < j.floor
        || j.floor < READ_FLOOR_MS
        || j.binding != raw
        || j.enrollment != binding.enrollment
        || j.user != binding.dispatch.owner().user
        || j.settlement != settlement
        || j.base != *base
        || j.scope != j.base.scope
        || archive_storage::attestation_raw(conn, &j.scope.account_id, &j.intent)?.as_deref()
            != Some(&j.attestation)
    {
        return Err(bad());
    }
    sql_number(j.revision)?;
    sql_number(j.floor)?;
    sql_number(j.known_deadline)?;
    cleanup_archive_team(j, &settled_team(j))?;
    if j.pending.sequence == 0 || j.team.last_interrupted_id.as_ref() == Some(&j.pending.id) {
        return Err(bad());
    }
    let admitted = restore(&binding.inventory)?;
    let prior = admitted.row().state().acknowledged().ok_or_else(bad)?;
    if prior.id != base.id || prior.client_msg_id != base.client_msg_id {
        return Err(bad());
    }
    let proof = archive_storage::proof(&j.attestation)?;
    if !matches!(
        terminal.phase(),
        CommandPhase::Completed(CommandReply::Acknowledged { .. } | CommandReply::Rejected { .. })
    ) && !(matches!(terminal.phase(), CommandPhase::NotSent) && proof.before_execute())
    {
        return Err(bad());
    }
    let revision = match j.stage {
        Stage::ReadReserved => 1,
        Stage::ReadDispatched => 2,
        Stage::ReadDone => {
            if j.decision.is_some() {
                4
            } else {
                3
            }
        }
        Stage::CleanupReserved => 4,
        Stage::CleanupDispatched => 5,
        Stage::CleanupDone => 6,
    };
    if j.revision != revision {
        return Err(bad());
    }
    let read_present = !matches!(j.stage, Stage::ReadReserved | Stage::ReadDispatched);
    let cleanup_present = matches!(
        j.stage,
        Stage::CleanupReserved | Stage::CleanupDispatched | Stage::CleanupDone
    );
    if j.read.is_some() != read_present
        || j.cleanup.is_some() != cleanup_present
        || j.result.is_some() != (j.stage == Stage::CleanupDone)
        || j.reduced_before.is_some() != j.reduced_after.is_some()
        || j.reduced_before.is_some() && j.stage != Stage::CleanupDone
    {
        return Err(bad());
    }
    if let Some(r) = &j.read {
        sql_number(r.until)?;
        if r.user != j.user
            || (r.kind != ReadKind::Response && r.body.is_some())
            || r.body.as_ref().is_some_and(|s| s.len() > 2 * 1024 * 1024)
        {
            return Err(bad());
        }
    }
    if let Some(d) = &j.decision {
        if !valid_uuid(&d.id)
            || d.action.cleanup() != cleanup_present
            || d.action.rejected()
                != matches!(
                    terminal.phase(),
                    CommandPhase::Completed(CommandReply::Rejected { .. })
                )
        {
            return Err(bad());
        }
        positive(j)?;
    } else if cleanup_present {
        return Err(bad());
    }
    if let Some(c) = &j.cleanup {
        let before = restore(&c.before)?;
        let after = restore(&c.after)?;
        if !before.same(&original) {
            return Err(bad());
        }
        let remote = positive(j)?;
        let mut state = resolve_reviewed_remote(
            before.row().state(),
            &remote,
            before.row().state().desired().cloned(),
        )
        .map_err(|_| bad())?;
        let op = state
            .reserve_slash_cleanup(&j.scope, &remote, &c.operation.id)
            .ok_or_else(bad)?;
        if op != c.operation {
            return Err(bad());
        }
        let mut row = successor(&before.row().inner, state)?;
        row.ownership.basis = StoredBasis::Remote;
        row.ownership.verified_epoch = j.team.epoch;
        if !same_successor(&before, &after, &row)? {
            return Err(bad());
        }
    }
    if let Some(r) = &j.result {
        sql_number(r.until)?;
        if r.kind != CleanupKind::Response && r.body.is_some()
            || r.body.as_ref().is_some_and(|s| s.len() > 2 * 1024 * 1024)
        {
            return Err(bad());
        }
        if let (Some(before), Some(after)) = (&j.reduced_before, &j.reduced_after) {
            if !cleanup::acknowledged(j, r)? {
                return Err(bad());
            }
            let before = restore(before)?;
            let after = restore(after)?;
            let op = &j.cleanup.as_ref().ok_or_else(bad)?.operation;
            if before.row().lineage().enrollment() != j.enrollment
                || before.row().state().pending() != Some(op)
                || !before
                    .row()
                    .state()
                    .matches_slash_cleanup_result(op, after.row().state())
                || !same_local(before.local(), after.local())
            {
                return Err(bad());
            }
            let expected = successor(&before.row().inner, after.row().state().clone())?;
            if !same_successor(&before, &after, &expected)? {
                return Err(bad());
            }
        }
    }
    Ok(())
}
pub(super) fn get(conn: &Connection, id: &str) -> Result<(Job, String), AppError> {
    let (intent,team,raw):(String,String,String)=conn.query_row("SELECT CASE WHEN LENGTH(CAST(intent AS BLOB))<=128 THEN intent ELSE NULL END,CASE WHEN LENGTH(CAST(team AS BLOB))<=256 THEN team ELSE NULL END,CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_remote_jobs WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let job: Job = wire::strict(&raw)?;
    if job.id != id || job.intent != intent || job.scope.key.team != team {
        return Err(bad());
    }
    validate_job(conn, &job)?;
    Ok((job, raw))
}
pub(super) fn exact(conn: &Connection, raw: &str) -> Result<Job, AppError> {
    let original: Job = wire::strict(raw)?;
    require(conn, &original.scope.account_id)?;
    audit(conn, &original.scope.account_id)?;
    let (job, current) = get(conn, &original.id)?;
    if current != raw {
        return Err(bad());
    }
    Ok(job)
}
pub(super) fn insert(conn: &Connection, j: &Job) -> Result<String, AppError> {
    if conn.is_autocommit() {
        return Err(bad());
    }
    validate_job(conn, j)?;
    let raw = encode(j)?;
    conn.execute(
        "INSERT INTO composer_draft_slash_remote_jobs VALUES(?1,?2,?3,?4)",
        params![j.id, j.intent, j.scope.key.team, raw],
    )?;
    audit(conn, &j.scope.account_id)?;
    Ok(raw)
}
pub(super) fn cas(conn: &Connection, prior: &str, next: &mut Job) -> Result<String, AppError> {
    if conn.is_autocommit() {
        return Err(bad());
    }
    let old = exact(conn, prior)?;
    if next.revision != old.revision || next.id != old.id {
        return Err(bad());
    }
    let allowed = matches!(
        (old.stage, next.stage),
        (Stage::ReadReserved, Stage::ReadDispatched)
            | (Stage::ReadDispatched, Stage::ReadDone)
            | (Stage::ReadDone, Stage::CleanupReserved)
            | (Stage::CleanupReserved, Stage::CleanupDispatched)
            | (Stage::CleanupDispatched, Stage::CleanupDone)
    ) || old.stage == Stage::ReadDone
        && next.stage == Stage::ReadDone
        && old.decision.is_none()
        && next.decision.as_ref().is_some_and(|d| !d.action.cleanup());
    if !allowed {
        return Err(bad());
    }
    // Immutable reservation and every preexisting evidence field must stay byte-exact.
    let old_value = serde_json::to_value(&old)?;
    let next_value = serde_json::to_value(&*next)?;
    for key in [
        "version",
        "revision",
        "id",
        "scope",
        "intent",
        "enrollment",
        "binding",
        "settlement",
        "attestation",
        "user",
        "base",
        "original",
        "team",
        "pending",
        "floor",
        "known_deadline",
    ] {
        if old_value[key] != next_value[key] {
            return Err(bad());
        }
    }
    for key in [
        "read",
        "decision",
        "cleanup",
        "result",
        "reduced_before",
        "reduced_after",
    ] {
        if !old_value[key].is_null() && old_value[key] != next_value[key] {
            return Err(bad());
        }
    }
    next.revision = next.revision.checked_add(1).ok_or_else(bad)?;
    validate_job(conn, next)?;
    if old.stage == Stage::ReadDispatched && next.stage == Stage::ReadDone {
        continuity::retain(conn, &old, next)?;
    }
    let raw = encode(next)?;
    if conn.execute("UPDATE composer_draft_slash_remote_jobs SET content=?1 WHERE id=?2 AND intent=?3 AND team=?4 AND content=?5",params![raw,next.id,next.intent,next.scope.key.team,prior])?!=1{return Err(bad())}
    audit(conn, &next.scope.account_id)?;
    Ok(raw)
}
pub(in super::super) fn audit(conn: &Connection, account: &str) -> Result<(), AppError> {
    require(conn, account)?;
    archive_storage::audit(conn, account)
}
pub(in super::super) fn audit_remote(conn: &Connection, account: &str) -> Result<(), AppError> {
    require(conn, account)?;
    let (n,size):(i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(content AS BLOB))),0) FROM (SELECT content||id||intent||team AS content FROM composer_draft_slash_remote_jobs UNION ALL SELECT content||intent||destination||enrollment AS content FROM composer_draft_slash_remote_archives UNION ALL SELECT binding AS content FROM composer_draft_slash_remote_archives UNION ALL SELECT content FROM composer_draft_slash_archives UNION ALL SELECT binding AS content FROM composer_draft_slash_archives UNION ALL SELECT content FROM composer_draft_slash_attestations UNION ALL SELECT content FROM composer_draft_slash_bindings)",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if !(0..=16384).contains(&n) || !(0..=16777216).contains(&size) {
        return Err(bad());
    }
    let mut q = conn.prepare("SELECT id FROM composer_draft_slash_remote_jobs")?;
    for id in q.query_map([], |r| r.get::<_, String>(0))? {
        let (j, _) = get(conn, &id?)?;
        continuity::validate_job(conn, &j)?;
        if j.scope.account_id != account {
            return Err(bad());
        }
    }
    let mut q=conn.prepare("SELECT intent,destination,enrollment,binding,CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_remote_archives")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })? {
        let (id, key, enrollment, binding, raw) = row?;
        let a: Archive = wire::strict(&raw)?;
        if a.intent != id
            || a.enrollment != enrollment
            || a.binding != binding
            || a.scope.account_id != account
            || serde_json::to_string(&a.scope.key)? != key
        {
            return Err(bad());
        }
        validate_archive(conn, &a)?;
    }
    Ok(())
}
fn validate_archive(conn: &Connection, a: &Archive) -> Result<(), AppError> {
    let j: Job = wire::strict(&a.job)?;
    let (_, stored) = get(conn, &j.id)?;
    if stored != a.job
        || a.version != 1
        || a.intent != j.intent
        || a.scope != j.scope
        || a.enrollment != j.enrollment
        || a.binding != j.binding
    {
        return Err(bad());
    }
    let before = valid_fence(&a.current, &a.scope, &a.enrollment)?;
    let after = restore(&a.after)?;
    let decision = j.decision.as_ref().ok_or_else(bad)?;
    let row = if decision.action.cleanup() {
        cleanup_archive_team(&j, &a.team)?;
        if j.stage != Stage::CleanupDone
            || !cleanup::acknowledged(&j, j.result.as_ref().ok_or_else(bad)?)?
        {
            return Err(bad());
        }
        let reduced = restore(j.reduced_after.as_ref().ok_or_else(bad)?)?;
        if !before
            .row()
            .state()
            .matches_slash_cleanup_followup(reduced.row().state())
            || before.row().revision() < reduced.row().revision()
            || before.row().inner.ownership != reduced.row().inner.ownership
        {
            return Err(bad());
        }
        let request =
            serde_json::to_string(&(&a.job, &a.current.view, &a.current.writer, a.current.serial))?;
        if request != a.request {
            return Err(bad());
        }
        successor(&before.row().inner, before.row().state().clone())?
    } else {
        if a.team != settled_team(&j) {
            return Err(bad());
        }
        let original = restore(&j.original.view)?;
        let remote = positive(&j)?;
        if !before.same(&original) || before.row().state().desired() != Some(&remote.snapshot) {
            return Err(bad());
        }
        let mut previous: Job = wire::strict(&a.job)?;
        previous.decision = None;
        previous.revision = previous.revision.checked_sub(1).ok_or_else(bad)?;
        if serde_json::to_string(&(serde_json::to_string(&previous)?, decision))? != a.request {
            return Err(bad());
        }
        let state = resolve_reviewed_remote(
            before.row().state(),
            &remote,
            before.row().state().desired().cloned(),
        )
        .map_err(|_| bad())?;
        let mut row = successor(&before.row().inner, state)?;
        row.ownership.basis = StoredBasis::Remote;
        row.ownership.verified_epoch = a.team.epoch;
        row
    };
    if !same_successor(&before, &after, &row)? {
        return Err(bad());
    }
    let (terminal, _) = completion::settlement_raw(conn, &j.scope, &j.intent)?;
    if a.retired != terminal.protected_retirement().map_err(|_| bad())?
        || slash_journal::protected_inventory(conn, &j.scope.account_id)
            .map_err(|_| bad())?
            .iter()
            .find(|r| r.id() == j.intent)
            != Some(&a.retired)
    {
        return Err(bad());
    }
    Ok(())
}
pub(super) fn archived(
    conn: &Connection,
    scope: &Scope,
    intent: &str,
    request: &str,
) -> Result<Option<Finished>, AppError> {
    audit(conn, &scope.account_id)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT content FROM composer_draft_slash_remote_archives WHERE intent=?1",
            [intent],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = raw {
        let a: Archive = wire::strict(&raw)?;
        if a.scope != *scope || a.request != request {
            return Err(bad());
        }
        Ok(Some(Finished { raw }))
    } else {
        Ok(None)
    }
}
pub(super) fn unresolved(conn: &Connection, team: &str) -> Result<bool, AppError> {
    let mut q=conn.prepare("SELECT id FROM composer_draft_slash_remote_jobs WHERE team=?1 AND intent NOT IN (SELECT intent FROM composer_draft_slash_remote_archives)")?;
    for id in q.query_map([team], |r| r.get::<_, String>(0))? {
        let (j, _) = get(conn, &id?)?;
        if j.stage != Stage::ReadDone || j.read.as_ref().is_none_or(|r| r.kind == ReadKind::Unknown)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
