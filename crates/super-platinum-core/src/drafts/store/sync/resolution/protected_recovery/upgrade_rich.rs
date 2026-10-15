//! Private barrier-only RichV1 storage unit. No runtime constructor or route.
use super::*;
use serde::{Deserialize, Serialize};
#[path = "upgrade_rich_authority.rs"]
mod authority;
#[path = "upgrade_rich_layout.rs"]
mod layout;
#[cfg(test)]
#[path = "upgrade_rich_tests.rs"]
mod tests;
use authority::Authority;

/// Observation only. No connection, writer, dispatch or migration capability.
struct Inspection {
    owner: Arc<Owned>,
    _file: File,
    identity: owned::Identity,
    image: Image,
    receipt: layout::Receipt,
}
impl Inspection {
    fn anchors(&self) -> Result<&snapshot::Table, AppError> {
        self.image.table(layout::ANCHOR)
    }
}

/// Cold/Owned/C3 authority has no production constructor. Existing queue IDs,
/// Open generations and decoded database receipts cannot create Authority.
fn migrate(authority: Authority) -> Result<Inspection, AppError> {
    run(authority, layout::Fault::default())
}
fn run(authority: Authority, fault: layout::Fault) -> Result<Inspection, AppError> {
    let owner = authority.owner().clone();
    authority.preflight()?;
    let conn = Connection::open_with_flags(owner.path(), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(std::time::Duration::from_millis(100))?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA legacy_alter_table=OFF; PRAGMA locking_mode=EXCLUSIVE;")?;
    // Exclusive SQL/file admission is supplemental: it cannot revoke old text
    // already in memory. The missing production cold proof must cover that.
    let tx = rusqlite::Transaction::new_unchecked(&conn, rusqlite::TransactionBehavior::Exclusive)?;
    let before = Image::read(&tx)?;
    if &before != authority.expected() {
        return Err(invalid("rich migration review changed"));
    }
    let receipt = layout::receipt(before, owner.account())?;
    layout::install(&tx, &receipt, &fault)?;
    let after = Image::read(&tx)?;
    layout::audit(&after, owner.account())?;
    authority.identity_current()?;
    // Sampled identity check; not atomic with external process lifetimes.
    tx.commit()?;
    drop(conn);
    inspect(owner)
}

/// Compatible reopening uses the owned path and read-only SQLite, never ordinary
/// DraftStore::open (whose error is the intentionally retained older-reader gate).
fn inspect(owner: Arc<Owned>) -> Result<Inspection, AppError> {
    owner.check()?;
    let (file, identity) = owner
        .retain_db()?
        .ok_or_else(|| invalid("rich database missing"))?;
    owner.readonly_preflight(&file)?;
    let conn = Connection::open_with_flags(owner.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_millis(100))?;
    let tx = rusqlite::Transaction::new_unchecked(&conn, rusqlite::TransactionBehavior::Deferred)?;
    let image = Image::read(&tx)?;
    let receipt = layout::audit(&image, owner.account())?;
    tx.commit()?;
    owner.check()?;
    if owner.retain_db()?.as_ref().map(|(_, id)| id) != Some(&identity) {
        return Err(invalid("rich database replaced during inspection"));
    }
    Ok(Inspection {
        owner,
        _file: file,
        identity,
        image,
        receipt,
    })
}
