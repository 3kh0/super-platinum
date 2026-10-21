//! Separate own-edit recovery in the account's existing comment database.
//! All restored values are inert. Commit `edit_begin` before transport, then
//! recheck live authority; a crash after that commit leaves a non-replayable lock.
use super::{
    AppError, Connection, Journal, OptionalExtension, Scope, TransactionBehavior, Value, encoded,
    invalid, params,
};
use crate::canvas::comments::content;
use crate::slack_lists::comments::{
    OwnEditAccepted, OwnEditRecord, OwnEditSeed, Page, Target, timestamp,
};
use serde::{Deserialize, Serialize};

mod store;
use store::{check_bounds, read, read_attempt, write, write_attempt};
const MAX_ROWS: i64 = 128;
const MAX_ATTEMPTS: i64 = 256;
const MAX_TOTAL_BYTES: i64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditPhase {
    Pending,
    Uncertain,
    NotSent,
    Rejected,
    RateLimited,
    Confirmed,
    Observed,
}
impl EditPhase {
    pub fn locked(self) -> bool {
        matches!(self, Self::Pending | Self::Uncertain)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditAttempt {
    id: String,
    record: OwnEditRecord,
    capture: String,
    phase: EditPhase,
    raw: Option<Value>,
    readback: Option<EditReadback>,
    retry_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditReadback {
    target: Target,
    raw: Value,
}
impl EditAttempt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn record(&self) -> &OwnEditRecord {
        &self.record
    }
    pub fn capture(&self) -> &str {
        &self.capture
    }
    pub fn phase(&self) -> EditPhase {
        self.phase
    }
    /// Original transport receipt, never overwritten with a readback message.
    pub fn raw(&self) -> Option<&Value> {
        self.raw.as_ref()
    }
    pub fn readback(&self) -> Option<&Value> {
        self.readback.as_ref().map(|v| &v.raw)
    }
    pub fn retry_at(&self) -> u64 {
        self.retry_at
    }
    fn validate(&self, scope: &Scope, ts: &str) -> Result<(), AppError> {
        uuid(&self.id)?;
        uuid(&self.capture)?;
        self.record.validate().map_err(|_| invalid())?;
        if self.record.ts() != ts
            || scope_for(self.record.seed())? != *scope
            || self.retry_at > i64::MAX as u64
        {
            return Err(invalid());
        }
        match self.phase {
            EditPhase::Pending | EditPhase::NotSent => {
                if self.raw.is_some() || self.readback.is_some() {
                    return Err(invalid());
                }
            }
            EditPhase::Uncertain => {
                if self.readback.is_some() {
                    return Err(invalid());
                }
            }
            EditPhase::Rejected => {
                if !self.raw.as_ref().is_some_and(rejected) || self.readback.is_some() {
                    return Err(invalid());
                }
            }
            EditPhase::RateLimited => {
                if self.raw.is_some() || self.readback.is_some() || self.retry_at == 0 {
                    return Err(invalid());
                }
            }
            EditPhase::Confirmed => {
                self.record
                    .validate_ack(self.raw.as_ref().ok_or_else(invalid)?)
                    .map_err(|_| invalid())?;
                if self.readback.is_some() {
                    return Err(invalid());
                }
            }
            EditPhase::Observed => {
                let readback = self.readback.as_ref().ok_or_else(invalid)?;
                let page =
                    Page::parse(&readback.target, readback.raw.clone()).map_err(|_| invalid())?;
                if self
                    .record
                    .observed(&page)
                    .map_err(|_| invalid())?
                    .is_none()
                {
                    return Err(invalid());
                }
            }
        }
        encoded(self)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditInventory {
    scope: Scope,
    ts: String,
    revision: u64,
    capture: String,
    seed: Option<OwnEditSeed>,
    blocks: Vec<Value>,
    attempt: Option<EditAttempt>,
    retry_at: u64,
}
impl EditInventory {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn ts(&self) -> &str {
        &self.ts
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn capture(&self) -> &str {
        &self.capture
    }
    pub fn seed(&self) -> Option<&OwnEditSeed> {
        self.seed.as_ref()
    }
    pub fn blocks(&self) -> &[Value] {
        &self.blocks
    }
    /// Includes terminal proof until explicit clear or a new committed attempt.
    pub fn attempt(&self) -> Option<&EditAttempt> {
        self.attempt.as_ref()
    }
    pub fn retry_at(&self) -> u64 {
        self.retry_at
    }
    fn validate(&self) -> Result<(), AppError> {
        key(&self.scope, &self.ts)?;
        if self.revision > i64::MAX as u64 || self.retry_at > i64::MAX as u64 {
            return Err(invalid());
        }
        if self.revision == 0 {
            if !self.capture.is_empty()
                || self.seed.is_some()
                || !self.blocks.is_empty()
                || self.attempt.is_some()
                || self.retry_at != 0
            {
                return Err(invalid());
            }
        } else {
            uuid(&self.capture)?;
        }
        if let Some(seed) = &self.seed {
            seed.validate().map_err(|_| invalid())?;
            if scope_for(seed)? != self.scope || seed.ts() != self.ts {
                return Err(invalid());
            }
            // Empty is a valid retained editing draft, never a send/delete.
            content::fallback(&self.blocks).map_err(|_| invalid())?;
        } else if !self.blocks.is_empty() || self.attempt.is_some() {
            return Err(invalid());
        }
        if let Some(attempt) = &self.attempt {
            attempt.validate(&self.scope, &self.ts)?;
            if Some(attempt.record.seed()) != self.seed.as_ref() || attempt.retry_at > self.retry_at
            {
                return Err(invalid());
            }
        }
        encoded(self)?;
        Ok(())
    }
}

/// Borrowed outcome proof allows retrying only failed local persistence while
/// retaining the actual original ACK. No variant recreates transport authority.
pub enum EditResolution<'a> {
    NotRequested,
    /// Host-observed issued HTTP 429 without a JSON receipt. Requires a positive
    /// absolute retry deadline; generic transport loss must use Uncertain.
    RateLimited,
    Rejected(&'a Value),
    Uncertain(Option<&'a Value>),
    Confirmed(&'a OwnEditAccepted),
    Observed(&'a Page),
}

pub(super) fn initialize(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS edit_drafts(owner TEXT PRIMARY KEY,body TEXT NOT NULL); CREATE TABLE IF NOT EXISTS edit_history(id TEXT PRIMARY KEY,owner TEXT NOT NULL,body TEXT NOT NULL);")?;
    Ok(())
}
fn uuid(value: &str) -> Result<(), AppError> {
    if uuid::Uuid::parse_str(value)
        .map_err(|_| invalid())?
        .to_string()
        != value
    {
        return Err(invalid());
    }
    Ok(())
}
fn scope_for(seed: &OwnEditSeed) -> Result<Scope, AppError> {
    Scope::capture(seed.workspace(), seed.user(), seed.target())
}
fn key(scope: &Scope, ts: &str) -> Result<String, AppError> {
    scope.validate()?;
    if !timestamp(ts) || ts == scope.root {
        return Err(invalid());
    }
    encoded(&(scope, ts))
}
fn bump(value: &mut EditInventory) -> Result<(), AppError> {
    value.revision = value
        .revision
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or_else(invalid)?;
    Ok(())
}
fn rejected(raw: &Value) -> bool {
    raw["ok"] == false
        && matches!(
            raw["error"].as_str(),
            Some(
                "cant_update_message"
                    | "edit_window_closed"
                    | "message_not_found"
                    | "is_inactive"
                    | "not_authed"
                    | "invalid_auth"
                    | "token_revoked"
                    | "account_inactive"
                    | "missing_scope"
                    | "no_permission"
                    | "ratelimited"
            )
        )
}

impl Journal {
    pub fn edit_inventory(&self, scope: &Scope, ts: &str) -> Result<EditInventory, AppError> {
        read(&self.conn, scope, ts)
    }
    pub fn edit_attempt(
        &self,
        scope: &Scope,
        ts: &str,
        id: &str,
    ) -> Result<Option<EditAttempt>, AppError> {
        read_attempt(&self.conn, scope, ts, id)
    }
    pub fn edit_save(
        &mut self,
        expected: &EditInventory,
        seed: &OwnEditSeed,
        capture: &str,
        blocks: Vec<Value>,
    ) -> Result<EditInventory, AppError> {
        expected.validate()?;
        seed.validate().map_err(|_| invalid())?;
        uuid(capture)?;
        content::fallback(&blocks).map_err(|_| invalid())?;
        if scope_for(seed)? != expected.scope || seed.ts() != expected.ts {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, &expected.scope, &expected.ts)?;
        if current != *expected
            || current.seed.as_ref().is_some_and(|s| s != seed)
            || (current.capture == capture && current.blocks != blocks)
        {
            return Err(invalid());
        }
        current.seed = Some(seed.clone());
        current.capture = capture.into();
        current.blocks = blocks;
        bump(&mut current)?;
        write(&tx, &current)?;
        check_bounds(&tx)?;
        tx.commit()?;
        Ok(current)
    }
    /// This durable pending lock must commit BEFORE any transport future runs.
    /// Restored pending/uncertain attempts can only be checked, never resent.
    pub fn edit_begin(
        &mut self,
        expected: &EditInventory,
        id: &str,
        record: &OwnEditRecord,
    ) -> Result<EditInventory, AppError> {
        expected.validate()?;
        uuid(id)?;
        record.validate().map_err(|_| invalid())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, &expected.scope, &expected.ts)?;
        let mut outgoing = current.blocks.clone();
        outgoing.extend_from_slice(record.seed().preserved());
        if current != *expected
            || current.seed.as_ref() != Some(record.seed())
            || current.attempt.as_ref().is_some_and(|a| a.phase.locked())
            || current.capture.is_empty()
            || outgoing != record.blocks()
        {
            return Err(invalid());
        }
        let attempt = EditAttempt {
            id: id.into(),
            record: record.clone(),
            capture: current.capture.clone(),
            phase: EditPhase::Pending,
            raw: None,
            readback: None,
            retry_at: current.retry_at,
        };
        write_attempt(&tx, &current.scope, &current.ts, &attempt, true)?;
        current.attempt = Some(attempt);
        bump(&mut current)?;
        write(&tx, &current)?;
        check_bounds(&tx)?;
        tx.commit()?;
        Ok(current)
    }
    /// Clear is an explicit user/host decision, not an effect of resolution.
    /// Caller must not clear a newer capture after resolving an older attempt.
    pub fn edit_clear(&mut self, expected: &EditInventory) -> Result<EditInventory, AppError> {
        expected.validate()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, &expected.scope, &expected.ts)?;
        if current != *expected || current.attempt.as_ref().is_some_and(|a| a.phase.locked()) {
            return Err(invalid());
        }
        current.seed = None;
        current.blocks.clear();
        current.attempt = None;
        current.capture = uuid::Uuid::new_v4().to_string();
        bump(&mut current)?;
        write(&tx, &current)?;
        check_bounds(&tx)?;
        tx.commit()?;
        Ok(current)
    }
    pub fn edit_resolve(
        &mut self,
        scope: &Scope,
        ts: &str,
        id: &str,
        resolution: EditResolution<'_>,
        retry_at: u64,
    ) -> Result<EditInventory, AppError> {
        uuid(id)?;
        if retry_at > i64::MAX as u64 {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, scope, ts)?;
        let active = current.attempt.as_ref().is_some_and(|a| a.id == id);
        let original = if active {
            current.attempt.clone().ok_or_else(invalid)?
        } else {
            // A terminal receipt may be retried after its explicit clear or a
            // newer attempt. It may verify history, never overwrite that draft.
            let old = read_attempt(&tx, scope, ts, id)?.ok_or_else(invalid)?;
            if old.phase.locked() {
                return Err(invalid());
            }
            old
        };
        let mut attempt = original.clone();
        match resolution {
            EditResolution::NotRequested => {
                attempt.phase = EditPhase::NotSent;
            }
            EditResolution::RateLimited => {
                if retry_at == 0 {
                    return Err(invalid());
                }
                attempt.phase = EditPhase::RateLimited;
            }
            EditResolution::Rejected(raw) => {
                if !rejected(raw) {
                    return Err(invalid());
                }
                attempt.phase = EditPhase::Rejected;
                assign_raw(&mut attempt, Some(raw))?;
            }
            EditResolution::Uncertain(raw) => {
                attempt.phase = EditPhase::Uncertain;
                assign_raw(&mut attempt, raw)?;
            }
            EditResolution::Confirmed(accepted) => {
                if accepted.record() != &attempt.record {
                    return Err(invalid());
                }
                attempt.phase = EditPhase::Confirmed;
                assign_raw(&mut attempt, Some(accepted.raw()))?;
            }
            EditResolution::Observed(page) => {
                if attempt
                    .record
                    .observed(page)
                    .map_err(|_| invalid())?
                    .is_none()
                {
                    return Err(invalid());
                }
                attempt.phase = EditPhase::Observed;
                attempt.readback = Some(EditReadback {
                    target: page.target().clone(),
                    raw: page.raw().clone(),
                });
            }
        }
        // Unknown cannot be relabeled known-unsent/rejected. Terminal results
        // permit exact idempotent local retries, never conflicting rewrites.
        if original.phase == EditPhase::Uncertain
            && matches!(
                attempt.phase,
                EditPhase::NotSent | EditPhase::Rejected | EditPhase::RateLimited
            )
            || (!original.phase.locked()
                && (attempt.phase != original.phase
                    || attempt.raw != original.raw
                    || attempt.readback != original.readback))
        {
            return Err(invalid());
        }
        attempt.retry_at = attempt.retry_at.max(retry_at);
        attempt.validate(scope, ts)?;
        if !active {
            if attempt != original {
                return Err(invalid());
            }
            tx.commit()?;
            return Ok(current);
        }
        current.retry_at = current.retry_at.max(attempt.retry_at);
        if current.attempt.as_ref() == Some(&attempt) {
            tx.commit()?;
            return Ok(current);
        }
        write_attempt(&tx, scope, ts, &attempt, false)?;
        current.attempt = Some(attempt);
        bump(&mut current)?;
        write(&tx, &current)?;
        check_bounds(&tx)?;
        tx.commit()?;
        Ok(current)
    }
}
fn assign_raw(attempt: &mut EditAttempt, raw: Option<&Value>) -> Result<(), AppError> {
    if let Some(raw) = raw {
        if attempt.raw.as_ref().is_some_and(|old| old != raw) {
            return Err(invalid());
        }
        attempt.raw = Some(raw.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
