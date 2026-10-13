//! Account-scoped forwarding receipts, committed before a single dispatch.
//!
//! Only `Begin::Started` authorizes dispatch. Stored requests start Uncertain:
//! a crash between commit and dispatch is indistinguishable from a lost reply.
//! Opening or listing the journal never retries a write. Explicit resends keep
//! the original Slack request but rotate a local attempt ID; that ID is not a
//! Slack idempotency promise. Use this synchronous API on a blocking worker.
use super::{Deliveries, Delivery, Status, valid_ts};
use crate::{
    error::AppError,
    slack::{
        Error,
        api::{ForwardMessage, ForwardedMessage},
        models::Message,
    },
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, path::Path, time::Duration};

const MAX_TERMINAL: i64 = 128;
const UNCONFIRMED: &str = "Delivery could not be confirmed. Check before resending.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub team_id: String,
    pub user_id: String,
    pub source: String,
    pub timestamp: String,
    pub destination: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub attempt_id: String,
    pub scope: Scope,
    pub delivery: Delivery,
    pub updated_at: u64,
    /// Explicitly stopped tracking; never interpreted as Sent or Failed.
    pub retired: bool,
    // A duplicate callback must not downgrade an already recorded ambiguity.
    response_recorded: bool,
}
impl Receipt {
    pub fn unresolved(&self) -> bool {
        !self.retired && matches!(self.delivery.status, Status::Uncertain(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Begin {
    Started(Receipt),
    Blocked(Receipt),
}

pub struct Journal {
    conn: Connection,
}

fn invalid(message: &'static str) -> AppError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}
fn account_id(account: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(account)
        .map(|id| id.to_string())
        .map_err(|_| invalid("invalid forwarding account"))
}
fn identity(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 512 || id.trim() != id || id.chars().any(char::is_control) {
        return Err(invalid("invalid forwarding scope"));
    }
    Ok(())
}
fn validate_scope(scope: &Scope) -> Result<(), AppError> {
    for id in [
        &scope.team_id,
        &scope.user_id,
        &scope.source,
        &scope.destination,
    ] {
        identity(id)?;
    }
    if !valid_ts(&scope.timestamp) {
        return Err(invalid("invalid forwarding source timestamp"));
    }
    Ok(())
}
fn validate_request(scope: &Scope, request: &ForwardMessage, name: &str) -> Result<(), AppError> {
    validate_scope(scope)?;
    if request.source != scope.source
        || request.timestamp != scope.timestamp
        || request.destination != scope.destination
        || uuid::Uuid::parse_str(&request.client_msg_id).is_err()
        || name.is_empty()
        || name.len() > 16_384
    {
        return Err(invalid("invalid forwarding request identity"));
    }
    Ok(())
}
fn timestamp(value: u64) -> Result<i64, AppError> {
    i64::try_from(value).map_err(|_| invalid("invalid forwarding timestamp"))
}

impl Journal {
    pub fn open_default(account: &str) -> Result<Self, AppError> {
        let path = crate::config::config_dir()?.join(format!(
            "forwarding-receipts-{}.sqlite",
            account_id(account)?
        ));
        Self::open(path, account)
    }

    /// Fixtures/tests supply their own path. The database records its owner;
    /// pointing another account at the same path fails closed.
    pub fn open(path: impl AsRef<Path>, account: &str) -> Result<Self, AppError> {
        let account = account_id(account)?;
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
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
            return Err(invalid("unsupported forwarding receipt schema"));
        }
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1), account TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS receipts(
               team TEXT NOT NULL, user TEXT NOT NULL, source TEXT NOT NULL, ts TEXT NOT NULL, destination TEXT NOT NULL,
               attempt_id TEXT NOT NULL UNIQUE, terminal INTEGER NOT NULL, updated_at INTEGER NOT NULL,
               retry_at INTEGER NOT NULL, body TEXT NOT NULL,
               PRIMARY KEY(team,user,source,ts,destination));
             PRAGMA user_version=1;"
        )?;
        tx.execute("INSERT OR IGNORE INTO identity VALUES(1,?1)", [&account])?;
        let owner: String = tx.query_row(
            "SELECT account FROM identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        if owner != account {
            return Err(invalid("forwarding receipt account mismatch"));
        }
        tx.commit()?;
        Ok(Self { conn })
    }

    pub fn get(&self, scope: &Scope) -> Result<Option<Receipt>, AppError> {
        validate_scope(scope)?;
        read(&self.conn, scope)
    }

    pub fn list_source(
        &self,
        team: &str,
        user: &str,
        source: &str,
        ts: &str,
    ) -> Result<Vec<Receipt>, AppError> {
        validate_scope(&Scope {
            team_id: team.into(),
            user_id: user.into(),
            source: source.into(),
            timestamp: ts.into(),
            destination: "_".into(),
        })?;
        let mut statement = self.conn.prepare(
            "SELECT body FROM receipts WHERE team=?1 AND user=?2 AND source=?3 AND ts=?4 ORDER BY updated_at,attempt_id"
        )?;
        let rows = statement.query_map(params![team, user, source, ts], |row| {
            row.get::<_, String>(0)
        })?;
        rows.map(|row| {
            let receipt = decode(&row?)?;
            if receipt.scope.team_id != team
                || receipt.scope.user_id != user
                || receipt.scope.source != source
                || receipt.scope.timestamp != ts
            {
                return Err(invalid("forwarding receipt scope mismatch"));
            }
            Ok(receipt)
        })
        .collect()
    }

    /// Includes receipts whose source message or destination metadata is absent.
    pub fn unresolved(&self, team: &str, user: &str) -> Result<Vec<Receipt>, AppError> {
        identity(team)?;
        identity(user)?;
        let mut statement = self.conn.prepare(
            "SELECT body FROM receipts WHERE team=?1 AND user=?2 AND terminal=0 ORDER BY updated_at,attempt_id"
        )?;
        let rows = statement.query_map(params![team, user], |row| row.get::<_, String>(0))?;
        rows.map(|row| {
            let receipt = decode(&row?)?;
            if receipt.scope.team_id != team
                || receipt.scope.user_id != user
                || !receipt.unresolved()
            {
                return Err(invalid("forwarding receipt scope or status mismatch"));
            }
            Ok(receipt)
        })
        .collect()
    }

    /// New explicit user intent. An unresolved previous request blocks even if
    /// the note or client_msg_id changes. Terminal receipts permit a new intent
    /// only after their retained Retry-After deadline.
    pub fn begin(
        &mut self,
        scope: Scope,
        name: String,
        request: ForwardMessage,
        now: u64,
    ) -> Result<Begin, AppError> {
        validate_request(&scope, &request, &name)?;
        timestamp(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(old) = read(&tx, &scope)?
            && (old.unresolved() || old.delivery.retry_at > now)
        {
            return Ok(Begin::Blocked(old));
        }
        let receipt = Receipt {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            scope,
            delivery: Delivery {
                name,
                request,
                status: Status::Uncertain(UNCONFIRMED.into()),
                started_at: now,
                retry_at: 0,
                previously_uncertain: false,
            },
            updated_at: now,
            retired: false,
            response_recorded: false,
        };
        write(&tx, &receipt)?;
        prune(&tx, now)?;
        tx.commit()?;
        Ok(Begin::Started(receipt))
    }

    /// ONLY after an explicit duplicate-delivery warning. Returns None for a
    /// stale/missing attempt, Blocked for a terminal receipt or active cooldown.
    /// Keeps source, destination, note, client ID and original history boundary.
    pub fn resend(
        &mut self,
        scope: &Scope,
        expected_attempt: &str,
        now: u64,
    ) -> Result<Option<Begin>, AppError> {
        validate_scope(scope)?;
        timestamp(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut receipt) = read(&tx, scope)? else {
            return Ok(None);
        };
        if receipt.attempt_id != expected_attempt {
            return Ok(None);
        }
        if !receipt.unresolved() || receipt.delivery.retry_at > now {
            return Ok(Some(Begin::Blocked(receipt)));
        }
        receipt.attempt_id = uuid::Uuid::new_v4().to_string();
        receipt.delivery.previously_uncertain = true;
        receipt.delivery.status = Status::Uncertain(UNCONFIRMED.into());
        receipt.updated_at = receipt.updated_at.max(now);
        receipt.response_recorded = false;
        write(&tx, &receipt)?;
        tx.commit()?;
        Ok(Some(Begin::Started(receipt)))
    }

    /// Persist the response using the immutable scope/attempt captured before
    /// dispatch, even if the initiating view or account is no longer active.
    /// Exactly repeated outcomes are idempotent; stale attempts never resolve
    /// a replacement, including a resend with the same Slack client_msg_id.
    pub fn finish(
        &mut self,
        scope: &Scope,
        attempt: &str,
        result: Result<ForwardedMessage, Error>,
        now: u64,
    ) -> Result<bool, AppError> {
        validate_scope(scope)?;
        timestamp(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut receipt) = read(&tx, scope)? else {
            return Ok(false);
        };
        if receipt.attempt_id != attempt || receipt.retired {
            return Ok(false);
        }
        let mut candidate = receipt.delivery.clone();
        candidate.status = Status::Sending;
        let request = candidate.request.clone();
        let mut ledger = Deliveries([(scope.destination.clone(), candidate)].into());
        ledger.finish(&request, result, now);
        let candidate = ledger.0.remove(&scope.destination).unwrap();
        timestamp(candidate.retry_at)?;
        if receipt.response_recorded || !receipt.unresolved() {
            return Ok(receipt.delivery.status == candidate.status);
        }
        receipt.delivery = candidate;
        receipt.response_recorded = true;
        receipt.updated_at = receipt.updated_at.max(now);
        write(&tx, &receipt)?;
        prune(&tx, now)?;
        tx.commit()?;
        Ok(true)
    }

    /// Only exact positive history evidence resolves uncertainty. The caller
    /// must have read this scope's destination. Absence is never rejection.
    pub fn confirm(
        &mut self,
        scope: &Scope,
        attempt: &str,
        request_id: &str,
        sender: &str,
        messages: &[Message],
        now: u64,
    ) -> Result<bool, AppError> {
        validate_scope(scope)?;
        timestamp(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut receipt) = read(&tx, scope)? else {
            return Ok(false);
        };
        if receipt.attempt_id != attempt
            || receipt.retired
            || receipt.delivery.request.client_msg_id != request_id
            || scope.user_id != sender
        {
            return Ok(false);
        }
        let Some(ts) = messages
            .iter()
            .find(|message| {
                message.client_msg_id.as_deref() == Some(request_id)
                    && message.user.as_deref() == Some(sender)
                    && message.ts.as_deref().is_some_and(valid_ts)
                    && message
                        .channel
                        .as_deref()
                        .is_none_or(|channel| channel == scope.destination)
            })
            .and_then(|message| message.ts.clone())
        else {
            return Ok(false);
        };
        if !receipt.unresolved() {
            return Ok(receipt.delivery.status == Status::Sent(ts));
        }
        receipt.delivery.status = Status::Sent(ts);
        receipt.updated_at = receipt.updated_at.max(now);
        write(&tx, &receipt)?;
        prune(&tx, now)?;
        tx.commit()?;
        Ok(true)
    }

    /// A read 429 is a cooldown, never a delivery outcome. Deadlines only grow.
    pub fn defer_check(
        &mut self,
        scope: &Scope,
        attempt: &str,
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
        if receipt.attempt_id != attempt || !receipt.unresolved() {
            return Ok(false);
        }
        receipt.delivery.retry_at = receipt.delivery.retry_at.max(retry_at);
        receipt.updated_at = receipt.updated_at.max(now);
        write(&tx, &receipt)?;
        tx.commit()?;
        Ok(true)
    }

    /// Explicitly stop tracking without claiming delivery or failure. Existing
    /// cooldown survives. The same already-terminal attempt is a harmless no-op.
    pub fn retire(&mut self, scope: &Scope, attempt: &str, now: u64) -> Result<bool, AppError> {
        validate_scope(scope)?;
        timestamp(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut receipt) = read(&tx, scope)? else {
            return Ok(false);
        };
        if receipt.attempt_id != attempt {
            return Ok(false);
        }
        if !receipt.unresolved() {
            return Ok(true);
        }
        receipt.retired = true;
        receipt.updated_at = receipt.updated_at.max(now);
        write(&tx, &receipt)?;
        prune(&tx, now)?;
        tx.commit()?;
        Ok(true)
    }
}

fn decode(body: &str) -> Result<Receipt, AppError> {
    let receipt: Receipt = serde_json::from_str(body)?;
    validate_request(
        &receipt.scope,
        &receipt.delivery.request,
        &receipt.delivery.name,
    )?;
    if uuid::Uuid::parse_str(&receipt.attempt_id).is_err()
        || receipt.updated_at < receipt.delivery.started_at
        || matches!(receipt.delivery.status, Status::Sending)
        || matches!(&receipt.delivery.status, Status::Sent(ts) if !valid_ts(ts))
    {
        return Err(invalid("invalid forwarding receipt"));
    }
    timestamp(receipt.delivery.started_at)?;
    timestamp(receipt.delivery.retry_at)?;
    timestamp(receipt.updated_at)?;
    Ok(receipt)
}
fn read(conn: &Connection, scope: &Scope) -> Result<Option<Receipt>, AppError> {
    let body: Option<String> = conn.query_row(
        "SELECT body FROM receipts WHERE team=?1 AND user=?2 AND source=?3 AND ts=?4 AND destination=?5",
        params![scope.team_id,scope.user_id,scope.source,scope.timestamp,scope.destination], |row| row.get(0)
    ).optional()?;
    let receipt = body.map(|body| decode(&body)).transpose()?;
    if receipt.as_ref().is_some_and(|r| r.scope != *scope) {
        return Err(invalid("forwarding receipt scope mismatch"));
    }
    Ok(receipt)
}
fn write(conn: &Connection, receipt: &Receipt) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO receipts(team,user,source,ts,destination,attempt_id,terminal,updated_at,retry_at,body)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(team,user,source,ts,destination) DO UPDATE SET attempt_id=excluded.attempt_id,
         terminal=excluded.terminal,updated_at=excluded.updated_at,retry_at=excluded.retry_at,body=excluded.body",
        params![receipt.scope.team_id,receipt.scope.user_id,receipt.scope.source,receipt.scope.timestamp,
            receipt.scope.destination,receipt.attempt_id,!receipt.unresolved(),timestamp(receipt.updated_at)?,
            timestamp(receipt.delivery.retry_at)?,serde_json::to_string(receipt)?]
    )?;
    Ok(())
}
fn prune(conn: &Connection, now: u64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM receipts WHERE attempt_id IN (SELECT attempt_id FROM receipts
         WHERE terminal=1 AND retry_at<=?1 ORDER BY updated_at DESC,attempt_id DESC LIMIT -1 OFFSET ?2)",
        params![timestamp(now)?,MAX_TERMINAL]
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
