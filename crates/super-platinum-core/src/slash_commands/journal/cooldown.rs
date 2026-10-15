use super::*;
#[derive(Clone)]
struct Gate {
    permanent: bool,
    duration: Option<i64>,
    run: Option<String>,
    mono: Option<i64>,
    wall: Option<i64>,
}
fn read(conn: &Connection, scope: &str) -> Result<Option<Gate>, Failure> {
    conn.query_row("SELECT permanent,duration_ms,run,until_mono,until_wall FROM sp_slash_cooldowns WHERE scope=?1",[scope],|r|{
        let permanent:i64=r.get(0)?;
        Ok((permanent,Gate{permanent:permanent==1,duration:r.get(1)?,run:r.get(2)?,mono:r.get(3)?,wall:r.get(4)?}))
    }).optional().map_err(storage)?.map(|(kind,g)|{
        if !(0..=1).contains(&kind) || if g.permanent {g.duration.is_some()||g.run.is_some()||g.mono.is_some()||g.wall.is_some()}
            else {g.duration.is_none_or(|v|v<0)||g.run.as_deref().is_none_or(|v|!valid_account(v))||g.mono.is_none_or(|v|v<0)||g.wall.is_none_or(|v|v<0)} {return Err(Failure::Invalid);}Ok(g)
    }).transpose()
}
fn write(conn: &Connection, scope: &str, g: &Gate) -> Result<(), Failure> {
    conn.execute("INSERT INTO sp_slash_cooldowns VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(scope) DO UPDATE SET permanent=excluded.permanent,duration_ms=excluded.duration_ms,run=excluded.run,until_mono=excluded.until_mono,until_wall=excluded.until_wall",
        params![scope,i64::from(g.permanent),g.duration,g.run,g.mono,g.wall]).map_err(storage)?;
    Ok(())
}
fn rebase(g: &mut Gate, now: &Stamp) -> Result<(), Failure> {
    if !g.permanent && g.run.as_deref() != Some(&now.run) {
        // Restart conservatively waits the entire advertised interval. Wall time
        // is display-only and cannot shorten the gate after a forward jump.
        let duration = g.duration.ok_or(Failure::Invalid)?;
        g.mono = Some(now.mono.checked_add(duration).ok_or(Failure::Invalid)?);
        g.wall = Some(now.wall.checked_add(duration).ok_or(Failure::Invalid)?);
        g.run = Some(now.run.clone());
    }
    Ok(())
}
/// Return blocked after persisting any rebase/expiry in the surrounding tx.
pub(super) fn blocked(conn: &Connection, scope: &str, now: &Stamp) -> Result<bool, Failure> {
    let Some(mut gate) = read(conn, scope)? else {
        return Ok(false);
    };
    if gate.permanent {
        return Ok(true);
    }
    rebase(&mut gate, now)?;
    if now.mono >= gate.mono.ok_or(Failure::Invalid)? {
        conn.execute("DELETE FROM sp_slash_cooldowns WHERE scope=?1", [scope])
            .map_err(storage)?;
        Ok(false)
    } else {
        write(conn, scope, &gate)?;
        Ok(true)
    }
}
pub(super) fn retain(
    conn: &Connection,
    scope: &str,
    record: &Record,
    now: &Stamp,
) -> Result<(), Failure> {
    let missing = record.http_status() == Some(429) && record.retry_after_secs().is_none();
    let Some(seconds) = record.retry_after_secs() else {
        if missing {
            write(
                conn,
                scope,
                &Gate {
                    permanent: true,
                    duration: None,
                    run: None,
                    mono: None,
                    wall: None,
                },
            )?;
        }
        return Ok(());
    };
    let duration = i64::try_from(seconds)
        .ok()
        .and_then(|v| v.checked_mul(1000));
    let Some(duration) = duration else {
        return write(
            conn,
            scope,
            &Gate {
                permanent: true,
                duration: None,
                run: None,
                mono: None,
                wall: None,
            },
        );
    };
    let (Some(mono), Some(wall)) = (
        now.mono.checked_add(duration),
        now.wall.checked_add(duration),
    ) else {
        return write(
            conn,
            scope,
            &Gate {
                permanent: true,
                duration: None,
                run: None,
                mono: None,
                wall: None,
            },
        );
    };
    let mut gate = Gate {
        permanent: false,
        duration: Some(duration),
        run: Some(now.run.clone()),
        mono: Some(mono),
        wall: Some(wall),
    };
    if let Some(mut prior) = read(conn, scope)? {
        if prior.permanent {
            return Ok(());
        }
        rebase(&mut prior, now)?;
        gate.duration = Some(duration.max(prior.duration.ok_or(Failure::Invalid)?));
        gate.mono = Some(mono.max(prior.mono.ok_or(Failure::Invalid)?));
        gate.wall = Some(wall.max(prior.wall.ok_or(Failure::Invalid)?));
    }
    write(conn, scope, &gate)
}
pub(super) fn audit(conn: &Connection, expected_account: &str) -> Result<(), Failure> {
    let (count,total):(i64,i64)=conn.query_row("SELECT count(*),COALESCE(SUM(length(scope)+COALESCE(length(run),0)),0) FROM sp_slash_cooldowns",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(storage)?;
    if count < 0 || count as usize > MAX_RECORDS || total < 0 || total as usize > MAX_BYTES {
        return Err(Failure::Invalid);
    }
    let mut query = conn
        .prepare("SELECT scope FROM sp_slash_cooldowns LIMIT 4097")
        .map_err(storage)?;
    let rows = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(storage)?;
    for (index, scope) in rows.enumerate() {
        if index >= MAX_RECORDS {
            return Err(Failure::Invalid);
        }
        let scope = scope.map_err(storage)?;
        let value =
            super::super::codec::decode(scope.as_bytes(), 1024).map_err(|_| Failure::Invalid)?;
        let (account, team, user): (String, String, String) =
            serde_json::from_value(value).map_err(storage)?;
        if account != expected_account
            || !valid_account(&account)
            || !super::super::id(&team, 'T')
            || !super::super::id(&user, 'U')
            || serde_json::to_string(&(&account, &team, &user)).map_err(storage)? != scope
        {
            return Err(Failure::Invalid);
        }
        read(conn, &scope)?.ok_or(Failure::Invalid)?;
    }
    Ok(())
}
