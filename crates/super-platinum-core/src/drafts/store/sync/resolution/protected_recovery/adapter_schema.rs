//! Explicit adapter sidecar protocol. No production migration caller exists.
use super::*;

pub(super) const VERSION: i64 = 1;
pub(super) fn present(conn: &Connection) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='composer_draft_adapter_format')",
        [], |r| r.get(0),
    )?)
}
pub(super) fn require(conn: &Connection) -> Result<(), AppError> {
    if !present(conn)? {
        return Err(invalid("protected adapter sidecar required"));
    }
    let rows = {
        let mut q = conn.prepare("SELECT singleton,version FROM composer_draft_adapter_format")?;
        q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    if rows != [(1, VERSION)] {
        return Err(invalid("unsupported adapter sidecar"));
    }
    let format: i64 = conn.query_row(
        "SELECT version FROM composer_draft_continuity_format WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if format != 7 {
        return Err(invalid("adapter requires protected format seven"));
    }
    Ok(())
}

/// Every current-engine canonical write calls this inside its own transaction,
/// including DELETE of an already absent row. Triggers additionally fence writes
/// from another connection that bypasses that helper. Neither is a new writer.
pub(super) fn touch_local_if_installed(conn: &Connection, key: &DraftKey) -> Result<(), AppError> {
    if !present(conn)? {
        return Ok(());
    }
    require(conn)?;
    conn.execute(
        "INSERT INTO composer_draft_local_clock(destination,revision,generation,writer_id,serial)
         VALUES(?1,1,NULL,NULL,0) ON CONFLICT(destination) DO UPDATE SET
         revision=revision+1,generation=NULL,writer_id=NULL,serial=0",
        [serde_json::to_string(key)?],
    )?;
    Ok(())
}

/// Independently known deadlines, not a newly reserved read's spacing floor.
/// Invoked within the outcome transaction; inactive v7 tests retain old behavior.
pub(super) fn known_if_installed(
    conn: &Connection,
    team: &str,
    until: u64,
) -> Result<(), AppError> {
    if !present(conn)? {
        return Ok(());
    }
    require(conn)?;
    if team.is_empty() {
        return Err(invalid("empty known-deadline team"));
    }
    conn.execute(
        "INSERT INTO composer_draft_known_read_limit(team,until_ms) VALUES(?1,?2)
         ON CONFLICT(team) DO UPDATE SET until_ms=MAX(until_ms,excluded.until_ms)",
        params![team, sql_number(until)?],
    )?;
    Ok(())
}
pub(super) fn known(conn: &Connection, team: &str) -> Result<u64, AppError> {
    require(conn)?;
    let raw: Option<i64> = conn
        .query_row(
            "SELECT until_ms FROM composer_draft_known_read_limit WHERE team=?1",
            [team],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|n| u64::try_from(n).map_err(|_| invalid("negative known read deadline")))
        .transpose()
        .map(|n| n.unwrap_or(0))
}

/// Installation is explicit and private/test-only. Loading v5 never calls this.
#[cfg(any(test, feature = "protected-test-support"))]
pub(super) fn install(store: &DraftStore, account: &str) -> Result<(), AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, account)?;
    if present(&tx)? {
        return Err(invalid("adapter already installed"));
    }
    let format: i64 = tx.query_row(
        "SELECT version FROM composer_draft_continuity_format WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if format != 7 {
        return Err(invalid("adapter upgrade requires v7"));
    }
    // Validate the complete existing protected state before creating sidecars.
    for scope in migration::scopes(&tx, account)? {
        storage::review_row(&tx, &scope)?;
        storage::team(&tx, &scope.key.team)?;
    }
    recovery::attempts(&tx)?;
    archive::inventory(&tx, account)?;
    inventory::reads(&tx)?;
    inventory::bound(
        &tx,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_continuity",
    )?;
    let teams = {
        let mut q = tx.prepare("SELECT team FROM composer_draft_continuity")?;
        q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut limits = Vec::new();
    for team in teams {
        let stream = storage::team(&tx, &team)?;
        limits.push((
            team.clone(),
            stream.read_retry_at_ms.max(recovery::deadline(&tx, &team)?),
        ));
    }
    tx.execute_batch(
        "CREATE TABLE composer_draft_adapter_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=1));
         INSERT INTO composer_draft_adapter_format VALUES(1,1);
         CREATE TABLE composer_draft_writer_lineage(writer_id TEXT PRIMARY KEY,destination TEXT NOT NULL,content TEXT NOT NULL);
         CREATE TABLE composer_draft_local_clock(
           destination TEXT PRIMARY KEY,
           revision INTEGER NOT NULL CHECK(typeof(revision)='integer' AND revision>0),
           generation TEXT,writer_id TEXT,serial INTEGER NOT NULL CHECK(typeof(serial)='integer' AND serial>=0),
           CHECK((generation IS NULL AND writer_id IS NULL AND serial=0) OR (generation IS NOT NULL AND writer_id IS NOT NULL)));
         CREATE TABLE composer_draft_known_read_limit(team TEXT PRIMARY KEY,until_ms INTEGER NOT NULL CHECK(typeof(until_ms)='integer' AND until_ms>=0));
         CREATE TABLE composer_draft_baseline_evidence(id TEXT PRIMARY KEY,team TEXT NOT NULL,content TEXT NOT NULL);
         CREATE TABLE composer_draft_execution_result(id TEXT PRIMARY KEY,team TEXT NOT NULL,content TEXT NOT NULL);
         INSERT INTO composer_draft_local_clock(destination,revision,generation,writer_id,serial)
           SELECT destination,1,NULL,NULL,0 FROM composer_drafts;
         CREATE TRIGGER composer_draft_local_insert AFTER INSERT ON composer_drafts BEGIN
           INSERT INTO composer_draft_local_clock VALUES(NEW.destination,1,NULL,NULL,0)
           ON CONFLICT(destination) DO UPDATE SET revision=revision+1,generation=NULL,writer_id=NULL,serial=0;
         END;
         CREATE TRIGGER composer_draft_local_update AFTER UPDATE ON composer_drafts BEGIN
           INSERT INTO composer_draft_local_clock VALUES(NEW.destination,1,NULL,NULL,0)
           ON CONFLICT(destination) DO UPDATE SET revision=revision+1,generation=NULL,writer_id=NULL,serial=0;
           INSERT INTO composer_draft_local_clock SELECT OLD.destination,1,NULL,NULL,0 WHERE OLD.destination!=NEW.destination
           ON CONFLICT(destination) DO UPDATE SET revision=revision+1,generation=NULL,writer_id=NULL,serial=0;
         END;
         CREATE TRIGGER composer_draft_local_delete AFTER DELETE ON composer_drafts BEGIN
           INSERT INTO composer_draft_local_clock VALUES(OLD.destination,1,NULL,NULL,0)
           ON CONFLICT(destination) DO UPDATE SET revision=revision+1,generation=NULL,writer_id=NULL,serial=0;
         END;
         CREATE TRIGGER composer_draft_local_reenroll AFTER INSERT ON composer_draft_sync BEGIN
           INSERT INTO composer_draft_local_clock VALUES(NEW.destination,1,NULL,NULL,0)
           ON CONFLICT(destination) DO UPDATE SET revision=revision+1,generation=NULL,writer_id=NULL,serial=0;
         END;"
    )?;
    for (team, until) in limits {
        known_if_installed(&tx, &team, until)?;
    }
    tx.commit()?;
    Ok(())
}
