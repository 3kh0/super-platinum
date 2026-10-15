use super::*;
const TABLES: [&str; 3] = ["sp_slash_meta", "sp_slash_intents", "sp_slash_cooldowns"];
pub(super) fn install(conn: &mut Connection, account: &str) -> Result<(), Failure> {
    if !valid_account(account) {
        return Err(Failure::Invalid);
    }
    // This changes only this connection; SQLite commit durability is mandatory.
    conn.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(storage)?;
    conn.pragma_update(None, "synchronous", "FULL")
        .map_err(storage)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let count:i64=tx.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('sp_slash_meta','sp_slash_intents','sp_slash_cooldowns')",[],|r|r.get(0)).map_err(storage)?;
    if count == 3 {
        require(&tx, account)?;
        transaction::inventory(&tx, account)?;
        cooldown::audit(&tx, account)?;
        tx.commit().map_err(storage)?;
        return Ok(());
    }
    if count != 0 {
        return Err(Failure::Invalid);
    }
    draft_account(&tx, account)?;
    tx.execute_batch("CREATE TABLE sp_slash_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL,account TEXT NOT NULL);
        CREATE TABLE sp_slash_intents(id TEXT PRIMARY KEY,scope TEXT NOT NULL,version INTEGER NOT NULL CHECK(version>0),created_run TEXT NOT NULL,body BLOB NOT NULL);
        CREATE INDEX sp_slash_scope ON sp_slash_intents(scope);
        CREATE TABLE sp_slash_cooldowns(scope TEXT PRIMARY KEY,permanent INTEGER NOT NULL CHECK(permanent IN(0,1)),duration_ms INTEGER,run TEXT,until_mono INTEGER,until_wall INTEGER);").map_err(storage)?;
    tx.execute("INSERT INTO sp_slash_meta VALUES(1,1,?1)", [account])
        .map_err(storage)?;
    require(&tx, account)?;
    tx.commit().map_err(storage)
}
pub(super) fn require(conn: &Connection, account: &str) -> Result<(), Failure> {
    if !valid_account(account) {
        return Err(Failure::Invalid);
    }
    let synchronous: i64 = conn
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .map_err(storage)?;
    if synchronous < 2 {
        return Err(Failure::Persistence);
    }
    let journal: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .map_err(storage)?;
    let in_memory: bool = conn
        .query_row(
            "SELECT file='' FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !matches!(journal.as_str(), "delete" | "truncate" | "persist" | "wal")
        && !(cfg!(test) && in_memory && journal == "memory")
    {
        return Err(Failure::Persistence);
    }
    let count: i64 = conn
        .query_row("SELECT count(*) FROM sp_slash_meta", [], |r| r.get(0))
        .map_err(storage)?;
    let (singleton, version, stored): (i64, i64, String) = conn
        .query_row(
            "SELECT singleton,version,account FROM sp_slash_meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(storage)?;
    if count != 1 || singleton != 1 || version != 1 || stored != account {
        return Err(Failure::Invalid);
    }
    for table in TABLES {
        let present: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if !present {
            return Err(Failure::Invalid);
        }
    }
    draft_account(conn, account)
}
fn draft_account(conn: &Connection, account: &str) -> Result<(), Failure> {
    let present:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='composer_draft_owner')",[],|r|r.get(0)).map_err(storage)?;
    if !present {
        return Ok(());
    }
    let count: i64 = conn
        .query_row("SELECT count(*) FROM composer_draft_owner", [], |r| {
            r.get(0)
        })
        .map_err(storage)?;
    if count == 0 {
        return Ok(());
    }
    let (singleton, stored): (i64, String) = conn
        .query_row(
            "SELECT singleton,account FROM composer_draft_owner",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    if count != 1 || singleton != 1 || stored != account {
        return Err(Failure::Stale);
    }
    Ok(())
}
