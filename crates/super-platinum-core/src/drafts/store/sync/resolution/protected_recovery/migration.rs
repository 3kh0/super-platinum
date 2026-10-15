//! Explicit candidate upgrades and restart normalization; no production caller.
use super::*;

pub(super) fn normalize(
    store: &DraftStore,
    account: &str,
) -> Result<Vec<ProtectedReview>, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, account)?;
    recovery::attempts(&tx)?;
    archive::inventory(&tx, account)?;
    let scopes = scopes(&tx, account)?;
    let mut result = Vec::new();
    // Decode ALL rows before mutating: malformed input rolls the transaction back.
    for scope in &scopes {
        let current = storage::review_row(&tx, scope)?;
        let state = current.row.stored.state.clone().restore_after_restart();
        if state != current.row.stored.state {
            storage::write_row(&tx, &successor(&current.row, state)?)?;
        }
        result.push(storage::review_row(&tx, scope)?);
    }
    inventory::bound(
        &tx,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_continuity",
    )?;
    let teams = {
        let mut q = tx.prepare("SELECT team FROM composer_draft_continuity")?;
        q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?
    };
    if scopes.iter().any(|s| !teams.contains(&s.key.team)) {
        return Err(invalid("active owner has no continuity team"));
    }
    for key in teams {
        let mut team = storage::team(&tx, &key)?;
        team.epoch = increment(team.epoch)?;
        team.mode = Mode::Gap;
        team.lease_id = None;
        team.gap_reason = Some("explicit restart normalization".into());
        if let Some(p) = team.pending.take() {
            team.last_interrupted_id = Some(p.id);
        }
        storage::write_team(&tx, &key, &team)?;
    }
    tx.commit()?;
    Ok(result)
}
pub(super) fn scopes(conn: &Connection, account: &str) -> Result<Vec<Scope>, AppError> {
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(destination AS BLOB))+LENGTH(CAST(state AS BLOB))),0) FROM composer_draft_sync",
    )?;
    let mut q = conn.prepare("SELECT destination FROM composer_draft_sync")?;
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for key in q.query_map([], |r| r.get::<_, String>(0))? {
        let key: DraftKey = serde_json::from_str(&key?)?;
        if !seen.insert(key.clone()) {
            return Err(invalid("duplicate semantic destination"));
        }
        out.push(Scope {
            account_id: account.into(),
            key,
        });
    }
    Ok(out)
}
pub(super) fn import_checks(conn: &Connection, account: &str) -> Result<(), AppError> {
    for scope in scopes(conn, account)? {
        let row = storage::read_row(conn, &scope)?;
        let floor = row.stored.state.protected_retained_deadline();
        // Known cooldowns constrain every owner's reads, even after the ticket
        // that learned the limit was durably completed and removed.
        if floor > 0 {
            recovery::persist_deadline(conn, &scope, floor)?;
        }
        let Some(ticket) = row.stored.state.protected_retained_check().cloned() else {
            continue;
        };
        if floor < READ_FLOOR_MS {
            return Err(invalid("imported pending read lacks admission floor"));
        }
        let mut o =
            recovery::owner_record(conn, &scope.key.team)?.unwrap_or(recovery::RecoveryOwner {
                version: 1,
                account: account.into(),
                team: scope.key.team.clone(),
                generation: uuid::Uuid::new_v4().to_string(),
                epoch: 1,
                deadline: 0,
                unknown_ordinal: 0,
            });
        o.deadline = o.deadline.max(floor);
        recovery::write_owner(conn, &o)?;
        let a = recovery::Attempt {
            version: 1,
            scope: scope.clone(),
            enrollment: row.ownership.enrollment_id.clone(),
            generation: o.generation,
            owner_epoch: o.epoch,
            state: if row.stored.state.pending() == Some(ticket.operation()) {
                recovery::State::Pending
            } else {
                recovery::State::Historical
            },
            ticket,
            floor,
            deadline: floor,
            result: None,
            late: None,
            predecessor: None,
        };
        recovery::write_attempt(conn, &a)?;
    }
    Ok(())
}
/// Upgrade only the privately activated v6 candidate, preserving all unresolved
/// baseline read records and lease fences. Never interpret v6 as local-origin.
pub(super) fn upgrade_v6(store: &DraftStore, account: &str) -> Result<(), AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, account)?;
    let version: i64 = tx.query_row(
        "SELECT version FROM composer_draft_continuity_format WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if version != 6 {
        return Err(invalid("only candidate v6 upgrade supported"));
    }
    let rows = {
        let mut q = tx.prepare(
            "SELECT destination,storage_revision,format_version,state FROM composer_draft_sync",
        )?;
        q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let mut decoded = Vec::new();
    for (key, revision, format, raw) in rows {
        if format != 6 {
            return Err(invalid("mixed protected formats"));
        }
        let scope = Scope {
            account_id: account.into(),
            key: serde_json::from_str(&key)?,
        };
        let mut value: serde_json::Value = wire::strict(&raw)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| invalid("invalid v6 envelope"))?;
        if object.len() != 3
            || !["state", "enrollment_id", "verified_epoch"]
                .iter()
                .all(|k| object.contains_key(*k))
        {
            return Err(invalid("invalid v6 fields"));
        }
        // Validate old counter before explicitly dropping verification authority.
        sql_number(
            object["verified_epoch"]
                .as_u64()
                .ok_or_else(|| invalid("invalid v6 epoch"))?,
        )?;
        object.insert("verified_epoch".into(), 0.into());
        object.insert("basis".into(), "Unverified".into());
        object.insert("origin".into(), "Imported".into());
        decoded.push(wire::decode(&scope, revision, 7, &value.to_string())?);
    }
    // Old guards cannot be bypassed by partially upgrading rows: one transaction.
    tx.execute_batch("DROP TRIGGER composer_draft_require_v6_insert; DROP TRIGGER composer_draft_require_v6_update;
        DROP TABLE composer_draft_continuity_format;
        CREATE TABLE composer_draft_continuity_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=7));
        INSERT INTO composer_draft_continuity_format VALUES(1,7);")?;
    for row in decoded {
        storage::write_row(&tx, &row)?;
    }
    tx.execute_batch("CREATE TRIGGER composer_draft_require_v7_insert BEFORE INSERT ON composer_draft_sync WHEN NEW.format_version != 7 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;
        CREATE TRIGGER composer_draft_require_v7_update BEFORE UPDATE ON composer_draft_sync WHEN NEW.format_version != 7 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;")?;
    recovery::schema(&tx)?;
    archive::schema(&tx)?;
    import_checks(&tx, account)?;
    tx.commit()?;
    Ok(())
}
