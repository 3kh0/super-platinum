//! Durable recovery for saved-file changes whose outcome may be unknown.
//!
//! Call `begin` on a blocking worker and commit its receipt BEFORE dispatching a
//! write. Only `Begin::Started` permits that one dispatch. Reopening a menu or
//! this journal never authorizes a retry; unresolved receipts require readback
//! or explicit retirement. Local operation IDs are not Slack idempotency keys.
use std::{fs::OpenOptions, path::Path, time::Duration};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

const MAX_TERMINAL_RECEIPTS: i64 = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileScope {
    pub team_id: String,
    pub user_id: String,
    pub file_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Save,
    Remove,
    Complete,
    Restore,
    Archive,
    Unarchive,
    Reminder(i64),
}
impl From<crate::later::SavedAction> for Action {
    fn from(action: crate::later::SavedAction) -> Self {
        use crate::later::SavedAction as S;
        match action {
            S::Save => Self::Save,
            S::Remove => Self::Remove,
            S::Complete => Self::Complete,
            S::Restore => Self::Restore,
            S::Archive => Self::Archive,
            S::Unarchive => Self::Unarchive,
            S::Reminder(due) => Self::Reminder(due),
        }
    }
}
impl From<Action> for crate::later::SavedAction {
    fn from(action: Action) -> Self {
        match action {
            Action::Save => Self::Save,
            Action::Remove => Self::Remove,
            Action::Complete => Self::Complete,
            Action::Restore => Self::Restore,
            Action::Archive => Self::Archive,
            Action::Unarchive => Self::Unarchive,
            Action::Reminder(due) => Self::Reminder(due),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub title: String,
    pub action: Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Also covers a crash after persistence but before network dispatch.
    Uncertain,
    Confirmed,
    Rejected,
    /// The user chose to stop tracking; this says nothing about Slack's state.
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Confirmed,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub operation_id: String,
    pub scope: FileScope,
    pub operation: Operation,
    pub status: Status,
    pub created_at: u64,
    pub updated_at: u64,
    /// Unix seconds, so cooldowns survive app restarts.
    pub retry_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Begin {
    Started(Receipt),
    /// An unresolved operation or an unexpired cooldown prevents dispatch.
    Blocked(Receipt),
}

/// Synchronous disk API: open and use this on a blocking worker, never render.
/// Contains titles, actions and stable IDs only, never credentials, URLs or file bytes.
pub struct Journal {
    conn: Connection,
}

fn invalid(message: &'static str) -> AppError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}

fn account_id(account: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(account)
        .map(|id| id.to_string())
        .map_err(|_| invalid("invalid account identity for operation receipts"))
}

fn validate_scope(scope: &FileScope) -> Result<(), AppError> {
    for id in [&scope.team_id, &scope.user_id, &scope.file_id] {
        if id.is_empty() || id.len() > 512 || id.trim() != id || id.chars().any(char::is_control) {
            return Err(invalid("invalid file operation scope"));
        }
    }
    Ok(())
}

fn validate_operation(operation: &Operation) -> Result<(), AppError> {
    if operation.title.len() > 16_384 || matches!(operation.action,Action::Reminder(due) if due<0) {
        return Err(invalid("invalid saved file operation"));
    }
    Ok(())
}

fn timestamp(value: u64) -> Result<i64, AppError> {
    i64::try_from(value).map_err(|_| invalid("invalid operation receipt timestamp"))
}

impl Journal {
    fn default_path(account: &str) -> Result<std::path::PathBuf, AppError> {
        Ok(crate::config::config_dir()?.join(format!(
            "saved-file-receipts-{}.sqlite",
            account_id(account)?
        )))
    }

    pub fn open_default(account: &str) -> Result<Self, AppError> {
        Self::open(Self::default_path(account)?, account)
    }

    /// An injected path keeps tests and disconnected fixtures off user data.
    /// The database also records its account identity and rejects mismatches.
    pub fn open(path: impl AsRef<Path>, account: &str) -> Result<Self, AppError> {
        let account = account_id(account)?;
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
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
        let version: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != 0 && version != 1 {
            return Err(invalid("unsupported operation receipt schema"));
        }
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS identity (singleton INTEGER PRIMARY KEY CHECK(singleton=1), account TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS receipts (
                team TEXT NOT NULL, user TEXT NOT NULL, file TEXT NOT NULL,
                operation_id TEXT NOT NULL UNIQUE, terminal INTEGER NOT NULL,
                updated_at INTEGER NOT NULL, retry_at INTEGER NOT NULL, body TEXT NOT NULL,
                PRIMARY KEY(team,user,file));
             PRAGMA user_version=1;",
        )?;
        tx.execute("INSERT OR IGNORE INTO identity VALUES(1,?1)", [&account])?;
        let owner: String = tx.query_row(
            "SELECT account FROM identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        if owner != account {
            return Err(invalid("operation receipt account mismatch"));
        }
        tx.commit()?;
        Ok(Self { conn })
    }

    pub fn get(&self, scope: &FileScope) -> Result<Option<Receipt>, AppError> {
        validate_scope(scope)?;
        read(&self.conn, scope)
    }

    pub fn unresolved(&self, team: &str, user: &str) -> Result<Vec<Receipt>, AppError> {
        self.inventory(team, user, None)
    }

    pub fn recoverable(&self, team: &str, user: &str, now: u64) -> Result<Vec<Receipt>, AppError> {
        timestamp(now)?;
        self.inventory(team, user, Some(now))
    }

    fn inventory(
        &self,
        team: &str,
        user: &str,
        now: Option<u64>,
    ) -> Result<Vec<Receipt>, AppError> {
        validate_scope(&FileScope {
            team_id: team.into(),
            user_id: user.into(),
            file_id: "_".into(),
        })?;
        let mut statement = self.conn.prepare(
            "SELECT body FROM receipts WHERE team=?1 AND user=?2 AND (terminal=0 OR retry_at>?3) ORDER BY updated_at,operation_id",
        )?;
        let rows = statement.query_map(
            params![
                team,
                user,
                now.map(timestamp).transpose()?.unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )?;
        rows.map(|row| {
            let receipt = decode(&row?)?;
            if receipt.scope.team_id != team
                || receipt.scope.user_id != user
                || (receipt.status != Status::Uncertain
                    && now.is_none_or(|now| receipt.retry_at <= now))
            {
                return Err(invalid("operation receipt scope or status mismatch"));
            }
            Ok(receipt)
        })
        .collect()
    }

    /// Call only for an explicit user write intent, then dispatch at most once.
    /// Failure to persist MUST abort dispatch. Unknown receipts block even when
    /// the requested operation or title differs from the previous one.
    pub fn begin(
        &mut self,
        scope: FileScope,
        operation: Operation,
        now: u64,
    ) -> Result<Begin, AppError> {
        validate_scope(&scope)?;
        validate_operation(&operation)?;
        timestamp(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(receipt) = read(&tx, &scope)?
            && (receipt.status == Status::Uncertain || receipt.retry_at > now)
        {
            return Ok(Begin::Blocked(receipt));
        }
        let receipt = Receipt {
            operation_id: uuid::Uuid::new_v4().to_string(),
            scope,
            operation,
            status: Status::Uncertain,
            created_at: now,
            updated_at: now,
            retry_at: 0,
        };
        write(&tx, &receipt)?;
        prune(&tx, now)?;
        tx.commit()?;
        Ok(Begin::Started(receipt))
    }

    /// Only positive readback/write acknowledgement or a definite rejection
    /// may resolve a receipt. A missing file or unchanged name is not proof.
    pub fn resolve(
        &mut self,
        scope: &FileScope,
        operation_id: &str,
        outcome: Outcome,
        now: u64,
        retry_at: u64,
    ) -> Result<bool, AppError> {
        let status = match outcome {
            Outcome::Confirmed => Status::Confirmed,
            Outcome::Rejected => Status::Rejected,
        };
        self.update(scope, operation_id, status, now, retry_at)
    }

    /// Preserve uncertainty while persisting a readback/request cooldown.
    pub fn defer_check(
        &mut self,
        scope: &FileScope,
        operation_id: &str,
        now: u64,
        retry_at: u64,
    ) -> Result<bool, AppError> {
        self.update(scope, operation_id, Status::Uncertain, now, retry_at)
    }

    /// Atomically reserve a read-only check across independently mounted menus.
    pub fn reserve_check(
        &mut self,
        scope: &FileScope,
        operation_id: &str,
        now: u64,
    ) -> Result<Option<Receipt>, AppError> {
        validate_scope(scope)?;
        timestamp(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut receipt) = read(&tx, scope)? else {
            return Ok(None);
        };
        if receipt.operation_id != operation_id
            || receipt.status != Status::Uncertain
            || receipt.retry_at > now
        {
            return Ok(None);
        }
        receipt.retry_at = now.saturating_add(30);
        timestamp(receipt.retry_at)?;
        receipt.updated_at = receipt.updated_at.max(now);
        write(&tx, &receipt)?;
        tx.commit()?;
        Ok(Some(receipt))
    }

    /// Explicit user authorization to stop tracking an inconclusive operation.
    /// Does not report success/failure and does not dispatch anything. Existing
    /// cooldowns still apply to a later, separately authorized write.
    pub fn retire(
        &mut self,
        scope: &FileScope,
        operation_id: &str,
        now: u64,
    ) -> Result<bool, AppError> {
        self.update(scope, operation_id, Status::Retired, now, 0)
    }

    fn update(
        &mut self,
        scope: &FileScope,
        operation_id: &str,
        status: Status,
        now: u64,
        retry_at: u64,
    ) -> Result<bool, AppError> {
        validate_scope(scope)?;
        timestamp(now)?;
        timestamp(retry_at)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut receipt) = read(&tx, scope)? else {
            return Ok(false);
        };
        if receipt.operation_id != operation_id || receipt.status != Status::Uncertain {
            return Ok(false);
        }
        receipt.status = status;
        receipt.updated_at = receipt.updated_at.max(now);
        receipt.retry_at = receipt.retry_at.max(retry_at);
        write(&tx, &receipt)?;
        prune(&tx, now)?;
        tx.commit()?;
        Ok(true)
    }
}

fn decode(body: &str) -> Result<Receipt, AppError> {
    let receipt: Receipt = serde_json::from_str(body)?;
    validate_scope(&receipt.scope)?;
    validate_operation(&receipt.operation)?;
    if uuid::Uuid::parse_str(&receipt.operation_id).is_err()
        || receipt.updated_at < receipt.created_at
    {
        return Err(invalid("invalid operation receipt identity or chronology"));
    }
    timestamp(receipt.created_at)?;
    timestamp(receipt.updated_at)?;
    timestamp(receipt.retry_at)?;
    Ok(receipt)
}

fn read(conn: &Connection, scope: &FileScope) -> Result<Option<Receipt>, AppError> {
    let body: Option<String> = conn
        .query_row(
            "SELECT body FROM receipts WHERE team=?1 AND user=?2 AND file=?3",
            params![scope.team_id, scope.user_id, scope.file_id],
            |row| row.get(0),
        )
        .optional()?;
    let receipt = body.map(|body| decode(&body)).transpose()?;
    if receipt
        .as_ref()
        .is_some_and(|receipt| receipt.scope != *scope)
    {
        return Err(invalid("operation receipt scope mismatch"));
    }
    Ok(receipt)
}

fn write(conn: &Connection, receipt: &Receipt) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO receipts(team,user,file,operation_id,terminal,updated_at,retry_at,body)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(team,user,file) DO UPDATE SET operation_id=excluded.operation_id,terminal=excluded.terminal,
         updated_at=excluded.updated_at,retry_at=excluded.retry_at,body=excluded.body",
        params![receipt.scope.team_id,receipt.scope.user_id,receipt.scope.file_id,receipt.operation_id,
            receipt.status != Status::Uncertain,timestamp(receipt.updated_at)?,timestamp(receipt.retry_at)?,serde_json::to_string(receipt)?],
    )?;
    Ok(())
}

fn prune(conn: &Connection, now: u64) -> Result<(), AppError> {
    // Unresolved writes and active cooldowns never expire through retention.
    conn.execute(
        "DELETE FROM receipts WHERE operation_id IN (
         SELECT operation_id FROM receipts WHERE terminal=1 AND retry_at<=?1
         ORDER BY updated_at DESC,operation_id DESC LIMIT -1 OFFSET ?2)",
        params![timestamp(now)?, MAX_TERMINAL_RECEIPTS],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
