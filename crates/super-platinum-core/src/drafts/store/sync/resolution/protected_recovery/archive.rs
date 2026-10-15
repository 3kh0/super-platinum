//! Terminal archive releases ownership; uncertain editing keeps the active guard.
use super::*;
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Archive {
    version: u32,
    pub(super) scope: Scope,
    pub(super) enrollment: String,
    revision: u64,
    row: String,
    local: Value,
    blocked: Value,
    alternates: Vec<Value>,
    writer_floors: Vec<(String, u64)>,
}
pub(super) fn schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("CREATE TABLE composer_draft_protected_archive(enrollment TEXT PRIMARY KEY,team TEXT NOT NULL,content TEXT NOT NULL);
        CREATE TABLE composer_draft_recovery_editing(enrollment TEXT PRIMARY KEY,destination TEXT NOT NULL);
        CREATE TABLE composer_draft_release_head(destination TEXT PRIMARY KEY,enrollment TEXT NOT NULL);")?;
    Ok(())
}
pub(super) fn inventory(conn: &Connection, account: &str) -> Result<Vec<Archive>, AppError> {
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(enrollment AS BLOB))+LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_protected_archive",
    )?;
    let mut q =
        conn.prepare("SELECT enrollment,team,content FROM composer_draft_protected_archive")?;
    let mut result = Vec::new();
    for r in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, team, raw) = r?;
        let a: Archive = wire::strict(&raw)?;
        if a.version != 1
            || id != a.enrollment
            || team != a.scope.key.team
            || account != a.scope.account_id
            || !valid_uuid(&id)
        {
            return Err(invalid("invalid archive identity/version"));
        }
        let row = wire::decode(&a.scope, sql_number(a.revision)?, 7, &a.row)?;
        if row.ownership.enrollment_id != id
            || row.stored.state.pending().is_some()
            || row.stored.state.consumption().is_some()
            || row.stored.state.acknowledged().is_some()
            || !matches!(row.stored.state.workflow(), Workflow::Idle)
        {
            return Err(invalid("invalid terminal archive"));
        }
        let _: LocalDraft = serde_json::from_value(a.local.clone())?;
        let _: Option<(String, LocalDraft)> = serde_json::from_value(a.blocked.clone())?;
        for v in &a.alternates {
            let (writer, revision, _): (String, u64, LocalDraft) =
                serde_json::from_value(v.clone())?;
            if !valid_uuid(&writer) || revision == 0 {
                return Err(invalid("invalid archived alternate"));
            }
            sql_number(revision)?;
        }
        for (writer, revision) in &a.writer_floors {
            if !valid_uuid(writer) || *revision == 0 {
                return Err(invalid("invalid archived writer"));
            }
            sql_number(*revision)?;
        }
        result.push(a);
    }
    Ok(result)
}
pub(super) fn reject_claim(
    conn: &Connection,
    scope: &Scope,
    server: Option<&str>,
    client: Option<&str>,
) -> Result<(), AppError> {
    for a in inventory(conn, &scope.account_id)? {
        if a.scope.key.team == scope.key.team
            && wire::decode(&a.scope, sql_number(a.revision)?, 7, &a.row)?
                .stored
                .state
                .protected_claims(server, client)
        {
            return Err(invalid("identity belongs to retained archived owner"));
        }
    }
    Ok(())
}
#[derive(Clone)]
pub(super) struct Release {
    pub(super) local: LocalDraft,
    pub(super) scope: Scope,
    pub(super) enrollment: String,
}
pub(super) fn release(store: &DraftStore, expected: &ProtectedReview) -> Result<Release, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    api::slash_guard(&tx, &current.review.scope)?;
    let state = &current.row.stored.state;
    let team = storage::team(&tx, &current.review.scope.key.team)?;
    if state.pending().is_some()
        || state.consumption().is_some()
        || state.acknowledged().is_some()
        || !matches!(state.workflow(), Workflow::Idle)
        || state.blocked_observation().is_some()
        || current.review.blocked.is_some()
        || !current.review.alternates.is_empty()
        || team
            .pending
            .as_ref()
            .is_some_and(|p| p.enrollment_id == current.row.ownership.enrollment_id)
        || recovery::pending_owner(&tx, &current.row.ownership.enrollment_id)?
        || inventory::reads(&tx)?.values().any(|r| {
            r.enrollment_id == current.row.ownership.enrollment_id
                && r.state == ReadState::Unresolved
        })
    {
        return Err(invalid(
            "terminal release needs resolved operation and reviewed versions",
        ));
    }
    let key = serde_json::to_string(&current.review.scope.key)?;
    let floors = {
        let mut q=tx.prepare("SELECT writer_id,revision FROM composer_draft_reviewed_writers WHERE destination=?1 ORDER BY writer_id")?;
        q.query_map([&key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(writer, revision)| {
            u64::try_from(revision)
                .map(|revision| (writer, revision))
                .map_err(|_| invalid("negative reviewed writer revision"))
        })
        .collect::<Result<Vec<_>, _>>()?
    };
    // Even identity-free release keeps an incarnation tombstone for stale handoffs.
    let a = Archive {
        version: 1,
        scope: current.review.scope.clone(),
        enrollment: current.row.ownership.enrollment_id.clone(),
        revision: current.row.stored.revision,
        row: wire::encode(&current.row)?,
        local: serde_json::to_value(&current.review.local)?,
        blocked: serde_json::to_value(
            current
                .review
                .blocked
                .as_ref()
                .map(|b| (&b.token, &b.draft)),
        )?,
        alternates: current
            .review
            .alternates
            .iter()
            .map(|a| serde_json::to_value((&a.writer_id, a.revision, &a.draft)))
            .collect::<Result<_, _>>()?,
        writer_floors: floors,
    };
    tx.execute(
        "INSERT INTO composer_draft_protected_archive VALUES(?1,?2,?3)",
        params![a.enrollment, a.scope.key.team, serde_json::to_string(&a)?],
    )?;
    tx.execute("INSERT INTO composer_draft_release_head VALUES(?1,?2) ON CONFLICT(destination) DO UPDATE SET enrollment=excluded.enrollment",params![key,a.enrollment])?;
    tx.execute(
        "DELETE FROM composer_draft_sync WHERE destination=?1",
        [&key],
    )?;
    tx.commit()?;
    Ok(Release {
        local: current.review.local,
        scope: current.review.scope,
        enrollment: current.row.ownership.enrollment_id,
    })
}
pub(super) fn save_released(
    store: &DraftStore,
    released: &Release,
    local: &LocalDraft,
    writer: &LocalWriter,
    serial: u64,
) -> Result<bool, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &released.scope.account_id)?;
    inventory(&tx, &released.scope.account_id)?;
    let key = serde_json::to_string(&released.scope.key)?;
    let head: Option<String> = tx
        .query_row(
            "SELECT enrollment FROM composer_draft_release_head WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM composer_draft_sync WHERE destination=?1) OR EXISTS(SELECT 1 FROM composer_draft_blocked WHERE destination=?1)",[&key],|r|r.get(0))?;
    let raw: Option<String> = tx
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let current: LocalDraft = raw
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    if head.as_ref() != Some(&released.enrollment)
        || active
        || has_alternates(&tx, &released.scope)?
        || !same_local(&current, &released.local)
        || reviewed_revision(&tx, &released.scope, writer)?.is_some()
    {
        preserve_conflict(&tx, &released.scope, local)?;
        preserve_owned_alternate(&tx, &released.scope, writer, serial, local)?;
        tx.commit()?;
        return Ok(false);
    }
    // Writer/serial are validated even on the successful path.
    if serial == 0 || !valid_uuid(writer.id()) {
        return Err(invalid("invalid released writer"));
    }
    sql_number(serial)?;
    write_local(&tx, &released.scope.key, local)?;
    // A successful handoff is one-use, including unchanged/caret-only saves.
    // Text returning to its old value must not revive delayed writer authority.
    let removed = tx.execute(
        "DELETE FROM composer_draft_release_head WHERE destination=?1 AND enrollment=?2",
        params![key, released.enrollment],
    )?;
    if removed != 1 {
        return Err(invalid("released handoff changed"));
    }
    tx.commit()?;
    Ok(true)
}
pub(super) fn keep_editing(store: &DraftStore, expected: &ProtectedReview) -> Result<(), AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    if !matches!(current.row.stored.state.workflow(), Workflow::Uncertain(_))
        || current
            .row
            .stored
            .state
            .local_preservation_intent()
            .is_none()
    {
        return Err(invalid(
            "uncertain editing requires immutable preservation intent",
        ));
    }
    tx.execute(
        "INSERT OR IGNORE INTO composer_draft_recovery_editing VALUES(?1,?2)",
        params![
            current.row.ownership.enrollment_id,
            serde_json::to_string(&current.review.scope.key)?
        ],
    )?;
    // No row deletion, outcome, retry, canonical edit, or permission to send.
    tx.commit()?;
    Ok(())
}
