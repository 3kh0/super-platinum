use super::*;

pub(super) fn read(conn: &Connection, scope: &Scope, ts: &str) -> Result<EditInventory, AppError> {
    let owner = key(scope, ts)?;
    let raw:Option<String>=conn.query_row(
        "SELECT CASE WHEN length(CAST(body AS BLOB))<=?2 THEN body ELSE NULL END FROM edit_drafts WHERE owner=?1",
        params![owner,super::super::MAX_BODY as i64],|row|row.get(0)).optional()?;
    let value = match raw {
        Some(raw) => serde_json::from_str::<EditInventory>(&raw)?,
        None => {
            let history: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM edit_history WHERE owner=?1)",
                [key(scope, ts)?],
                |row| row.get(0),
            )?;
            if history {
                return Err(invalid());
            }
            EditInventory {
                scope: scope.clone(),
                ts: ts.into(),
                revision: 0,
                capture: String::new(),
                seed: None,
                blocks: vec![],
                plain_delta: None,
                attempt: None,
                retry_at: 0,
            }
        }
    };
    value.validate()?;
    if value.scope != *scope || value.ts != ts {
        return Err(invalid());
    }
    if let Some(attempt) = &value.attempt {
        if read_attempt(conn, scope, ts, &attempt.id)?.as_ref() != Some(attempt) {
            return Err(invalid());
        }
    }
    Ok(value)
}
pub(super) fn read_attempt(
    conn: &Connection,
    scope: &Scope,
    ts: &str,
    id: &str,
) -> Result<Option<EditAttempt>, AppError> {
    uuid(id)?;
    let owner = key(scope, ts)?;
    let raw:Option<(String,String)>=conn.query_row(
        "SELECT owner,CASE WHEN length(CAST(body AS BLOB))<=?2 THEN body ELSE NULL END FROM edit_history WHERE id=?1",
        params![id,super::super::MAX_BODY as i64],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let Some((stored, raw)) = raw else {
        return Ok(None);
    };
    if stored != owner {
        return Err(invalid());
    }
    let value: EditAttempt = serde_json::from_str(&raw)?;
    value.validate(scope, ts)?;
    if value.id != id {
        return Err(invalid());
    }
    Ok(Some(value))
}
pub(super) fn write(conn: &Connection, value: &EditInventory) -> Result<(), AppError> {
    value.validate()?;
    conn.execute("INSERT INTO edit_drafts(owner,body) VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET body=excluded.body",
        params![key(&value.scope,&value.ts)?,encoded(value)?])?;
    Ok(())
}
pub(super) fn write_attempt(
    conn: &Connection,
    scope: &Scope,
    ts: &str,
    value: &EditAttempt,
    insert: bool,
) -> Result<(), AppError> {
    value.validate(scope, ts)?;
    let owner = key(scope, ts)?;
    if insert {
        conn.execute(
            "INSERT INTO edit_history(id,owner,body) VALUES(?1,?2,?3)",
            params![value.id, owner, encoded(value)?],
        )?;
    } else {
        if conn.execute(
            "UPDATE edit_history SET body=?3 WHERE id=?1 AND owner=?2",
            params![value.id, owner, encoded(value)?],
        )? != 1
        {
            return Err(invalid());
        }
    }
    Ok(())
}
pub(super) fn check_bounds(conn: &Connection) -> Result<(), AppError> {
    let rows: i64 = conn.query_row("SELECT count(*) FROM edit_drafts", [], |r| r.get(0))?;
    let attempts: i64 = conn.query_row("SELECT count(*) FROM edit_history", [], |r| r.get(0))?;
    let bytes:i64=conn.query_row("SELECT COALESCE((SELECT sum(length(CAST(body AS BLOB))) FROM edit_drafts),0)+COALESCE((SELECT sum(length(CAST(body AS BLOB))) FROM edit_history),0)",[],|r|r.get(0))?;
    if rows > MAX_ROWS || attempts > MAX_ATTEMPTS || bytes > MAX_TOTAL_BYTES {
        return Err(invalid());
    }
    Ok(())
}
