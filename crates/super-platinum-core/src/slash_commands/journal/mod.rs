//! Account-pinned SQLite command receipts, used only through the existing worker.
//! No Admission constructor, draft mutation, dispatch or schema auto-install.
use super::{
    Failure,
    api::Reply,
    controller::{Controller, Journal, Phase, Record},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    collections::BTreeMap,
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
mod cooldown;
mod queue;
mod schema;
mod transaction;
pub use queue::Client;
pub(crate) use queue::Command;

const MAX_RECORDS: usize = 4096;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_RECORD: usize = 1024 * 1024;
fn storage(_: impl std::fmt::Display) -> Failure {
    Failure::Persistence
}
fn valid_account(account: &str) -> bool {
    uuid::Uuid::parse_str(account).is_ok_and(|v| !v.is_nil())
}
fn valid(record: &Record) -> Result<(), Failure> {
    Controller::restore(record.clone()).map(|_| ())
}
fn unresolved(record: &Record) -> bool {
    matches!(
        record.phase(),
        Phase::DispatchPossible | Phase::Unknown | Phase::Completed(Reply::Unknown { .. })
    )
}
fn rate_owner(record: &Record) -> String {
    // All destinations and apps in one account/workspace/user share the gate.
    serde_json::to_string(&(
        &record.owner().account,
        &record.owner().team,
        &record.owner().user,
    ))
    .expect("string tuple")
}
#[derive(Clone)]
struct Stamp {
    run: String,
    mono: i64,
    wall: i64,
}
fn stamp() -> Result<Stamp, Failure> {
    static START: OnceLock<(String, Instant)> = OnceLock::new();
    let (run, start) = START.get_or_init(|| (uuid::Uuid::new_v4().to_string(), Instant::now()));
    Ok(Stamp {
        run: run.clone(),
        mono: i64::try_from(start.elapsed().as_millis()).map_err(storage)?,
        wall: i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(storage)?
                .as_millis(),
        )
        .map_err(storage)?,
    })
}

/// Explicit additive migration. Must run off UI and only at an owned migration
/// gate. Normal open/Client/load never install or repair missing schema.
pub(crate) fn install(conn: &mut Connection, account: &str) -> Result<(), Failure> {
    schema::install(conn, account)
}
/// Reads are display/recovery data only. Controller::restore never rearms them.
pub(crate) fn load(conn: &mut Connection, account: &str) -> Result<Vec<Record>, Failure> {
    let tx = conn.transaction().map_err(storage)?;
    schema::require(&tx, account)?;
    cooldown::audit(&tx, account)?;
    let rows = transaction::inventory(&tx, account)?
        .into_values()
        .map(|row| row.record)
        .collect();
    tx.commit().map_err(storage)?;
    Ok(rows)
}
pub(crate) fn compare(
    conn: &mut Connection,
    account: &str,
    old: Option<&Record>,
    next: &Record,
) -> Result<(), Failure> {
    // Obtain monotonic time after BEGIN IMMEDIATE acquired its write lock.
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let now = stamp()?;
    match transaction::apply(&tx, account, old, next, &now) {
        // A restart rebase must persist even though admission is denied, or
        // every retry would restart the full waiting interval.
        Err(Failure::RateLimited) => {
            tx.commit().map_err(storage)?;
            Err(Failure::RateLimited)
        }
        Err(error) => Err(error),
        Ok(()) => tx.commit().map_err(storage),
    }
}

/// Named transaction join, not a callback or a second commit. Private to core;
/// the protected operation owns BEGIN IMMEDIATE and the sole commit.
pub(crate) fn protected_dispatch(
    conn: &Connection,
    account: &str,
    old: &Record,
    next: &Record,
) -> Result<(), Failure> {
    if conn.is_autocommit()
        || !matches!(old.phase(), Phase::Prepared)
        || !matches!(next.phase(), Phase::DispatchPossible)
    {
        return Err(Failure::NoReplay);
    }
    transaction::apply(conn, account, Some(old), next, &stamp()?)
}
/// Terminal receipt and cooldown join the caller's one protected transaction.
/// This creates no execution permission and never admits a replay.
pub(crate) fn protected_complete(
    conn: &Connection,
    account: &str,
    old: &Record,
    next: &Record,
) -> Result<(), Failure> {
    if conn.is_autocommit()
        || !matches!(old.phase(), Phase::DispatchPossible)
        || !matches!(
            next.phase(),
            Phase::NotSent | Phase::Completed(_) | Phase::Unknown
        )
        || next.retired()
    {
        return Err(Failure::NoReplay);
    }
    transaction::apply(conn, account, Some(old), next, &stamp()?)
}
/// Retire an already settled terminal in the archival owner's transaction.
/// No cooldown update, fresh dispatch or independent commit.
pub(crate) fn protected_retire(
    conn: &Connection,
    account: &str,
    terminal: &Record,
) -> Result<Record, Failure> {
    if conn.is_autocommit() {
        return Err(Failure::NoReplay);
    }
    let next = terminal.protected_retirement()?;
    transaction::apply(conn, account, Some(terminal), &next, &stamp()?)?;
    Ok(next)
}
pub(crate) fn protected_inventory(
    conn: &Connection,
    account: &str,
) -> Result<Vec<Record>, Failure> {
    schema::require(conn, account)?;
    cooldown::audit(conn, account)?;
    Ok(transaction::inventory(conn, account)?
        .into_values()
        .map(|entry| entry.record)
        .collect())
}

#[cfg(test)]
mod tests;
