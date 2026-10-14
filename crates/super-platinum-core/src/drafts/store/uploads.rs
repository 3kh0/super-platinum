//! Durable upload recovery, separate from disposable transcript cache.
//!
//! Only `reserve` creates a row. Finalization and retirement require its issued
//! ticket, so late updates cannot resurrect a retired upload. A reservation is
//! not proof of delivery, nor permission to replay an earlier publication.
use super::{DraftStore, sync};
use crate::error::AppError;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
mod model;
pub use super::worker::uploads::{finalize, load, reserve, retire};
pub use model::{Attachment, Draft, Reservation, Stored, Ticket};
#[cfg(test)]
pub(super) mod tests;
pub const MAX_ACTIVE: usize = 128;
const FORMAT: i64 = 1;

pub(super) fn migrate(conn: &rusqlite::Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS upload_journal_identity (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1), incarnation TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS upload_receipts (
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,
        client_id TEXT NOT NULL UNIQUE, format_version INTEGER NOT NULL,
        draft TEXT NOT NULL, file_ids TEXT);",
    )?;
    Ok(())
}
fn journal(conn: &rusqlite::Connection, account: &str) -> Result<String, AppError> {
    sync::owner(conn, account)?;
    conn.execute(
        "INSERT OR IGNORE INTO upload_journal_identity(singleton,incarnation) VALUES(1,?1)",
        [uuid::Uuid::new_v4().to_string()],
    )?;
    let id: String = conn.query_row(
        "SELECT incarnation FROM upload_journal_identity WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(sync::invalid("invalid upload journal identity"));
    }
    Ok(id)
}
fn read(
    conn: &rusqlite::Connection,
    account: &str,
    journal: &str,
    sequence: i64,
) -> Result<Option<Stored>, AppError> {
    let row: Option<(String, i64, String, Option<String>)> = conn
        .query_row(
            "SELECT client_id,format_version,draft,file_ids FROM upload_receipts WHERE sequence=?1",
            [sequence],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((client, format, draft, files)) = row else {
        return Ok(None);
    };
    if format != FORMAT || sequence <= 0 {
        return Err(sync::invalid("unsupported upload receipt format"));
    }
    let draft: Draft = serde_json::from_str(&draft)?;
    draft.validate()?;
    if draft.account_id != account || draft.client_msg_id != client {
        return Err(sync::invalid("upload receipt owner mismatch"));
    }
    let files: Option<Vec<String>> = files.map(|s| serde_json::from_str(&s)).transpose()?;
    if let Some(files) = &files {
        model::validate_files(files, draft.attachments.len())?;
    }
    Ok(Some(Stored {
        ticket: Ticket {
            account_id: account.into(),
            journal_id: journal.into(),
            sequence: sequence as u64,
            client_msg_id: client,
        },
        draft,
        file_ids: files,
    }))
}
fn ticket_row(conn: &rusqlite::Connection, ticket: &Ticket) -> Result<Option<Stored>, AppError> {
    let identity = journal(conn, &ticket.account_id)?;
    if identity != ticket.journal_id {
        return Ok(None);
    }
    let seq = i64::try_from(ticket.sequence).map_err(|_| sync::invalid("invalid upload ticket"))?;
    Ok(read(conn, &ticket.account_id, &identity, seq)?.filter(|row| row.ticket == *ticket))
}
impl DraftStore {
    /// New explicit send only. `created=false` recovers an existing reservation;
    /// it MUST NOT authorize another publication. Lost acknowledgement: load,
    /// never automatically repeat reserve or the network operation.
    pub fn reserve_upload(&mut self, draft: &Draft) -> Result<Reservation, AppError> {
        draft.validate()?;
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let identity = journal(&tx, &draft.account_id)?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT sequence FROM upload_receipts WHERE client_id=?1",
                [&draft.client_msg_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(sequence) = existing {
            let stored = read(&tx, &draft.account_id, &identity, sequence)?
                .ok_or_else(|| sync::invalid("upload reservation disappeared"))?;
            if stored.draft != *draft {
                return Err(sync::invalid(
                    "upload client identity already belongs to another draft",
                ));
            }
            tx.commit()?;
            return Ok(Reservation {
                created: false,
                stored,
            });
        }
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM upload_receipts", [], |row| row.get(0))?;
        if count >= MAX_ACTIVE as i64 {
            return Err(sync::invalid("too many unresolved upload receipts"));
        }
        tx.execute(
            "INSERT INTO upload_receipts(client_id,format_version,draft) VALUES(?1,?2,?3)",
            params![draft.client_msg_id, FORMAT, serde_json::to_string(draft)?],
        )?;
        let sequence = tx.last_insert_rowid();
        let stored = read(&tx, &draft.account_id, &identity, sequence)?
            .ok_or_else(|| sync::invalid("upload reservation disappeared"))?;
        tx.commit()?;
        Ok(Reservation {
            created: true,
            stored,
        })
    }
    /// Persist validated private-finalization IDs before publication. Never
    /// inserts: None means this ticket was retired or belongs to an old journal.
    pub fn finalize_upload(
        &mut self,
        ticket: &Ticket,
        mut file_ids: Vec<String>,
    ) -> Result<Option<Stored>, AppError> {
        file_ids.sort();
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut stored) = ticket_row(&tx, ticket)? else {
            tx.commit()?;
            return Ok(None);
        };
        model::validate_files(&file_ids, stored.draft.attachments.len())?;
        if let Some(existing) = &stored.file_ids {
            if existing != &file_ids {
                return Err(sync::invalid("upload finalized identity cannot change"));
            }
        } else {
            tx.execute(
                "UPDATE upload_receipts SET file_ids=?1 WHERE sequence=?2",
                params![
                    serde_json::to_string(&file_ids)?,
                    i64::try_from(ticket.sequence)
                        .map_err(|_| sync::invalid("invalid upload ticket"))?
                ],
            )?;
            stored.file_ids = Some(file_ids);
        }
        tx.commit()?;
        Ok(Some(stored))
    }
    /// Exact retirement after canonical proof or a definite pre-send rejection.
    /// A stale pre-finalization snapshot cannot retire a finalized receipt.
    pub fn retire_upload(&mut self, expected: &Stored) -> Result<bool, AppError> {
        expected.draft.validate()?;
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(current) = ticket_row(&tx, &expected.ticket)? else {
            tx.commit()?;
            return Ok(false);
        };
        if current != *expected {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute(
            "DELETE FROM upload_receipts WHERE sequence=?1",
            [i64::try_from(expected.ticket.sequence)
                .map_err(|_| sync::invalid("invalid upload ticket"))?],
        )?;
        // AUTOINCREMENT's sqlite_sequence survives deletion. No tombstone rows
        // or lifetime send limit are needed; only active receipts occupy space.
        tx.commit()?;
        Ok(true)
    }
    /// Background load only. Every returned item remains unconfirmed, including
    /// reservations that never obtained final IDs; loading authorizes no send.
    pub fn load_uploads(&mut self, account: &str) -> Result<Vec<Stored>, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let identity = journal(&tx, account)?;
        let sequences = {
            let mut q =
                tx.prepare("SELECT sequence FROM upload_receipts ORDER BY sequence LIMIT ?1")?;
            q.query_map([MAX_ACTIVE as i64 + 1], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        if sequences.len() > MAX_ACTIVE {
            return Err(sync::invalid("upload receipt journal exceeds its limit"));
        }
        let rows = sequences
            .into_iter()
            .map(|seq| {
                read(&tx, account, &identity, seq)?
                    .ok_or_else(|| sync::invalid("upload reservation disappeared"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit()?;
        Ok(rows)
    }
}
