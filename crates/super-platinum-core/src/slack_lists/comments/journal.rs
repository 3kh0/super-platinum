//! Blocking account-local recovery. Restored attempts are inert locks, never sends.
use super::{Page, ReplyRecord, Target};
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs::OpenOptions, path::Path, time::Duration};
const MAX_BODY: usize = 1024 * 1024;
const MAX_SCOPES: i64 = 128;
const MAX_HISTORY: i64 = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub workspace: String,
    pub user: String,
    pub list: String,
    pub record: String,
    pub created: Option<Value>,
    pub creator: Option<Value>,
    pub root: String,
}
impl Scope {
    pub fn capture(workspace: &str, user: &str, target: &Target) -> Result<Self, AppError> {
        target.validate().map_err(|_| invalid())?;
        let value = Self {
            workspace: workspace.into(),
            user: user.into(),
            list: target.list_id().into(),
            record: target.record_id().into(),
            created: target.record_raw().get("date_created").cloned(),
            creator: target.record_raw().get("created_by").cloned(),
            root: target.root_ts().ok_or_else(invalid)?.into(),
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), AppError> {
        if [&self.workspace, &self.user, &self.list, &self.record]
            .iter()
            .any(|v| !super::identity(v))
            || super::channel(&self.list).is_err()
            || !super::timestamp(&self.root)
            || serde_json::to_vec(self)?.len() > 8192
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn matches(&self, record: &ReplyRecord) -> bool {
        Self::capture(record.workspace(), record.user(), record.target())
            .as_ref()
            .is_ok_and(|s| s == self)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    record: ReplyRecord,
    capture: String,
    #[serde(default)]
    raw: Option<Value>,
}
impl Attempt {
    pub fn record(&self) -> &ReplyRecord {
        &self.record
    }
    pub fn capture(&self) -> &str {
        &self.capture
    }
    pub fn raw(&self) -> Option<&Value> {
        self.raw.as_ref()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    scope: Scope,
    revision: u64,
    capture: String,
    blocks: Vec<Value>,
    attempt: Option<Attempt>,
}
impl Inventory {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn capture(&self) -> &str {
        &self.capture
    }
    pub fn blocks(&self) -> &[Value] {
        &self.blocks
    }
    pub fn attempt(&self) -> Option<&Attempt> {
        self.attempt.as_ref()
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    fn validate(&self) -> Result<(), AppError> {
        self.scope.validate()?;
        if self.revision > i64::MAX as u64
            || (self.revision > 0 && uuid::Uuid::parse_str(&self.capture).is_err())
            || encoded(&self.blocks)?.len() > MAX_BODY / 2
        {
            return Err(invalid());
        }
        if let Some(attempt) = &self.attempt {
            attempt.record.validate().map_err(|_| invalid())?;
            if !self.scope.matches(&attempt.record)
                || uuid::Uuid::parse_str(&attempt.capture).is_err()
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
pub enum Resolution {
    NotRequested,
    Rejected(Value),
    Uncertain(Option<Value>),
    Accepted(Value),
}
pub struct Journal {
    conn: Connection,
}
fn invalid() -> AppError {
    std::io::Error::other("List comment recovery storage is unavailable or changed").into()
}
fn encoded<T: Serialize>(v: &T) -> Result<String, AppError> {
    let s = serde_json::to_string(v)?;
    if s.len() > MAX_BODY {
        return Err(invalid());
    }
    Ok(s)
}
fn key(scope: &Scope) -> Result<String, AppError> {
    scope.validate()?;
    encoded(scope)
}
fn read(conn: &Connection, scope: &Scope) -> Result<Inventory, AppError> {
    let raw:Option<String>=conn.query_row("SELECT CASE WHEN length(CAST(body AS BLOB))<=?2 THEN body ELSE NULL END FROM drafts WHERE owner=?1",params![key(scope)?,MAX_BODY as i64],|r|r.get(0)).optional()?;
    let value = match raw {
        Some(raw) => serde_json::from_str::<Inventory>(&raw)?,
        None => Inventory {
            scope: scope.clone(),
            revision: 0,
            capture: String::new(),
            blocks: Vec::new(),
            attempt: None,
        },
    };
    value.validate()?;
    if value.scope != *scope {
        return Err(invalid());
    }
    Ok(value)
}
fn write(conn: &Connection, value: &Inventory) -> Result<(), AppError> {
    value.validate()?;
    conn.execute("INSERT INTO drafts(owner,body) VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET body=excluded.body",params![key(&value.scope)?,encoded(value)?])?;
    Ok(())
}
impl Journal {
    pub fn open_default(account: &str) -> Result<Self, AppError> {
        let account = uuid::Uuid::parse_str(account).map_err(|_| invalid())?;
        Self::open(
            crate::config::config_dir()?.join(format!("list-comment-replies-{account}.sqlite")),
            &account.to_string(),
        )
    }
    pub fn open(path: impl AsRef<Path>, account: &str) -> Result<Self, AppError> {
        let account = uuid::Uuid::parse_str(account)
            .map_err(|_| invalid())?
            .to_string();
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
        drop(options.open(path)?);
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA journal_mode=DELETE;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if !matches!(version, 0 | 1) {
            return Err(invalid());
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS metadata(account TEXT NOT NULL); CREATE TABLE IF NOT EXISTS drafts(owner TEXT PRIMARY KEY,body TEXT NOT NULL); CREATE TABLE IF NOT EXISTS history(id TEXT PRIMARY KEY,owner TEXT NOT NULL,body TEXT NOT NULL); PRAGMA user_version=1;")?;
        let accounts: Vec<String> = {
            let mut s = tx.prepare("SELECT account FROM metadata")?;
            let rows = s.query_map([], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        match accounts.as_slice() {
            [] => {
                tx.execute("INSERT INTO metadata(account) VALUES(?1)", [account])?;
            }
            [found] if found == &account => {}
            _ => return Err(invalid()),
        }
        tx.commit()?;
        Ok(Self { conn })
    }
    pub fn inventory(&self, scope: &Scope) -> Result<Inventory, AppError> {
        read(&self.conn, scope)
    }
    pub fn save(
        &mut self,
        expected: &Inventory,
        capture: &str,
        blocks: Vec<Value>,
    ) -> Result<Inventory, AppError> {
        uuid::Uuid::parse_str(capture).map_err(|_| invalid())?;
        encoded(&blocks)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, &expected.scope)?;
        if current.revision != expected.revision {
            return Err(invalid());
        }
        if current.revision == 0
            && tx.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, i64>(0))?
                >= MAX_SCOPES
        {
            return Err(invalid());
        }
        current.revision = current.revision.checked_add(1).ok_or_else(invalid)?;
        current.capture = capture.into();
        current.blocks = blocks;
        write(&tx, &current)?;
        tx.commit()?;
        Ok(current)
    }
    /// Commit this lock before invoking any transport future. No API rebuilds an intent.
    pub fn begin(
        &mut self,
        expected: &Inventory,
        record: ReplyRecord,
    ) -> Result<Inventory, AppError> {
        record.validate().map_err(|_| invalid())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, &expected.scope)?;
        if current.revision != expected.revision
            || current.attempt.is_some()
            || !current.scope.matches(&record)
            || current.blocks != record.blocks()
            || current.capture.is_empty()
        {
            return Err(invalid());
        }
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM history WHERE owner=?1",
            [key(&current.scope)?],
            |r| r.get(0),
        )?;
        if count >= MAX_HISTORY {
            return Err(invalid());
        }
        let attempt = Attempt {
            record,
            capture: current.capture.clone(),
            raw: None,
        };
        tx.execute(
            "INSERT INTO history(id,owner,body) VALUES(?1,?2,?3)",
            params![
                attempt.record.client_msg_id(),
                key(&current.scope)?,
                encoded(&attempt)?
            ],
        )?;
        current.attempt = Some(attempt);
        current.revision += 1;
        write(&tx, &current)?;
        tx.commit()?;
        Ok(current)
    }
    pub fn resolve(
        &mut self,
        scope: &Scope,
        record: &ReplyRecord,
        resolution: Resolution,
    ) -> Result<Inventory, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, scope)?;
        let Some(mut attempt) = current.attempt.clone().filter(|a| &a.record == record) else {
            return Err(invalid());
        };
        let (keep, accepted, raw) = match resolution {
            Resolution::NotRequested => (false, false, None),
            Resolution::Rejected(raw) => {
                if raw.get("ok") != Some(&Value::Bool(false))
                    || raw
                        .get("error")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                {
                    return Err(invalid());
                }
                (false, false, Some(raw))
            }
            Resolution::Uncertain(raw) => (true, false, raw),
            Resolution::Accepted(raw) => {
                let page = Page::parse(
                    record.target(),
                    serde_json::json!({"ok":true,"messages":[raw.clone()]}),
                )
                .map_err(|_| invalid())?;
                if record.observed(&page).map_err(|_| invalid())?.is_none() {
                    return Err(invalid());
                }
                (false, true, Some(raw))
            }
        };
        attempt.raw = raw;
        if accepted && current.capture == attempt.capture && current.blocks == record.blocks() {
            current.blocks.clear();
            current.capture = uuid::Uuid::new_v4().to_string();
        }
        tx.execute(
            "UPDATE history SET body=?2 WHERE id=?1",
            params![record.client_msg_id(), encoded(&attempt)?],
        )?;
        current.attempt = keep.then_some(attempt);
        current.revision += 1;
        write(&tx, &current)?;
        tx.commit()?;
        Ok(current)
    }
}
#[cfg(test)]
mod tests;
