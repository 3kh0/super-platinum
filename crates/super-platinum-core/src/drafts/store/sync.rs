//! Durable ordinary-draft groundwork. Nothing here enables Slack autosave.
//!
//! Enrollment requires an explicitly supplied, lossless `DraftSync` snapshot;
//! local markdown is never converted here. Empty visible drafts may still own
//! sync rows. A prepared operation permits one dispatch after the commit reply;
//! a lost reply must be recovered by loading, never by replaying preparation.
use super::{DraftKey, DraftStore, Entries, LocalDraft, write_local};
use crate::{
    drafts::sync::{DraftSync, Operation, Scope, Workflow},
    error::AppError,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::BTreeMap;
mod alternates;
mod resolution;
pub use alternates::{LocalAlternate, LocalWriter};
use alternates::{has_alternates, preserve_owned_alternate, read_alternates, reviewed_revision};
pub use resolution::{LocalChoice, LocalReview, LocalSettlement};

// Version 4 adds persisted event ordering and unsupported observation evidence.
// Read earlier rows; older binaries reject v4 rather than discard these guards.
const FORMAT: i64 = 4;

#[derive(Clone, Debug)]
pub struct Stored {
    pub revision: u64,
    pub state: DraftSync,
}
#[derive(Clone, Debug)]
pub struct BlockedLocal {
    /// Explicit settlement is conditional on this exact version of the edit.
    pub token: String,
    pub draft: LocalDraft,
}
#[derive(Debug)]
pub struct Loaded {
    pub local: Entries,
    pub synced: BTreeMap<DraftKey, Stored>,
    /// Legacy edits attempted after enrollment. Preserve separately; never
    /// silently use these as authoritative blocks or overwrite the synced pair.
    pub blocked_local: BTreeMap<DraftKey, BlockedLocal>,
    /// Worker writes that have not committed (for example disk-full failures).
    /// These are not durable and cannot authorize a network operation.
    pub uncommitted_local: BTreeMap<DraftKey, LocalDraft>,
    /// Latest refused input per independent editor; never overwrites another
    /// editor's version and survives account changes/process restart.
    pub local_alternates: BTreeMap<DraftKey, Vec<LocalAlternate>>,
}
#[derive(Clone, Copy, Debug)]
pub enum LoadMode {
    Current,
    /// Only at process recovery, before any writer starts dispatching.
    AfterRestart,
}
#[derive(Debug)]
pub enum Commit {
    Applied(Stored),
    Stale(Option<Stored>),
}
#[derive(Debug)]
pub enum Release {
    /// The paired local value is unchanged and now accepts ordinary writes.
    Released(LocalDraft),
    Stale(Option<Stored>),
    Blocked(Stored),
}

#[derive(Debug)]
pub enum Prepare {
    Prepared {
        stored: Stored,
        operation: Operation,
    },
    Blocked(Stored),
    Stale(Option<Stored>),
}

pub(super) fn invalid(message: &str) -> AppError {
    std::io::Error::other(message).into()
}
pub(super) fn owner(conn: &Connection, account: &str) -> Result<(), AppError> {
    if uuid::Uuid::parse_str(account).is_err() {
        return Err(invalid("invalid draft account"));
    }
    conn.execute(
        "INSERT OR IGNORE INTO composer_draft_owner VALUES(1,?1)",
        [account],
    )?;
    let existing: String = conn.query_row(
        "SELECT account FROM composer_draft_owner WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    if existing != account {
        return Err(invalid("draft store account mismatch"));
    }
    Ok(())
}
fn validate(scope: &Scope, state: &DraftSync) -> Result<(), AppError> {
    if state.scope() != scope
        || !state.consumption_valid()
        || !state.observation_state_valid()
        || scope.key.team.is_empty()
        || scope.key.channel.is_empty()
        || scope.key.thread.as_ref().is_some_and(String::is_empty)
        || uuid::Uuid::parse_str(state.client_msg_id()).is_err()
        || state
            .acknowledged()
            .is_some_and(|remote| remote.scope != *scope)
        || state.pending().is_some_and(|operation| {
            operation.scope != *scope
                || operation
                    .base
                    .as_ref()
                    .is_some_and(|remote| remote.scope != *scope)
        })
        || matches!(state.workflow(), Workflow::Conflict { remote: Some(remote), .. } if remote.scope != *scope)
    {
        return Err(invalid("draft sync scope mismatch"));
    }
    Ok(())
}
fn decode(scope: &Scope, revision: i64, format: i64, content: String) -> Result<Stored, AppError> {
    if !matches!(format, 1 | 2 | 3 | FORMAT) || revision <= 0 {
        return Err(invalid("unsupported draft sync record"));
    }
    let state = serde_json::from_str(&content)?;
    validate(scope, &state)?;
    Ok(Stored {
        revision: revision as u64,
        state,
    })
}
fn read(conn: &Connection, scope: &Scope) -> Result<Option<Stored>, AppError> {
    let raw = conn.query_row("SELECT storage_revision,format_version,state FROM composer_draft_sync WHERE destination=?1",
        [serde_json::to_string(&scope.key)?], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    raw.map(|(revision, format, content)| decode(scope, revision, format, content))
        .transpose()
}
fn next_revision(previous: Option<&Stored>) -> Result<u64, AppError> {
    previous
        .map_or(0, |row| row.revision)
        .checked_add(1)
        .filter(|revision| *revision <= i64::MAX as u64)
        .ok_or_else(|| invalid("draft storage revision exhausted"))
}
fn write(conn: &Connection, stored: &Stored) -> Result<(), AppError> {
    conn.execute("INSERT INTO composer_draft_sync(destination,storage_revision,format_version,state) VALUES(?1,?2,?3,?4) ON CONFLICT(destination) DO UPDATE SET storage_revision=excluded.storage_revision,format_version=excluded.format_version,state=excluded.state",
        params![serde_json::to_string(&stored.state.scope().key)?,stored.revision as i64,FORMAT,serde_json::to_string(&stored.state)?])?;
    Ok(())
}

fn preserve_conflict(conn: &Connection, scope: &Scope, local: &LocalDraft) -> Result<(), AppError> {
    // Never replace another retained version. A subsequent input remains in
    // its editor snapshot until explicitly reviewed if this slot is occupied.
    conn.execute(
        "INSERT OR IGNORE INTO composer_draft_blocked(destination,token,content) VALUES(?1,?2,?3)",
        params![
            serde_json::to_string(&scope.key)?,
            uuid::Uuid::new_v4().to_string(),
            serde_json::to_string(local)?
        ],
    )?;
    Ok(())
}

impl DraftStore {
    pub(super) fn preserve_local_conflict(
        &mut self,
        scope: &Scope,
        local: &LocalDraft,
    ) -> Result<(), AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        preserve_conflict(&tx, scope, local)?;
        tx.commit()?;
        Ok(())
    }

    /// Conditional local handoff after release. A newer account-visit edit,
    /// reenrollment, or preserved alternate must never be overwritten.
    pub fn save_local_if_unchanged(
        &mut self,
        scope: &Scope,
        expected: &LocalDraft,
        local: &LocalDraft,
    ) -> Result<bool, AppError> {
        self.save_local_conditionally(scope, expected, local, None)
    }

    pub fn save_owned_local_if_unchanged(
        &mut self,
        scope: &Scope,
        expected: &LocalDraft,
        local: &LocalDraft,
        writer: &LocalWriter,
        revision: u64,
    ) -> Result<bool, AppError> {
        self.save_local_conditionally(scope, expected, local, Some((writer, revision)))
    }

    fn save_local_conditionally(
        &mut self,
        scope: &Scope,
        expected: &LocalDraft,
        local: &LocalDraft,
        writer: Option<(&LocalWriter, u64)>,
    ) -> Result<bool, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        if let Some((writer, revision)) = writer {
            if let Some(floor) = reviewed_revision(&tx, scope, writer)? {
                // A reviewed editor cannot regain authority from a delayed
                // conditional request, even if its old base happens to match.
                if revision > floor {
                    preserve_conflict(&tx, scope, local)?;
                    preserve_owned_alternate(&tx, scope, writer, revision, local)?;
                }
                tx.commit()?;
                return Ok(false);
            }
        }
        if read(&tx, scope)?.is_some() {
            preserve_conflict(&tx, scope, local)?;
            if let Some((writer, revision)) = writer {
                preserve_owned_alternate(&tx, scope, writer, revision, local)?;
            }
            tx.commit()?;
            return Ok(false);
        }
        let blocked: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM composer_draft_blocked WHERE destination=?1)",
            [serde_json::to_string(&scope.key)?],
            |row| row.get(0),
        )?;
        if blocked || has_alternates(&tx, scope)? {
            if let Some((writer, revision)) = writer {
                preserve_owned_alternate(&tx, scope, writer, revision, local)?;
            }
            tx.commit()?;
            return Ok(false);
        }
        let raw: Option<String> = tx
            .query_row(
                "SELECT content FROM composer_drafts WHERE destination=?1",
                [serde_json::to_string(&scope.key)?],
                |row| row.get(0),
            )
            .optional()?;
        let current: LocalDraft = raw
            .map(|raw| serde_json::from_str(&raw))
            .transpose()?
            .unwrap_or_default();
        if serde_json::to_value(&current)? != serde_json::to_value(expected)? {
            preserve_conflict(&tx, scope, local)?;
            if let Some((writer, revision)) = writer {
                preserve_owned_alternate(&tx, scope, writer, revision, local)?;
            }
            tx.commit()?;
            return Ok(false);
        }
        write_local(&tx, &scope.key, local)?;
        tx.commit()?;
        Ok(true)
    }

    /// Release only an exactly reviewed, locally owned idle row. Retain the
    /// composer row byte-for-byte; release never recreates or clears its text.
    pub fn release_synced(&mut self, scope: &Scope, expected: u64) -> Result<Release, AppError> {
        let expected_sql = i64::try_from(expected)
            .map_err(|_| invalid("draft storage revision exceeds SQLite range"))?;
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        let previous = read(&tx, scope)?;
        if previous.as_ref().map(|row| row.revision) != Some(expected) {
            return Ok(Release::Stale(previous));
        }
        let stored = previous.unwrap();
        let blocked: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM composer_draft_blocked WHERE destination=?1)",
            [serde_json::to_string(&scope.key)?],
            |row| row.get(0),
        )?;
        if blocked
            || has_alternates(&tx, scope)?
            || stored.state.blocked_observation().is_some()
            || stored.state.acknowledged().is_some()
            || stored.state.pending().is_some()
            || stored.state.consumption().is_some()
            || !matches!(stored.state.workflow(), Workflow::Idle)
        {
            return Ok(Release::Blocked(stored));
        }
        let raw: Option<String> = tx
            .query_row(
                "SELECT content FROM composer_drafts WHERE destination=?1",
                [serde_json::to_string(&scope.key)?],
                |row| row.get(0),
            )
            .optional()?;
        let local = raw
            .map(|raw| serde_json::from_str(&raw))
            .transpose()?
            .unwrap_or_default();
        tx.execute(
            "DELETE FROM composer_draft_sync WHERE destination=?1 AND storage_revision=?2",
            params![serde_json::to_string(&scope.key)?, expected_sql],
        )?;
        tx.commit()?;
        Ok(Release::Released(local))
    }

    /// Validate ownership and preserve an alternate in one write transaction.
    /// A concurrent release must never turn this into an authoritative local edit.
    pub fn save_local_alternate(
        &mut self,
        scope: &Scope,
        local: LocalDraft,
    ) -> Result<(), AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        if read(&tx, scope)?.is_none() {
            return Err(invalid("alternate input requires a managed draft"));
        }
        tx.execute("INSERT INTO composer_draft_blocked(destination,token,content) VALUES(?1,?2,?3) ON CONFLICT(destination) DO UPDATE SET token=excluded.token,content=excluded.content",
            params![serde_json::to_string(&scope.key)?,uuid::Uuid::new_v4().to_string(),serde_json::to_string(&local)?])?;
        tx.commit()?;
        Ok(())
    }

    /// Both tables commit together. `None` enrolls only an absent sync row whose
    /// durable local content already matches `local` (save local edits first).
    /// `settle_blocked` explicitly retires that exact rejected legacy edit,
    /// whether the caller incorporated it or deliberately chose another version.
    pub fn commit_synced(
        &mut self,
        scope: &Scope,
        expected: Option<u64>,
        local: &LocalDraft,
        state: &DraftSync,
        settle_blocked: Option<&str>,
    ) -> Result<Commit, AppError> {
        validate(scope, state)?;
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        let previous = read(&tx, scope)?;
        if previous.as_ref().map(|row| row.revision) != expected {
            return Ok(Commit::Stale(previous));
        }
        let new_consumption = state.consumption().is_some()
            && previous.as_ref().and_then(|row| row.state.consumption()) != state.consumption();
        let restoring_consumption = previous.as_ref().is_some_and(|row| {
            matches!(row.state.workflow(), Workflow::Rejected(_))
                && row.state.consumption().is_some_and(|intent| {
                    state.consumption().is_none() && state.desired() == Some(&intent.snapshot)
                })
        });
        if restoring_consumption {
            let blocked: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM composer_draft_blocked WHERE destination=?1)",
                [serde_json::to_string(&scope.key)?],
                |row| row.get(0),
            )?;
            if blocked || has_alternates(&tx, scope)? {
                return Ok(Commit::Stale(previous));
            }
        }
        if (previous.is_none() || settle_blocked.is_some() || new_consumption)
            && has_alternates(&tx, scope)?
        {
            return Err(invalid(
                "local alternate versions require explicit settlement",
            ));
        }
        if previous.is_none() {
            let raw: Option<String> = tx
                .query_row(
                    "SELECT content FROM composer_drafts WHERE destination=?1",
                    [serde_json::to_string(&scope.key)?],
                    |row| row.get(0),
                )
                .optional()?;
            let existing: LocalDraft = raw
                .map(|raw| serde_json::from_str(&raw))
                .transpose()?
                .unwrap_or_default();
            if serde_json::to_value(&existing)? != serde_json::to_value(local)? {
                return Ok(Commit::Stale(None));
            }
        }
        // Creating a dispatch identity belongs exclusively to prepare_synced.
        // Edits/acknowledgments may preserve an existing in-flight operation.
        if let Workflow::InFlight(operation) = state.workflow()
            && !previous
                .as_ref()
                .and_then(|row| row.state.pending())
                .is_some_and(|old| old == operation)
        {
            return Err(invalid("new draft operations require durable preparation"));
        }
        let stored = Stored {
            revision: next_revision(previous.as_ref())?,
            state: state.clone(),
        };
        write_local(&tx, &scope.key, local)?;
        write(&tx, &stored)?;
        if let Some(token) = settle_blocked {
            tx.execute(
                "DELETE FROM composer_draft_blocked WHERE destination=?1 AND token=?2",
                params![serde_json::to_string(&scope.key)?, token],
            )?;
        }
        tx.commit()?;
        Ok(Commit::Applied(stored))
    }

    /// Atomically claims one persisted revision. It never replays an existing
    /// InFlight/Uncertain operation, even when a previous reply was lost.
    pub fn prepare_synced(&mut self, scope: &Scope, expected: u64) -> Result<Prepare, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        let previous = read(&tx, scope)?;
        if previous.as_ref().map(|row| row.revision) != Some(expected) {
            return Ok(Prepare::Stale(previous));
        }
        let mut stored = previous.unwrap();
        let blocked: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM composer_draft_blocked WHERE destination=?1)",
            [serde_json::to_string(&scope.key)?],
            |row| row.get(0),
        )?;
        if blocked || has_alternates(&tx, scope)? {
            return Ok(Prepare::Blocked(stored));
        }
        let operation = stored.state.begin_operation(scope);
        if operation.is_none() {
            // begin_operation can acknowledge an unchanged local revision.
            let old = read(&tx, scope)?.unwrap();
            if stored.state != old.state {
                stored.revision = next_revision(Some(&old))?;
                write(&tx, &stored)?;
            }
            tx.commit()?;
            return Ok(Prepare::Blocked(stored));
        }
        stored.revision = next_revision(Some(&stored))?;
        write(&tx, &stored)?;
        tx.commit()?;
        Ok(Prepare::Prepared {
            stored,
            operation: operation.unwrap(),
        })
    }

    /// The local and sync snapshots are read in one transaction. Recovery also
    /// durably normalizes InFlight to Uncertain before returning any state.
    pub fn load_synced(&mut self, account: &str, mode: LoadMode) -> Result<Loaded, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, account)?;
        let raw = {
            let mut query = tx.prepare(
                "SELECT destination,storage_revision,format_version,state FROM composer_draft_sync",
            )?;
            query
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut synced = BTreeMap::new();
        for (key, revision, format, content) in raw {
            let key = serde_json::from_str(&key)?;
            let scope = Scope {
                account_id: account.into(),
                key,
            };
            let mut stored = decode(&scope, revision, format, content)?;
            if matches!(mode, LoadMode::AfterRestart) {
                let recovered = stored.state.clone().restore_after_restart();
                if recovered != stored.state {
                    stored.revision = next_revision(Some(&stored))?;
                    stored.state = recovered;
                    write(&tx, &stored)?;
                }
            }
            synced.insert(scope.key, stored);
        }
        let local = {
            let mut query = tx.prepare("SELECT destination,content FROM composer_drafts")?;
            query
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .map(|row| {
                    let (key, content) = row?;
                    Ok((serde_json::from_str(&key)?, serde_json::from_str(&content)?))
                })
                .collect::<Result<Entries, AppError>>()?
        };
        let blocked_local = {
            let mut query =
                tx.prepare("SELECT destination,token,content FROM composer_draft_blocked")?;
            query
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .map(|row| {
                    let (key, token, content) = row?;
                    Ok((
                        serde_json::from_str(&key)?,
                        BlockedLocal {
                            token,
                            draft: serde_json::from_str(&content)?,
                        },
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, AppError>>()?
        };
        let local_alternates = read_alternates(&tx)?;
        tx.commit()?;
        Ok(Loaded {
            local,
            synced,
            blocked_local,
            uncommitted_local: BTreeMap::new(),
            local_alternates,
        })
    }
}

#[cfg(test)]
mod consume_tests;
#[cfg(test)]
pub(super) mod tests;

#[cfg(test)]
mod release_tests;
