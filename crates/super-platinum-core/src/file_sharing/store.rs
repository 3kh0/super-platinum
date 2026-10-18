//! Synchronous SQLite custody. Run on a blocking worker, never a render path.
use super::*;
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::{fs::OpenOptions, path::Path, time::Duration};

pub mod direct_open;
pub mod drafts;

pub const MAX_UNRESOLVED: usize = 32;
pub const MAX_TERMINAL: usize = 128;
const MAX_BODY: usize = FILE_SHARE_NOTE_BYTES + FILE_SHARE_RESPONSE_BYTES + 65_536;

pub enum Begin {
    Started(Receipt),
    Blocked(Receipt),
    CoolingDown { retry_at: u64 },
    Capacity,
}

pub struct Journal {
    conn: Connection,
}

fn invalid() -> AppError {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Invalid file Share receipt",
    )
    .into()
}
fn number(n: u64) -> Result<i64, AppError> {
    i64::try_from(n).map_err(|_| invalid())
}
fn identity(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && id.trim() == id && !id.chars().any(char::is_control)
}
fn account(value: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(value)
        .map(|v| v.to_string())
        .map_err(|_| invalid())
}
fn validate(r: &Receipt) -> Result<(), AppError> {
    if uuid::Uuid::parse_str(&r.attempt_id).is_err()
        || !identity(&r.scope.team)
        || !identity(&r.scope.user)
        || r.scope.file != r.intent.file
        || r.scope.destination != r.intent.destination
        || r.intent.request().is_err()
        || r.file_title.is_empty()
        || r.file_title.len() > 16_384
        || r.destination_name.is_empty()
        || r.destination_name.len() > 16_384
        || r.updated_at < r.created_at
    {
        return Err(invalid());
    }
    number(r.created_at)?;
    number(r.updated_at)?;
    if let Some(Outcome::Response(raw)) = &r.outcome {
        if decode_existing_file_share(raw.clone()).raw().is_none() {
            return Err(invalid());
        }
    }
    Ok(())
}
fn encode(r: &Receipt) -> Result<String, AppError> {
    validate(r)?;
    let value = serde_json::to_string(r).map_err(|_| invalid())?;
    if value.len() > MAX_BODY {
        return Err(invalid());
    }
    Ok(value)
}
fn decode(body: String, scope: &Scope, attempt: &str, terminal: bool) -> Result<Receipt, AppError> {
    if body.len() > MAX_BODY {
        return Err(invalid());
    }
    let r: Receipt = serde_json::from_str(&body).map_err(|_| invalid())?;
    validate(&r)?;
    if &r.scope != scope || r.attempt_id != attempt || terminal == r.unresolved() {
        return Err(invalid());
    }
    Ok(r)
}
fn get(conn: &Connection, scope: &Scope) -> Result<Option<Receipt>, AppError> {
    let row: Option<(String, String, bool, i64)> = conn.query_row(
        "SELECT body,attempt,terminal,length(CAST(body AS BLOB)) FROM receipts WHERE team=?1 AND user=?2 AND file=?3 AND destination=?4",
        params![scope.team,scope.user,scope.file,scope.destination],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
    ).optional()?;
    row.map(|(body, attempt, terminal, size)| {
        if size < 0 || size as usize > MAX_BODY {
            return Err(invalid());
        }
        decode(body, scope, &attempt, terminal)
    })
    .transpose()
}
fn cooldown(conn: &Connection, team: &str, user: &str) -> Result<u64, AppError> {
    let value: Option<i64> = conn
        .query_row(
            "SELECT retry_at FROM cooldowns WHERE team=?1 AND user=?2",
            params![team, user],
            |r| r.get(0),
        )
        .optional()?;
    u64::try_from(value.unwrap_or(0)).map_err(|_| invalid())
}
fn save(tx: &Transaction<'_>, r: &Receipt) -> Result<(), AppError> {
    let body = encode(r)?;
    tx.execute("INSERT INTO receipts(team,user,file,destination,attempt,terminal,updated_at,body) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(team,user,file,destination) DO UPDATE SET attempt=excluded.attempt,terminal=excluded.terminal,updated_at=excluded.updated_at,body=excluded.body",
        params![r.scope.team,r.scope.user,r.scope.file,r.scope.destination,r.attempt_id,!r.unresolved(),number(r.updated_at)?,body])?;
    // Indexed terminal flags are not sufficient authority to destroy a note.
    // Validate every selected row before deleting any; an inconsistent row
    // aborts this whole transaction, including the unrelated settlement above.
    let mut stmt = tx.prepare("SELECT team,user,file,destination,attempt,terminal,body,updated_at FROM receipts WHERE terminal=1 ORDER BY updated_at DESC,attempt DESC LIMIT -1 OFFSET ?1")?;
    let rows = stmt.query_map([MAX_TERMINAL as i64], |row| {
        Ok((
            Scope {
                team: row.get(0)?,
                user: row.get(1)?,
                file: row.get(2)?,
                destination: row.get(3)?,
            },
            row.get::<_, String>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
        ))
    })?;
    let mut prune = Vec::new();
    for row in rows {
        let (scope, attempt, terminal, body, updated_at) = row?;
        let receipt = decode(body, &scope, &attempt, terminal)?;
        if number(receipt.updated_at)? != updated_at {
            return Err(invalid());
        }
        prune.push(receipt);
    }
    drop(stmt);
    for receipt in prune {
        drafts::retire_association(tx, &receipt)?;
        direct_open::retire_completed(tx, &receipt)?;
        let deleted = tx.execute("DELETE FROM receipts WHERE team=?1 AND user=?2 AND file=?3 AND destination=?4 AND attempt=?5 AND terminal=1",
            params![receipt.scope.team,receipt.scope.user,receipt.scope.file,receipt.scope.destination,receipt.attempt_id])?;
        if deleted != 1 {
            return Err(invalid());
        }
    }
    Ok(())
}

impl Journal {
    pub fn open_default(owner: &str) -> Result<Self, AppError> {
        let owner = account(owner)?;
        Self::open(
            crate::config::config_dir()?.join(format!("file-share-receipts-{owner}.sqlite")),
            &owner,
        )
    }
    /// Fixtures/tests must inject an owned path; database identity is independent
    /// of the path. Uses established private-file/full-sync journal conventions.
    pub fn open(path: impl AsRef<Path>, owner: &str) -> Result<Self, AppError> {
        let owner = account(owner)?;
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
        conn.execute_batch("PRAGMA synchronous=FULL;PRAGMA journal_mode=DELETE;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if !matches!(version, 0 | 1 | 2 | 3 | 4) {
            return Err(invalid());
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),account TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS receipts(team TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,destination TEXT NOT NULL,attempt TEXT NOT NULL UNIQUE,terminal INTEGER NOT NULL CHECK(terminal IN(0,1)),updated_at INTEGER NOT NULL,body TEXT NOT NULL,PRIMARY KEY(team,user,file,destination));
            CREATE TABLE IF NOT EXISTS cooldowns(team TEXT NOT NULL,user TEXT NOT NULL,retry_at INTEGER NOT NULL CHECK(retry_at>=0),PRIMARY KEY(team,user));")?;
        tx.execute("INSERT OR IGNORE INTO identity VALUES(1,?1)", [&owner])?;
        let stored: String =
            tx.query_row("SELECT account FROM identity WHERE singleton=1", [], |r| {
                r.get(0)
            })?;
        if stored != owner {
            return Err(invalid());
        }
        drafts::migrate(&tx, version)?;
        direct_open::migrate(&tx, version)?;
        tx.execute_batch("PRAGMA user_version=4;")?;
        tx.commit()?;
        Ok(Self { conn })
    }
    pub fn get(&self, scope: &Scope) -> Result<Option<Receipt>, AppError> {
        get(&self.conn, scope)
    }
    pub fn retry_at(&self, team: &str, user: &str) -> Result<u64, AppError> {
        cooldown(&self.conn, team, user)
    }
    /// Atomic record/cooldown gate. Reopen/list never invokes this method.
    pub fn begin(
        &mut self,
        scope: Scope,
        intent: Intent,
        file_title: String,
        destination_name: String,
        now: u64,
    ) -> Result<Begin, AppError> {
        self.begin_checked(scope, intent, file_title, destination_name, now, None)
    }
    fn begin_checked(
        &mut self,
        scope: Scope,
        intent: Intent,
        file_title: String,
        destination_name: String,
        now: u64,
        draft: Option<&drafts::Draft>,
    ) -> Result<Begin, AppError> {
        let receipt = Receipt {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            scope,
            intent,
            file_title,
            destination_name,
            created_at: now,
            updated_at: now,
            outcome: None,
        };
        encode(&receipt)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(draft) = draft {
            drafts::check_current(&tx, draft)?;
            if let Some(receipt) = drafts::blocking(&tx, Some(draft))? {
                return Ok(Begin::Blocked(receipt));
            }
        }
        let previous = get(&tx, &receipt.scope)?;
        if let Some(old) = &previous {
            if old.unresolved() {
                return Ok(Begin::Blocked(old.clone()));
            }
        }
        let retry_at = cooldown(&tx, &receipt.scope.team, &receipt.scope.user)?;
        if retry_at > now {
            return Ok(Begin::CoolingDown { retry_at });
        }
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM receipts WHERE terminal=0", [], |r| {
                r.get(0)
            })?;
        if count >= MAX_UNRESOLVED as i64 {
            return Ok(Begin::Capacity);
        }
        if let Some(previous) = &previous {
            drafts::retire_association(&tx, previous)?;
            direct_open::retire_completed(&tx, previous)?;
        }
        save(&tx, &receipt)?;
        if let Some(draft) = draft {
            drafts::associate(&tx, draft, &receipt)?;
        }
        tx.commit()?;
        Ok(Begin::Started(receipt))
    }
    /// Only before dispatch, after a failed final admission check. Caller must
    /// retain uncertainty if this exact attempt cannot be settled durably.
    pub fn cancel_before_dispatch(
        &mut self,
        scope: &Scope,
        attempt: &str,
        now: u64,
    ) -> Result<Option<Receipt>, AppError> {
        self.finish(
            scope,
            attempt,
            Outcome::NotSent(NotSent::AdmissionCanceled),
            now,
        )
    }
    /// Always call with captured owner/attempt after the one transport call,
    /// before checking whether the dialog, account or source row is still shown.
    /// This consumes the Result. The caller must retain a clone before calling
    /// so a storage error cannot discard its only in-memory response/outcome.
    pub fn finish_result(
        &mut self,
        scope: &Scope,
        attempt: &str,
        result: Result<Value, Error>,
        now: u64,
    ) -> Result<Option<Receipt>, AppError> {
        self.finish(scope, attempt, classify(result), now)
    }
    /// Persist a retained result only for this exact originally admitted note.
    /// No admission, transport or retry of a Slack write occurs here.
    pub fn retry_result(
        &mut self,
        expected: &Receipt,
        result: Result<Value, Error>,
        received_at: u64,
    ) -> Result<Option<Receipt>, AppError> {
        self.finish_checked(
            &expected.scope,
            &expected.attempt_id,
            classify(result),
            received_at,
            Some(expected),
        )
    }
    /// A bounded retained classification needs no fabricated oversized payload.
    pub fn retry_response_limit(
        &mut self,
        expected: &Receipt,
        received_at: u64,
    ) -> Result<Option<Receipt>, AppError> {
        self.finish_checked(
            &expected.scope,
            &expected.attempt_id,
            Outcome::ResponseLimit,
            received_at,
            Some(expected),
        )
    }
    fn finish(
        &mut self,
        scope: &Scope,
        attempt: &str,
        outcome: Outcome,
        now: u64,
    ) -> Result<Option<Receipt>, AppError> {
        self.finish_checked(scope, attempt, outcome, now, None)
    }
    fn finish_checked(
        &mut self,
        scope: &Scope,
        attempt: &str,
        outcome: Outcome,
        now: u64,
        expected: Option<&Receipt>,
    ) -> Result<Option<Receipt>, AppError> {
        number(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut r) = get(&tx, scope)? else {
            return Ok(None);
        };
        if r.attempt_id != attempt {
            return Ok(None);
        }
        if expected.is_some_and(|e| {
            e.scope != r.scope
                || e.attempt_id != r.attempt_id
                || e.intent != r.intent
                || e.file_title != r.file_title
                || e.destination_name != r.destination_name
                || e.created_at != r.created_at
        }) {
            return Err(invalid());
        }
        if r.outcome.is_some() {
            return Ok(Some(r));
        }
        if let Outcome::RateLimited { retry_after_secs } = outcome {
            let retry_at = now
                .saturating_add(retry_after_secs.unwrap_or(30))
                .min(i64::MAX as u64);
            tx.execute("INSERT INTO cooldowns(team,user,retry_at) VALUES(?1,?2,?3) ON CONFLICT(team,user) DO UPDATE SET retry_at=MAX(retry_at,excluded.retry_at)",params![scope.team,scope.user,number(retry_at)?])?;
        }
        r.updated_at = now.max(r.created_at);
        r.outcome = Some(outcome);
        let linked = drafts::take_association(&tx, &r)?;
        save(&tx, &r)?;
        drafts::settled(&tx, &r, linked.as_ref())?;
        tx.commit()?;
        Ok(Some(r))
    }
    /// Unresolved notes remain discoverable without their original source row.
    /// Bounded by MAX_UNRESOLVED; never resolves or sends anything.
    pub fn unresolved(&self, team: &str, user: &str) -> Result<Vec<Receipt>, AppError> {
        let mut stmt=self.conn.prepare("SELECT file,destination,body,attempt,terminal FROM receipts WHERE team=?1 AND user=?2 AND terminal=0 ORDER BY updated_at,attempt LIMIT ?3")?;
        let rows = stmt.query_map(params![team, user, (MAX_UNRESOLVED + 1) as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (file, destination, body, attempt, terminal) = row?;
            out.push(decode(
                body,
                &Scope {
                    team: team.into(),
                    user: user.into(),
                    file,
                    destination,
                },
                &attempt,
                terminal,
            )?);
        }
        if out.len() > MAX_UNRESOLVED {
            return Err(invalid());
        }
        Ok(out)
    }
}
