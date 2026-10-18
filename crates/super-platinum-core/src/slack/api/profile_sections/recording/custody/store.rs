//! Off-renderer SQLite journal. Exact account/team/user identity and revision
//! fencing prevent an old task from replacing a newer audio or operation record.
use super::*;
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{fs::OpenOptions, path::Path, time::Duration};
pub const MAX_RECORDS: usize = 16;
pub struct Journal {
    conn: Connection,
    account: String,
}
#[derive(Clone)]
pub struct Stored {
    pub revision: u64,
    pub record: Record,
}
fn invalid_store() -> AppError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid recording custody").into()
}
fn account(value: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(value)
        .map(|v| v.to_string())
        .map_err(|_| invalid_store())
}
impl Journal {
    /// Uses the established configured storage root and existing account UUID.
    /// Fixture callers must use their explicitly owned path via open instead.
    pub fn open_default(owner: &str) -> Result<Self, AppError> {
        let owner = account(owner)?;
        Self::open(
            crate::config::config_dir()?.join(format!("profile-recording-{owner}.sqlite")),
            &owner,
        )
    }
    pub fn open(path: impl AsRef<Path>, owner: &str) -> Result<Self, AppError> {
        let owner = account(owner)?;
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(invalid_store());
            }
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
        conn.execute_batch("PRAGMA synchronous=FULL;PRAGMA journal_mode=DELETE;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if !matches!(version, 0 | 1 | 2) {
            return Err(invalid_store());
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),account TEXT NOT NULL);CREATE TABLE IF NOT EXISTS recordings(team TEXT NOT NULL,user TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),body TEXT NOT NULL,audio BLOB,PRIMARY KEY(team,user));")?;
        tx.execute("INSERT OR IGNORE INTO identity VALUES(1,?1)", [&owner])?;
        let stored: String =
            tx.query_row("SELECT account FROM identity WHERE singleton=1", [], |r| {
                r.get(0)
            })?;
        if stored != owner {
            return Err(invalid_store());
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS generations(team TEXT NOT NULL,user TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),PRIMARY KEY(team,user));INSERT INTO generations(team,user,revision) SELECT team,user,revision FROM recordings WHERE true ON CONFLICT(team,user) DO UPDATE SET revision=MAX(generations.revision,excluded.revision);CREATE TABLE IF NOT EXISTS cooldowns(team TEXT NOT NULL,user TEXT NOT NULL,retry_at INTEGER NOT NULL CHECK(retry_at>=0),PRIMARY KEY(team,user));PRAGMA user_version=2;")?;
        tx.commit()?;
        Ok(Self {
            conn,
            account: owner,
        })
    }
    fn check_scope(&self, scope: &Scope) -> Result<(), AppError> {
        if !scope.valid() || scope.account != self.account {
            Err(invalid_store())
        } else {
            Ok(())
        }
    }
    /// Reading is inert: pending stages are exposed as Unknown, never replayed.
    pub fn get(&self, scope: &Scope) -> Result<Option<Stored>, AppError> {
        self.check_scope(scope)?;
        let cooldown = self.cooldown(scope)?;
        read(&self.conn, scope).map(|v| {
            v.map(|mut s| {
                s.record.defer(cooldown);
                Stored {
                    revision: s.revision,
                    record: s.record.recovered(),
                }
            })
        })
    }
    pub fn cooldown(&self, scope: &Scope) -> Result<u64, AppError> {
        self.check_scope(scope)?;
        let value: Option<i64> = self
            .conn
            .query_row(
                "SELECT retry_at FROM cooldowns WHERE team=?1 AND user=?2",
                params![scope.team, scope.user],
                |r| r.get(0),
            )
            .optional()?;
        u64::try_from(value.unwrap_or(0)).map_err(|_| invalid_store())
    }
    pub fn defer(&mut self, scope: &Scope, until: u64) -> Result<(), AppError> {
        self.check_scope(scope)?;
        let until = i64::try_from(until).map_err(|_| invalid_store())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM cooldowns WHERE team=?1 AND user=?2)",
            params![scope.team, scope.user],
            |r| r.get(0),
        )?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM cooldowns", [], |r| r.get(0))?;
        if !exists && count >= MAX_RECORDS as i64 {
            return Err(invalid_store());
        }
        tx.execute("INSERT INTO cooldowns(team,user,retry_at) VALUES(?1,?2,?3) ON CONFLICT(team,user) DO UPDATE SET retry_at=MAX(cooldowns.retry_at,excluded.retry_at)",params![scope.team,scope.user,until])?;
        tx.commit()?;
        Ok(())
    }
    /// Full-sync compare-and-swap. Caller persists the next stage before dispatch.
    /// Unknown records cannot be replaced by an absent-row assumption.
    pub fn put(&mut self, expected: Option<u64>, record: &Record) -> Result<Stored, AppError> {
        self.check_scope(record.scope())?;
        record.validate().map_err(|_| invalid_store())?;
        let body = serde_json::to_string(&record.body).map_err(|_| invalid_store())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let retry: Option<i64> = tx
            .query_row(
                "SELECT retry_at FROM cooldowns WHERE team=?1 AND user=?2",
                params![record.scope().team, record.scope().user],
                |r| r.get(0),
            )
            .optional()?;
        if record.retry_at() < u64::try_from(retry.unwrap_or(0)).map_err(|_| invalid_store())? {
            return Err(invalid_store());
        }
        let old = read(&tx, record.scope())?;
        if old.as_ref().map(|r| r.revision) != expected {
            return Err(invalid_store());
        }
        if let Some(old) = &old {
            if record.retry_at() < old.record.retry_at() {
                return Err(invalid_store());
            }
        }
        let generation: Option<i64> = tx
            .query_row(
                "SELECT revision FROM generations WHERE team=?1 AND user=?2",
                params![record.scope().team, record.scope().user],
                |r| r.get(0),
            )
            .optional()?;
        if generation.is_none() {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM generations", [], |r| r.get(0))?;
            if count >= MAX_RECORDS as i64 {
                return Err(invalid_store());
            }
        }
        let generation = u64::try_from(generation.unwrap_or(0)).map_err(|_| invalid_store())?;
        if old.as_ref().is_some_and(|old| old.revision > generation) {
            return Err(invalid_store());
        }
        let revision = generation
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(invalid_store)?;
        tx.execute("INSERT INTO recordings(team,user,revision,body,audio) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(team,user) DO UPDATE SET revision=excluded.revision,body=excluded.body,audio=excluded.audio",params![record.scope().team,record.scope().user,revision as i64,body,record.audio().map(|a|a.bytes())])?;
        tx.execute("INSERT INTO generations(team,user,revision) VALUES(?1,?2,?3) ON CONFLICT(team,user) DO UPDATE SET revision=excluded.revision",params![record.scope().team,record.scope().user,revision as i64])?;
        tx.commit()?;
        Ok(Stored {
            revision,
            record: record.clone(),
        })
    }
    /// Explicit discard of a local-only draft. Remote operations and staged file
    /// ownership must first be resolved and persisted, never silently deleted.
    pub fn discard_local(
        &mut self,
        scope: &Scope,
        revision: u64,
        now: u64,
    ) -> Result<(), AppError> {
        self.check_scope(scope)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = read(&tx, scope)?.ok_or_else(invalid_store)?;
        if old.revision != revision
            || old.record.pending().is_some()
            || old.record.staged_file().is_some()
            || old.record.retry_at() > now
        {
            return Err(invalid_store());
        }
        let changed = tx.execute(
            "DELETE FROM recordings WHERE team=?1 AND user=?2 AND revision=?3",
            params![scope.team, scope.user, revision as i64],
        )?;
        if changed != 1 {
            return Err(invalid_store());
        }
        tx.commit()?;
        Ok(())
    }
}
fn read(conn: &Connection, scope: &Scope) -> Result<Option<Stored>, AppError> {
    // CASE bounds data before SQLite copies it into Rust, including corrupt rows.
    let raw:Option<(i64,Option<String>,Option<Vec<u8>>,i64)>=conn.query_row("SELECT revision,CASE WHEN length(CAST(body AS BLOB))<=?3 THEN body END,CASE WHEN length(audio)<=?4 THEN audio END,COALESCE(length(audio),0) FROM recordings WHERE team=?1 AND user=?2",params![scope.team,scope.user,MAX_HEADER_BYTES as i64,MAX_BYTES as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    raw.map(|(revision, body, bytes, size)| {
        if revision <= 0 || size < 0 || size > MAX_BYTES as i64 {
            return Err(invalid_store());
        }
        let body: Body =
            serde_json::from_str(&body.ok_or_else(invalid_store)?).map_err(|_| invalid_store())?;
        if body.scope != *scope {
            return Err(invalid_store());
        }
        let audio = match (&body.audio, bytes) {
            (None, None) => None,
            (Some(info), Some(bytes)) => {
                Some(Audio::new(bytes, &info.mime, info.elapsed_ms).map_err(|_| invalid_store())?)
            }
            _ => return Err(invalid_store()),
        };
        let record = Record { body, audio };
        record.validate().map_err(|_| invalid_store())?;
        Ok(Stored {
            revision: revision as u64,
            record,
        })
    })
    .transpose()
}
