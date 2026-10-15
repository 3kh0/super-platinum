use super::*;
pub(super) struct Entry {
    pub record: Record,
    pub run: String,
    pub bytes: Vec<u8>,
}
pub(super) fn inventory(
    conn: &Connection,
    account: &str,
) -> Result<BTreeMap<String, Entry>, Failure> {
    let (count,total):(i64,i64)=conn.query_row("SELECT count(*),COALESCE(SUM(length(body)+length(id)+length(scope)+length(created_run)),0) FROM sp_slash_intents",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(storage)?;
    if count < 0 || count as usize > MAX_RECORDS || total < 0 || total as usize > MAX_BYTES {
        return Err(Failure::Invalid);
    }
    let mut query=conn.prepare("SELECT id,scope,version,created_run,CASE WHEN length(body)<=1048576 THEN body ELSE NULL END FROM sp_slash_intents").map_err(storage)?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(storage)?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (id, scope, version, run, bytes) = row.map_err(storage)?;
        let record = Record::decode(&bytes)?;
        valid(&record)?;
        if record.owner().account != account
            || record.id() != id
            || rate_owner(&record) != scope
            || i64::try_from(record.version()).map_err(storage)? != version
            || !valid_account(&run)
            || out.insert(id, Entry { record, run, bytes }).is_some()
        {
            return Err(Failure::Invalid);
        }
    }
    Ok(out)
}
fn transition(old: Option<&Record>, next: &Record) -> Result<(), Failure> {
    valid(next)?;
    let Some(old) = old else {
        return if next.version() == 1 && !next.retired() && matches!(next.phase(), Phase::Prepared)
        {
            Ok(())
        } else {
            Err(Failure::Invalid)
        };
    };
    valid(old)?;
    if old.id() != next.id()
        || !next.same_input(old)
        || old.retired()
        || old.version().checked_add(1) != Some(next.version())
    {
        return Err(Failure::Stale);
    }
    if next.retired() {
        return if !unresolved(old)
            && next.phase() == old.phase()
            && next.http_status() == old.http_status()
            && next.retry_after_secs() == old.retry_after_secs()
        {
            Ok(())
        } else {
            Err(Failure::NoReplay)
        };
    }
    match (old.phase(), next.phase()) {
        (Phase::Prepared, Phase::DispatchPossible) => Ok(()),
        (Phase::DispatchPossible, Phase::NotSent | Phase::Completed(_) | Phase::Unknown) => Ok(()),
        _ => Err(Failure::NoReplay),
    }
}
pub(super) fn apply(
    conn: &Connection,
    account: &str,
    old: Option<&Record>,
    next: &Record,
    now: &Stamp,
) -> Result<(), Failure> {
    schema::require(conn, account)?;
    if next.owner().account != account || !valid_account(&now.run) || now.mono < 0 || now.wall < 0 {
        return Err(Failure::Stale);
    }
    transition(old, next)?;
    let rows = inventory(conn, account)?;
    cooldown::audit(conn, account)?;
    let current = rows.get(next.id());
    if current.is_some_and(|v| v.record == *next) {
        if current.is_none_or(|v| serde_json::to_vec(next).ok().as_ref() != Some(&v.bytes)) {
            return Err(Failure::Stale);
        }
        // Terminal/Prepared insertion receipts are disk-only idempotence.
        // A repeated dispatch admission never issues a second permission.
        return if matches!(next.phase(), Phase::DispatchPossible) {
            Err(Failure::NoReplay)
        } else {
            Ok(())
        };
    }
    if current.map(|v| &v.record) != old {
        return Err(Failure::Stale);
    }
    let scope = rate_owner(next);
    let admission = old.is_none() || matches!(next.phase(), Phase::DispatchPossible);
    if admission {
        if cooldown::blocked(conn, &scope, now)? {
            return Err(Failure::RateLimited);
        }
        if rows.values().any(|v| {
            v.record.id() != next.id() && rate_owner(&v.record) == scope && unresolved(&v.record)
        }) {
            return Err(Failure::NoReplay);
        }
        if matches!(next.phase(), Phase::DispatchPossible)
            && current.is_none_or(|v| v.run != now.run)
        {
            return Err(Failure::NoReplay);
        }
    }
    let bytes = serde_json::to_vec(next).map_err(storage)?;
    if bytes.len() > MAX_RECORD {
        return Err(Failure::Invalid);
    }
    let version = i64::try_from(next.version()).map_err(storage)?;
    match current {
        None => {
            conn.execute(
                "INSERT INTO sp_slash_intents VALUES(?1,?2,?3,?4,?5)",
                params![next.id(), scope, version, now.run, bytes],
            )
            .map_err(storage)?;
        }
        Some(prior) => {
            let old_bytes = serde_json::to_vec(&prior.record).map_err(storage)?;
            let changed=conn.execute("UPDATE sp_slash_intents SET version=?1,body=?2 WHERE id=?3 AND version=?4 AND body=?5",
                params![version,bytes,next.id(),i64::try_from(prior.record.version()).map_err(storage)?,old_bytes]).map_err(storage)?;
            if changed != 1 {
                return Err(Failure::Stale);
            }
        }
    }
    // Retiring a terminal receipt must not extend its prior cooldown.
    if matches!(old.map(Record::phase), Some(Phase::DispatchPossible)) {
        cooldown::retain(conn, &scope, next, now)?;
    }
    inventory(conn, account)?;
    cooldown::audit(conn, account)?;
    Ok(())
}
