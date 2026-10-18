//! Explicit v2 retention schema and bounded raw binding inventory.
use super::*;
pub(super) fn format(conn: &Connection, version: i64) -> Result<(), AppError> {
    let n:i64=conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('composer_draft_slash_archives','composer_draft_slash_attestations')",[],|r|r.get(0))?;
    let remote:i64=conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('composer_draft_slash_remote_jobs','composer_draft_slash_remote_archives')",[],|r|r.get(0))?;
    if !matches!((version, n, remote), (1, 0, 0) | (2, 2, 0) | (3, 2, 2)) {
        return Err(invalid("partial/future slash retention schema"));
    }
    Ok(())
}
pub(super) fn v2(conn: &Connection) -> Result<bool, AppError> {
    let version: i64 = conn.query_row(
        "SELECT version FROM composer_draft_slash_format WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    format(conn, version)?;
    Ok(version >= 2)
}
pub(super) fn require(conn: &Connection, account: &str) -> Result<(), AppError> {
    bindings::require(conn, account)?;
    if !v2(conn)? {
        return Err(invalid("slash retention migration required"));
    }
    Ok(())
}
pub(super) fn migrate(store: &DraftStore, account: &str) -> Result<(), Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    migrate_transaction(&tx, account)?;
    disk(tx.commit())
}
pub(super) fn migrate_transaction(conn: &Connection, account: &str) -> Result<(), Error> {
    if conn.is_autocommit() {
        return Err(Error::Storage);
    }
    disk(bindings::require(conn, account))?;
    disk(bindings::all(conn, account))?;
    disk(completion::audit(conn, account))?;
    if !disk(v2(conn))? {
        disk(conn.execute_batch("CREATE TABLE composer_draft_slash_archives(intent TEXT PRIMARY KEY,destination TEXT NOT NULL,enrollment TEXT NOT NULL,binding TEXT NOT NULL,content TEXT NOT NULL);
          CREATE TABLE composer_draft_slash_attestations(intent TEXT PRIMARY KEY,content TEXT NOT NULL);
          UPDATE composer_draft_slash_format SET version=2 WHERE singleton=1 AND version=1;"))?;
    }
    disk(archive_storage::audit(conn, account))?;
    Ok(())
}
pub(super) fn binding_rows(
    conn: &Connection,
) -> Result<Vec<(String, String, String, String)>, AppError> {
    let source = if remote::enabled(conn)? {
        "SELECT intent,destination,enrollment,content FROM composer_draft_slash_bindings UNION ALL SELECT intent,destination,enrollment,binding AS content FROM composer_draft_slash_archives UNION ALL SELECT intent,destination,enrollment,binding AS content FROM composer_draft_slash_remote_archives"
    } else if v2(conn)? {
        "SELECT intent,destination,enrollment,content FROM composer_draft_slash_bindings UNION ALL SELECT intent,destination,enrollment,binding AS content FROM composer_draft_slash_archives"
    } else {
        "SELECT intent,destination,enrollment,content FROM composer_draft_slash_bindings"
    };
    let (n,size):(i64,i64)=conn.query_row(&format!("SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(content AS BLOB))+LENGTH(CAST(intent AS BLOB))+LENGTH(CAST(destination AS BLOB))+LENGTH(CAST(enrollment AS BLOB))),0) FROM ({source})"),[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if !(0..=4096).contains(&n) || !(0..=16777216).contains(&size) {
        return Err(invalid("combined binding history bound"));
    }
    let mut q=conn.prepare(&format!("SELECT intent,destination,enrollment,CASE WHEN LENGTH(CAST(content AS BLOB))<=1048576 THEN content ELSE NULL END FROM ({source})"))?;
    Ok(
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<_>, _>>()?,
    )
}
pub(super) fn binding_raw(conn: &Connection, id: &str) -> Result<String, AppError> {
    let rows = binding_rows(conn)?
        .into_iter()
        .filter(|r| r.0 == id)
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return Err(invalid("missing/duplicate binding intent"));
    }
    Ok(rows.into_iter().next().unwrap().3)
}
pub(super) fn archived(conn: &Connection, id: &str) -> Result<Option<String>, AppError> {
    Ok(conn.query_row("SELECT CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_archives WHERE intent=?1",[id],|r|r.get(0)).optional()?)
}
pub(super) fn active(
    conn: &Connection,
    scope: &Scope,
    id: &str,
    raw: &str,
) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM composer_draft_slash_bindings WHERE intent=?1 AND destination=?2 AND content=?3)",params![id,serde_json::to_string(&scope.key)?,raw],|r|r.get(0))?)
}
