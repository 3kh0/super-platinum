//! Private nesting boundary: facade metadata and the existing reducer transition
//! commit together. No connection or transaction callback crosses the public API.
use super::*;
use std::ops::Deref;

pub(super) enum Atomic<'a> {
    Root(rusqlite::Transaction<'a>),
    Nested {
        conn: &'a Connection,
        name: String,
        finished: bool,
    },
}
pub(super) fn begin(conn: &Connection) -> Result<Atomic<'_>, AppError> {
    if conn.is_autocommit() {
        Ok(Atomic::Root(rusqlite::Transaction::new_unchecked(
            conn,
            TransactionBehavior::Immediate,
        )?))
    } else {
        // Generated identifier only. Never interpolate caller text into SQL.
        let name = format!("protected_{}", uuid::Uuid::new_v4().simple());
        conn.execute_batch(&format!("SAVEPOINT {name}"))?;
        Ok(Atomic::Nested {
            conn,
            name,
            finished: false,
        })
    }
}
impl Deref for Atomic<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            Self::Root(tx) => tx,
            Self::Nested { conn, .. } => conn,
        }
    }
}
impl Atomic<'_> {
    pub(super) fn commit(mut self) -> Result<(), AppError> {
        match &mut self {
            Self::Root(tx) => {
                // Transaction cannot move out of a Drop type. Commit the exact
                // root and disable rusqlite's rollback-on-drop afterwards.
                tx.execute_batch("COMMIT")?;
                tx.set_drop_behavior(rusqlite::DropBehavior::Ignore);
            }
            Self::Nested {
                conn,
                name,
                finished,
            } => {
                conn.execute_batch(&format!("RELEASE SAVEPOINT {name}"))?;
                *finished = true;
            }
        }
        Ok(())
    }
}
impl Drop for Atomic<'_> {
    fn drop(&mut self) {
        if let Self::Nested {
            conn,
            name,
            finished: false,
        } = self
        {
            let _ = conn.execute_batch(&format!(
                "ROLLBACK TO SAVEPOINT {name}; RELEASE SAVEPOINT {name}"
            ));
        }
        // Root's Transaction still owns its ordinary rollback-on-drop behavior.
    }
}
