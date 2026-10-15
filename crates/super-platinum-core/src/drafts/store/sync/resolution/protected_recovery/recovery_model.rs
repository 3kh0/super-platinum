//! Versioned recovery registry. No loaded record is a dispatch capability.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct RecoveryOwner {
    pub(in super::super) version: u32,
    pub(in super::super) account: String,
    pub(in super::super) team: String,
    pub(in super::super) generation: String,
    pub(in super::super) epoch: u64,
    pub(in super::super) deadline: u64,
    pub(in super::super) unknown_ordinal: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in super::super) enum State {
    Pending,
    Completed,
    SupersededUnknown,
    Historical,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in super::super) enum ResultKind {
    Positive(facade::Outcome),
    Nonpositive(KnownReadFailure),
    Executed {
        version: u32,
        disposition: api::KnownReadFailure,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Attempt {
    pub(in super::super) version: u32,
    pub(in super::super) scope: Scope,
    pub(in super::super) enrollment: String,
    pub(in super::super) generation: String,
    pub(in super::super) owner_epoch: u64,
    pub(in super::super) ticket: crate::drafts::sync::CheckTicket,
    pub(in super::super) floor: u64,
    pub(in super::super) deadline: u64,
    pub(in super::super) state: State,
    pub(in super::super) result: Option<ResultKind>,
    pub(in super::super) late: Option<ResultKind>,
    pub(in super::super) predecessor: Option<String>,
}
pub(in super::super) fn schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("CREATE TABLE composer_draft_recovery_owners(team TEXT PRIMARY KEY,content TEXT NOT NULL);
        CREATE TABLE composer_draft_status_reads(id TEXT PRIMARY KEY,team TEXT NOT NULL,content TEXT NOT NULL);
        CREATE TABLE composer_draft_read_abandonment(id TEXT PRIMARY KEY,team TEXT NOT NULL,content TEXT NOT NULL);
        CREATE TABLE composer_draft_read_late(id TEXT PRIMARY KEY,team TEXT NOT NULL,content TEXT NOT NULL);")?;
    Ok(())
}
pub(in super::super) fn owner_record(
    conn: &Connection,
    team: &str,
) -> Result<Option<RecoveryOwner>, AppError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT content FROM composer_draft_recovery_owners WHERE team=?1",
            [team],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let o: RecoveryOwner = wire::strict(&raw)?;
    if o.version != 1
        || o.team != team
        || !valid_uuid(&o.account)
        || !valid_uuid(&o.generation)
        || o.epoch == 0
        || o.unknown_ordinal > 31
    {
        return Err(invalid("invalid recovery owner"));
    }
    sql_number(o.epoch)?;
    sql_number(o.deadline)?;
    Ok(Some(o))
}
pub(in super::super) fn write_owner(conn: &Connection, o: &RecoveryOwner) -> Result<(), AppError> {
    conn.execute("INSERT INTO composer_draft_recovery_owners VALUES(?1,?2) ON CONFLICT(team) DO UPDATE SET content=excluded.content",params![o.team,serde_json::to_string(o)?])?;
    Ok(())
}
pub(in super::super) fn persist_deadline(
    conn: &Connection,
    scope: &Scope,
    until: u64,
) -> Result<(), AppError> {
    sql_number(until)?;
    adapter_schema::known_if_installed(conn, &scope.key.team, until)?;
    // This entry point receives independently known deadlines, never a newly
    // reserved status ticket's own spacing floor. Keep that distinction in the
    // shared team fence so claim cannot exempt a late limit inside its floor.
    let mut team = storage::team(conn, &scope.key.team)?;
    team.read_retry_at_ms = team.read_retry_at_ms.max(until);
    storage::write_team(conn, &scope.key.team, &team)?;
    let mut o = owner_record(conn, &scope.key.team)?.unwrap_or(RecoveryOwner {
        version: 1,
        account: scope.account_id.clone(),
        team: scope.key.team.clone(),
        generation: uuid::Uuid::new_v4().to_string(),
        epoch: 1,
        deadline: 0,
        unknown_ordinal: 0,
    });
    if o.account != scope.account_id {
        return Err(invalid("cooldown account changed"));
    }
    o.deadline = o.deadline.max(until);
    write_owner(conn, &o)?;
    Ok(())
}
pub(in super::super) fn attempts(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, Attempt>, AppError> {
    validate_audits(conn)?;
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(id AS BLOB))+LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_status_reads",
    )?;
    let mut q = conn.prepare("SELECT id,team,content FROM composer_draft_status_reads")?;
    let mut out = std::collections::BTreeMap::new();
    for r in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, team, raw) = r?;
        let shape: serde_json::Value = wire::strict(&raw)?;
        if shape.get("result").is_none()
            || shape.get("late").is_none()
            || shape.get("predecessor").is_none()
        {
            return Err(invalid("missing attempt guard"));
        }
        let a: Attempt = wire::strict(&raw)?;
        let o =
            owner_record(conn, &team)?.ok_or_else(|| invalid("attempt recovery owner missing"))?;
        let executed = [&a.result, &a.late]
            .into_iter()
            .flatten()
            .any(|r| matches!(r, ResultKind::Executed { .. }));
        if a.version != if executed { 2 } else { 1 }
            || a.scope.account_id != o.account
            || a.scope.key.team != team
            || a.scope != a.ticket.operation().scope
            || id != a.ticket.id()
            || !valid_uuid(&id)
            || !valid_uuid(&a.enrollment)
            || !valid_uuid(&a.generation)
            || a.owner_epoch == 0
            || a.owner_epoch > o.epoch
            || a.floor < READ_FLOOR_MS
            || a.deadline < a.floor
            || a.deadline > o.deadline
            || (a.owner_epoch == o.epoch && a.generation != o.generation)
            || matches!(a.state, State::Pending) && (a.result.is_some() || a.late.is_some())
            || matches!(a.state, State::Completed) && a.result.is_none()
            || matches!(a.state, State::SupersededUnknown) && a.result.is_some()
        {
            return Err(invalid("invalid status-read metadata"));
        }
        if executed {
            adapter_schema::require(conn)?;
            if [&a.result, &a.late]
                .into_iter()
                .flatten()
                .any(|r| matches!(r, ResultKind::Executed { version, .. } if *version != 1))
            {
                return Err(invalid("unsupported execution disposition"));
            }
        }
        for n in [a.owner_epoch, a.floor, a.deadline] {
            sql_number(n)?;
        }
        let op = a.ticket.operation();
        if !valid_uuid(&op.id)
            || !valid_uuid(&op.client_msg_id)
            || op.scope.key.team.is_empty()
            || op.scope.key.channel.is_empty()
            || op.scope.key.thread.as_ref().is_some_and(String::is_empty)
            || op.base.as_ref().is_some_and(|b| {
                b.scope != op.scope || b.id.is_empty() || b.client_msg_id != op.client_msg_id
            })
            || op.snapshot.is_none()
            || matches!(a.state, State::Historical) && a.result.is_some()
        {
            return Err(invalid("invalid retained status operation"));
        }
        // Core operation validity is checked against the owning reducer at
        // reserve/complete. Registry validation also rejects unsupported kinds.
        if !matches!(
            a.ticket.operation().kind,
            crate::drafts::sync::OperationKind::Send | crate::drafts::sync::OperationKind::Update
        ) && !a.ticket.operation().known_schedule_check()
        {
            return Err(invalid("unsupported check kind"));
        }
        out.insert(id, a);
    }
    for a in out.values() {
        if let Some(id) = &a.predecessor {
            if out.get(id).is_none_or(|prior: &Attempt| {
                prior.scope != a.scope
                    || prior.enrollment != a.enrollment
                    || prior.ticket.operation() != a.ticket.operation()
                    || prior.floor >= a.floor
                    || prior.state == State::Pending
            }) {
                return Err(invalid("invalid status attempt predecessor"));
            }
        }
    }
    Ok(out)
}
pub(in super::super) fn write_attempt(conn: &Connection, a: &Attempt) -> Result<(), AppError> {
    conn.execute("INSERT INTO composer_draft_status_reads VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content",params![a.ticket.id(),a.scope.key.team,serde_json::to_string(a)?])?;
    Ok(())
}
pub(in super::super) fn deadline(conn: &Connection, team: &str) -> Result<u64, AppError> {
    attempts(conn)?;
    Ok(owner_record(conn, team)?.map_or(0, |o| o.deadline))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Abandonment {
    pub(in super::super) version: u32,
    pub(in super::super) scope: Scope,
    pub(in super::super) original: ReadRecord,
    pub(in super::super) generation: String,
    pub(in super::super) retry_at_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LateBaseline {
    version: u32,
    failure: KnownReadFailure,
    until: u64,
}
pub(in super::super) fn late_baseline(
    conn: &Connection,
    id: &str,
    record: &ReadRecord,
    failure: KnownReadFailure,
    until: u64,
) -> Result<(), AppError> {
    let previous: Option<String> = conn
        .query_row(
            "SELECT content FROM composer_draft_read_late WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let old = previous
        .map(|s| wire::strict::<LateBaseline>(&s))
        .transpose()?;
    if old.as_ref().is_some_and(|v| v.failure != failure) {
        return Err(invalid("late baseline evidence changed"));
    }
    let value = LateBaseline {
        version: 1,
        failure,
        until: until.max(old.map_or(0, |v| v.until)),
    };
    conn.execute("INSERT INTO composer_draft_read_late VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content",params![id,record.team,serde_json::to_string(&value)?])?;
    Ok(())
}
fn validate_audits(conn: &Connection) -> Result<(), AppError> {
    let records = inventory::reads(conn)?;
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(id AS BLOB))+LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_read_abandonment",
    )?;
    let mut q = conn.prepare("SELECT id,team,content FROM composer_draft_read_abandonment")?;
    let mut seen = std::collections::BTreeSet::new();
    for r in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, team, raw) = r?;
        let a: Abandonment = wire::strict(&raw)?;
        let current = records
            .get(&id)
            .ok_or_else(|| invalid("baseline audit lost its record"))?;
        let o = owner_record(conn, &team)?
            .ok_or_else(|| invalid("baseline audit lost recovery owner"))?;
        if a.version != 1
            || a.scope.account_id != o.account
            || a.scope.key.team != team
            || a.original.team != team
            || !valid_uuid(&a.generation)
            || a.original.state != ReadState::Unresolved
            || a.original.completion.is_some()
            || a.original.retry_at_ms != a.original.floor_ms
            || current.state != ReadState::SupersededUnknown
            || current.lease_id != a.original.lease_id
            || current.epoch != a.original.epoch
            || current.enrollment_id != a.original.enrollment_id
            || current.sequence != a.original.sequence
            || current.floor_ms != a.original.floor_ms
            || a.retry_at_ms < a.original.floor_ms
            || a.retry_at_ms > o.deadline
            || a.retry_at_ms > current.retry_at_ms
        {
            return Err(invalid("invalid baseline abandonment audit"));
        }
        seen.insert(id);
    }
    if records
        .iter()
        .any(|(id, r)| r.state == ReadState::SupersededUnknown && !seen.contains(id))
    {
        return Err(invalid("superseded baseline lost its audit"));
    }
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(id AS BLOB))+LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_read_late",
    )?;
    let mut q = conn.prepare("SELECT id,team,content FROM composer_draft_read_late")?;
    for r in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, team, raw) = r?;
        let late: LateBaseline = wire::strict(&raw)?;
        if late.version != 1
            || records.get(&id).is_none_or(|r| {
                r.team != team
                    || r.state != ReadState::SupersededUnknown
                    || late.until > r.retry_at_ms
            })
        {
            return Err(invalid("invalid late baseline metadata"));
        }
        sql_number(late.until)?;
    }
    Ok(())
}
pub(in super::super) fn unresolved(conn: &Connection, team: &str) -> Result<bool, AppError> {
    Ok(attempts(conn)?
        .values()
        .any(|a| a.scope.key.team == team && a.state == State::Pending))
}
pub(in super::super) fn pending_owner(
    conn: &Connection,
    enrollment: &str,
) -> Result<bool, AppError> {
    Ok(attempts(conn)?
        .values()
        .any(|a| a.enrollment == enrollment && a.state == State::Pending))
}
