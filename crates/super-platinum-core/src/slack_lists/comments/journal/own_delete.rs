//! Inert own-delete attempts in the account's existing comment database.
//! Pending commits before transport; reopening never recreates a sendable intent.
//! Reply drafts and own-edit rows are never touched by these transactions.
use super::{
    AppError, Connection, Journal, OptionalExtension, Scope, TransactionBehavior, Value, encoded,
    invalid, params,
};
use crate::slack_lists::comments::{OwnDeleteAccepted, OwnDeleteRecord, Page, Target, timestamp};
use serde::{Deserialize, Serialize};
mod store;
use store::{check_bounds, read, read_attempt, write, write_attempt};
const MAX_ROWS: i64 = 128;
const MAX_ATTEMPTS: i64 = 256;
const MAX_TOTAL_BYTES: i64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletePhase {
    Pending,
    Uncertain,
    NotSent,
    Rejected,
    RateLimited,
    Confirmed,
    Observed,
}
impl DeletePhase {
    pub fn locked(self) -> bool {
        matches!(self, Self::Pending | Self::Uncertain)
    }
    fn finished(self) -> bool {
        matches!(self, Self::Confirmed | Self::Observed)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteAttempt {
    id: String,
    record: OwnDeleteRecord,
    phase: DeletePhase,
    raw: Option<Value>,
    readback: Option<DeleteReadback>,
    retry_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteReadback {
    target: Target,
    raw: Value,
}
impl DeleteAttempt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn record(&self) -> &OwnDeleteRecord {
        &self.record
    }
    pub fn phase(&self) -> DeletePhase {
        self.phase
    }
    /// Original transport receipt, separate from a positive later read.
    pub fn raw(&self) -> Option<&Value> {
        self.raw.as_ref()
    }
    pub fn readback(&self) -> Option<&Value> {
        self.readback.as_ref().map(|r| &r.raw)
    }
    pub fn retry_at(&self) -> u64 {
        self.retry_at
    }
    fn validate(&self, scope: &Scope, ts: &str) -> Result<(), AppError> {
        uuid(&self.id)?;
        self.record.validate().map_err(|_| invalid())?;
        if self.record.ts() != ts
            || scope_for(&self.record)? != *scope
            || self.retry_at > i64::MAX as u64
        {
            return Err(invalid());
        }
        match self.phase {
            DeletePhase::Pending | DeletePhase::NotSent => {
                if self.raw.is_some() || self.readback.is_some() {
                    return Err(invalid());
                }
            }
            DeletePhase::Uncertain => {
                if self.readback.is_some() {
                    return Err(invalid());
                }
            }
            DeletePhase::Rejected => {
                if !self.raw.as_ref().is_some_and(OwnDeleteRecord::rejected)
                    || self.readback.is_some()
                {
                    return Err(invalid());
                }
            }
            DeletePhase::RateLimited => {
                if self.raw.is_some() || self.readback.is_some() || self.retry_at == 0 {
                    return Err(invalid());
                }
            }
            DeletePhase::Confirmed => {
                self.record
                    .validate_ack(self.raw.as_ref().ok_or_else(invalid)?)
                    .map_err(|_| invalid())?;
                if self.readback.is_some() {
                    return Err(invalid());
                }
            }
            DeletePhase::Observed => {
                let readback = self.readback.as_ref().ok_or_else(invalid)?;
                let page =
                    Page::parse(&readback.target, readback.raw.clone()).map_err(|_| invalid())?;
                if !self.record.observed(&page).map_err(|_| invalid())? {
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
pub struct DeleteInventory {
    scope: Scope,
    ts: String,
    revision: u64,
    attempt: Option<DeleteAttempt>,
    retry_at: u64,
}
impl DeleteInventory {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn ts(&self) -> &str {
        &self.ts
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn attempt(&self) -> Option<&DeleteAttempt> {
        self.attempt.as_ref()
    }
    pub fn retry_at(&self) -> u64 {
        self.retry_at
    }
    fn validate(&self) -> Result<(), AppError> {
        key(&self.scope, &self.ts)?;
        if self.revision > i64::MAX as u64
            || self.retry_at > i64::MAX as u64
            || (self.revision == 0 && (self.attempt.is_some() || self.retry_at != 0))
            || (self.revision > 0 && self.attempt.is_none())
        {
            return Err(invalid());
        }
        if let Some(attempt) = &self.attempt {
            attempt.validate(&self.scope, &self.ts)?;
            if attempt.retry_at != self.retry_at {
                return Err(invalid());
            }
        }
        encoded(self)?;
        Ok(())
    }
}
pub enum DeleteResolution<'a> {
    NotRequested,
    /// Issued HTTP 429, requiring a positive absolute host-observed deadline.
    RateLimited,
    Rejected(&'a Value),
    Uncertain(Option<&'a Value>),
    Confirmed(&'a OwnDeleteAccepted),
    Observed(&'a Page),
}
pub(super) fn initialize(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS delete_state(owner TEXT PRIMARY KEY,body TEXT NOT NULL); CREATE TABLE IF NOT EXISTS delete_history(id TEXT PRIMARY KEY,owner TEXT NOT NULL,body TEXT NOT NULL);")?;
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
fn scope_for(record: &OwnDeleteRecord) -> Result<Scope, AppError> {
    Scope::capture(record.workspace(), record.user(), record.target())
}
fn key(scope: &Scope, ts: &str) -> Result<String, AppError> {
    scope.validate()?;
    if !timestamp(ts) || ts == scope.root {
        return Err(invalid());
    }
    encoded(&(scope, ts))
}
fn bump(value: &mut DeleteInventory) -> Result<(), AppError> {
    value.revision = value
        .revision
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or_else(invalid)?;
    Ok(())
}
impl Journal {
    pub fn delete_inventory(&self, scope: &Scope, ts: &str) -> Result<DeleteInventory, AppError> {
        read(&self.conn, scope, ts)
    }
    pub fn delete_attempt(
        &self,
        scope: &Scope,
        ts: &str,
        id: &str,
    ) -> Result<Option<DeleteAttempt>, AppError> {
        read_attempt(&self.conn, scope, ts, id)
    }
    /// Commit exactly once before issuance. Pending/uncertain and positively
    /// deleted timestamps never allow another begin, including same-ID retries.
    /// Caller still owns permission/clock/owner guards after this blocking work.
    pub fn delete_begin(
        &mut self,
        expected: &DeleteInventory,
        id: &str,
        record: &OwnDeleteRecord,
    ) -> Result<DeleteInventory, AppError> {
        expected.validate()?;
        uuid(id)?;
        record.validate().map_err(|_| invalid())?;
        if scope_for(record)? != expected.scope || record.ts() != expected.ts {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, &expected.scope, &expected.ts)?;
        if current != *expected
            || current
                .attempt
                .as_ref()
                .is_some_and(|a| a.phase.locked() || a.phase.finished())
        {
            return Err(invalid());
        }
        let attempt = DeleteAttempt {
            id: id.into(),
            record: record.clone(),
            phase: DeletePhase::Pending,
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
    /// Resolve original inert evidence only. Exact retries are idempotent and
    /// cannot overwrite a successor attempt or turn unknown into known-unsent.
    pub fn delete_resolve(
        &mut self,
        scope: &Scope,
        ts: &str,
        id: &str,
        resolution: DeleteResolution<'_>,
        retry_at: u64,
    ) -> Result<DeleteInventory, AppError> {
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
            let old = read_attempt(&tx, scope, ts, id)?.ok_or_else(invalid)?;
            if old.phase.locked() {
                return Err(invalid());
            }
            old
        };
        let mut attempt = original.clone();
        match resolution {
            DeleteResolution::NotRequested => attempt.phase = DeletePhase::NotSent,
            DeleteResolution::RateLimited => {
                if retry_at == 0 {
                    return Err(invalid());
                }
                attempt.phase = DeletePhase::RateLimited;
            }
            DeleteResolution::Rejected(raw) => {
                if !OwnDeleteRecord::rejected(raw) {
                    return Err(invalid());
                }
                attempt.phase = DeletePhase::Rejected;
                assign_raw(&mut attempt, Some(raw))?;
            }
            DeleteResolution::Uncertain(raw) => {
                attempt.phase = DeletePhase::Uncertain;
                assign_raw(&mut attempt, raw)?;
            }
            DeleteResolution::Confirmed(accepted) => {
                if accepted.record() != &attempt.record {
                    return Err(invalid());
                }
                attempt.phase = DeletePhase::Confirmed;
                assign_raw(&mut attempt, Some(accepted.raw()))?;
            }
            DeleteResolution::Observed(page) => {
                if !attempt.record.observed(page).map_err(|_| invalid())? {
                    return Err(invalid());
                }
                attempt.phase = DeletePhase::Observed;
                attempt.readback = Some(DeleteReadback {
                    target: page.target().clone(),
                    raw: page.raw().clone(),
                });
            }
        }
        if (original.phase == DeletePhase::Uncertain
            && matches!(
                attempt.phase,
                DeletePhase::NotSent | DeletePhase::Rejected | DeletePhase::RateLimited
            ))
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
fn assign_raw(attempt: &mut DeleteAttempt, raw: Option<&Value>) -> Result<(), AppError> {
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
