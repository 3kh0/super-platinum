//! Blocking SQLite API. Commit Begin::Started before the one network dispatch.
//! A restored receipt only blocks writes; no read can manufacture acceptance.
use super::{Checkpoint, Scope};
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{fs::OpenOptions, path::Path, time::Duration};

const MAX_RECEIPTS: i64 = 32;
const MAX_BODY: usize = crate::canvas::wire::MAX_BYTES * 5 + 128 * 1024;

mod confirmed;
pub use confirmed::ReplacementRestore;
pub mod annotations;
mod comments;
pub mod replies;
pub use comments::{CommentBegin, CommentDecision, CommentReceipt, CommentReview};
mod review;
pub use review::{
    EndReview, EndReviewAppend, LocalDecision, LocalReview, SplitReview, StartTypingReview,
};

#[derive(Clone, PartialEq, Eq)]
pub struct Receipt {
    id: String,
    checkpoint: Checkpoint,
    body: String,
}
impl Receipt {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }
    pub fn operation_id(&self) -> &str {
        &self.id
    }
}
pub enum Begin {
    Started(Receipt),
    Blocked(Receipt),
}
pub struct Journal {
    conn: Connection,
    account: String,
}
fn invalid() -> AppError {
    std::io::Error::other("Canvas recovery storage is invalid").into()
}
fn account(value: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| invalid())
}
fn scope_valid(scope: &Scope) -> Result<(), AppError> {
    for value in [&scope.workspace, &scope.user, &scope.file] {
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(invalid());
        }
    }
    Ok(())
}
impl Journal {
    pub fn open_default(owner: &str) -> Result<Self, AppError> {
        Self::open(
            crate::config::config_dir()?
                .join(format!("canvas-receipts-{}.sqlite", account(owner)?)),
            owner,
        )
    }
    /// Inject a temporary path for every test and disconnected fixture.
    pub fn open(path: impl AsRef<Path>, owner: &str) -> Result<Self, AppError> {
        let owner = account(owner)?;
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        drop(file);
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA journal_mode=DELETE;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if !matches!(version, 0 | 1 | 2 | 3 | 4 | 5 | 6) {
            return Err(invalid());
        }
        if version >= 4 {
            let clocks: i64 = tx.query_row(
                "SELECT count(*) FROM reply_clock WHERE singleton=1 AND value>=0",
                [],
                |r| r.get(0),
            )?;
            if clocks != 1 {
                return Err(invalid());
            }
        }
        if version >= 5 {
            // Missing current-version tables are corruption, not a fresh store.
            tx.query_row("SELECT count(*) FROM annotation_attempts", [], |r| {
                r.get::<_, i64>(0)
            })?;
            tx.query_row("SELECT count(*) FROM annotation_history", [], |r| {
                r.get::<_, i64>(0)
            })?;
        }
        if version == 6 {
            tx.query_row("SELECT count(*) FROM confirmed_replacements", [], |r| {
                r.get::<_, i64>(0)
            })?;
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),account TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS receipts(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,
          operation TEXT NOT NULL UNIQUE,body TEXT NOT NULL,PRIMARY KEY(workspace,user,file));
          CREATE TABLE IF NOT EXISTS local_reviews(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,
          operation TEXT NOT NULL UNIQUE,body TEXT NOT NULL,observed TEXT NOT NULL,reviewed_at INTEGER NOT NULL,
          decision TEXT NOT NULL CHECK(decision='local_unlock_outcome_unknown'));
          CREATE TABLE IF NOT EXISTS comment_receipts(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,
          operation TEXT NOT NULL UNIQUE,body TEXT NOT NULL,PRIMARY KEY(workspace,user,file));
          CREATE TABLE IF NOT EXISTS comment_reviews(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,
          operation TEXT NOT NULL UNIQUE,body TEXT NOT NULL,observed TEXT NOT NULL,decision TEXT NOT NULL CHECK(decision='local_unlock_outcome_unknown'));
          CREATE TABLE IF NOT EXISTS reply_drafts(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,root TEXT NOT NULL,id TEXT PRIMARY KEY,body TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS reply_draft_owner ON reply_drafts(workspace,user,file,root);
          CREATE TABLE IF NOT EXISTS reply_clock(singleton INTEGER PRIMARY KEY CHECK(singleton=1),value INTEGER NOT NULL CHECK(value>=0));
          INSERT OR IGNORE INTO reply_clock VALUES(1,0);
          CREATE TABLE IF NOT EXISTS reply_captures(id TEXT PRIMARY KEY);
          CREATE TABLE IF NOT EXISTS reply_attempts(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,id TEXT NOT NULL UNIQUE,body TEXT NOT NULL,PRIMARY KEY(workspace,user,file));
          CREATE TABLE IF NOT EXISTS reply_late_results(id TEXT PRIMARY KEY,observed TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS reply_history(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,id TEXT PRIMARY KEY,body TEXT NOT NULL,observed TEXT NOT NULL,status TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS reply_history_owner ON reply_history(workspace,user,file,id);
          CREATE TABLE IF NOT EXISTS annotation_attempts(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,id TEXT NOT NULL UNIQUE,body TEXT NOT NULL,PRIMARY KEY(workspace,user,file));
          CREATE TABLE IF NOT EXISTS annotation_history(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,id TEXT PRIMARY KEY,body TEXT NOT NULL,status TEXT NOT NULL,observed TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS annotation_history_owner ON annotation_history(workspace,user,file,id);
          CREATE TABLE IF NOT EXISTS confirmed_replacements(workspace TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,
          operation TEXT NOT NULL UNIQUE,body TEXT NOT NULL,PRIMARY KEY(workspace,user,file));
          PRAGMA user_version=6;")?;
        tx.execute("INSERT OR IGNORE INTO identity VALUES(1,?1)", [&owner])?;
        let actual: String =
            tx.query_row("SELECT account FROM identity WHERE singleton=1", [], |r| {
                r.get(0)
            })?;
        if actual != owner {
            return Err(invalid());
        }
        tx.commit()?;
        Ok(Self {
            conn,
            account: owner,
        })
    }
    pub fn get(&self, scope: &Scope) -> Result<Option<Receipt>, AppError> {
        read(&self.conn, scope)
    }
    pub fn begin(&mut self, checkpoint: Checkpoint) -> Result<Begin, AppError> {
        checkpoint.validate().map_err(|_| invalid())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(old) = read(&tx, checkpoint.scope())? {
            return Ok(Begin::Blocked(old));
        }
        let count: i64 = tx.query_row(
            "SELECT (SELECT count(*) FROM receipts)+(SELECT count(*) FROM comment_receipts)+(SELECT count(*) FROM reply_attempts)+(SELECT count(*) FROM annotation_attempts)",
            [],
            |r| r.get(0),
        )?;
        if count >= MAX_RECEIPTS {
            return Err(invalid());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let body = serde_json::to_string(&checkpoint)?;
        if body.len() > MAX_BODY {
            return Err(invalid());
        }
        let s = checkpoint.scope();
        tx.execute(
            "INSERT INTO receipts VALUES(?1,?2,?3,?4,?5)",
            params![s.workspace, s.user, s.file, id, body],
        )?;
        tx.commit()?;
        Ok(Begin::Started(Receipt {
            id,
            checkpoint,
            body,
        }))
    }
    /// Caller must own definite non-dispatch or sealed ACK plus exact canonical
    /// proof. Unknown delivery and matching visible content are not authority.
    /// Removal failure keeps the lock. An old receipt cannot remove a new one.
    pub fn complete(&mut self, receipt: &Receipt) -> Result<bool, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scope = receipt.checkpoint.scope();
        let changed=tx.execute("DELETE FROM receipts WHERE workspace=?1 AND user=?2 AND file=?3 AND operation=?4 AND body=?5",
            params![scope.workspace,scope.user,scope.file,receipt.id,receipt.body])?;
        tx.commit()?;
        Ok(changed == 1)
    }
}
fn read(conn: &Connection, scope: &Scope) -> Result<Option<Receipt>, AppError> {
    scope_valid(scope)?;
    if annotations::present(conn, scope)?
        || comments::present(conn, scope)?
        || replies::present(conn, scope)?
    {
        return Err(invalid());
    }
    let row:Option<(String,String)>=conn.query_row(
        "SELECT operation,CASE WHEN length(CAST(body AS BLOB))<=?4 THEN body ELSE NULL END FROM receipts WHERE workspace=?1 AND user=?2 AND file=?3",
        params![scope.workspace,scope.user,scope.file,MAX_BODY as i64],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((id, body)) = row else {
        return Ok(None);
    };
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(invalid());
    }
    let checkpoint: Checkpoint = serde_json::from_str(&body)?;
    checkpoint.validate().map_err(|_| invalid())?;
    if checkpoint.scope() != scope {
        return Err(invalid());
    }
    Ok(Some(Receipt {
        id,
        checkpoint,
        body,
    }))
}
