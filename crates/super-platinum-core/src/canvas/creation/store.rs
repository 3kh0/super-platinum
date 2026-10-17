use super::{Fault, Result, model::*, retirement_model::*};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fs::OpenOptions, path::Path, time::Duration};

mod conflicts;
mod decisions;
mod delivery;
mod evidence;
mod integrity;
mod local_review;
pub use local_review::LocalVersionChoice;
mod legacy_integrity;
mod migration;
mod retirement;
pub use retirement::RetirementReview;
#[cfg(test)]
mod tests;

const SCHEMA: &str = "
CREATE TABLE identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),account TEXT NOT NULL);
CREATE TABLE scopes(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL);
CREATE TABLE drafts(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL);
CREATE TABLE attempts(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL);
CREATE TABLE conflicts(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL);
CREATE TABLE decisions(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL);
CREATE TABLE dispositions(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL);
PRAGMA user_version=2;";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    owner: Owner,
    active: Option<String>,
    retry_at: u64,
}
trait Record: Serialize + DeserializeOwned + PartialEq {
    const TABLE: &'static str;
    fn key(&self) -> String;
    fn owner(&self) -> &Owner;
    fn validate(&self) -> Result<()>;
}
macro_rules! record {
    ($ty:ty, $table:literal, $id:ident) => {
        impl Record for $ty {
            const TABLE: &'static str = $table;
            fn key(&self) -> String {
                self.$id.clone()
            }
            fn owner(&self) -> &Owner {
                &self.owner
            }
            fn validate(&self) -> Result<()> {
                <$ty>::validate(self)
            }
        }
    };
}
record!(Draft, "drafts", id);
record!(Attempt, "attempts", id);
record!(Conflict, "conflicts", id);
record!(ArchiveReceipt, "decisions", operation);
record!(Disposition, "dispositions", id);
record!(RetiredAttempt, "retired_attempts", id);
record!(LateResultEvidence, "late_evidence", id);
record!(EvidenceDisposition, "evidence_dispositions", id);

impl Record for Scope {
    const TABLE: &'static str = "scopes";
    fn key(&self) -> String {
        self.owner.key()
    }
    fn owner(&self) -> &Owner {
        &self.owner
    }
    fn validate(&self) -> Result<()> {
        self.owner.validate()?;
        clock(self.retry_at)?;
        if let Some(id) = &self.active {
            uuid(id)?;
        }
        Ok(())
    }
}

pub struct Store {
    conn: Connection,
    account: String,
}
impl Store {
    /// Explicit path only. Call from a retained blocking worker, never render.
    /// Tests/fixtures must supply a private temporary path, not application storage.
    pub fn open(path: impl AsRef<Path>, account: &str) -> Result<Self> {
        uuid(account)?;
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(Fault::Invalid);
        }
        if let Some(parent) = path.parent() {
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
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            let tables: i64 = tx.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )?;
            if tables != 0 {
                return Err(Fault::Invalid);
            }
            tx.execute_batch(SCHEMA)?;
            tx.execute("INSERT INTO identity VALUES(1,?1)", [account])?;
        } else if version != 2 && version != 3 {
            return Err(Fault::Invalid);
        }
        let identity: (i64, String) =
            tx.query_row("SELECT count(*),account FROM identity", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if identity != (1, account.to_owned()) {
            return Err(Fault::Invalid);
        }
        // A v2 migration validates the entire bounded predecessor before adding tables.
        if version == 0 || version == 2 {
            legacy_integrity::validate(&tx, account)?;
            migration::upgrade(&tx)?;
        }
        integrity::validate(&tx, account)?;
        tx.commit()?;
        Ok(Self {
            conn,
            account: account.into(),
        })
    }
    fn owner(&self, owner: &Owner) -> Result<()> {
        owner.validate()?;
        if owner.account != self.account {
            Err(Fault::Invalid)
        } else {
            Ok(())
        }
    }
    /// One coherent snapshot covers association validation AND result projection.
    /// The private connection cannot be externally nested; unchecked_transaction
    /// permits an immutable public reader and rejects accidental nesting at runtime.
    fn read_snapshot<T>(&self, project: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let tx = self.conn.unchecked_transaction()?;
        integrity::validate(&tx, &self.account)?;
        let result = project(&tx)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn active(&self, owner: &Owner) -> Result<Option<Draft>> {
        self.owner(owner)?;
        self.read_snapshot(|conn| active(conn, owner))
    }
    pub fn draft(&self, owner: &Owner, id: &str) -> Result<Option<Draft>> {
        self.owner(owner)?;
        uuid(id)?;
        self.read_snapshot(|conn| {
            let draft = read::<Draft>(conn, owner, id)?;
            if let Some(draft) = &draft {
                association(conn, draft)?;
            }
            Ok(draft)
        })
    }
    pub fn attempt(&self, owner: &Owner, id: &str) -> Result<Option<Attempt>> {
        self.owner(owner)?;
        uuid(id)?;
        self.read_snapshot(|conn| read_attempt(conn, owner, id))
    }
    /// Exact absence admission. Does not replace an unseen saved draft.
    pub fn create(&mut self, owner: &Owner, writer: &str, content: Content) -> Result<Draft> {
        self.owner(owner)?;
        content.validate()?;
        uuid(writer)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        if active(&tx, owner)?.is_some() {
            return Err(Fault::Stale);
        }
        let mut scope = scope(&tx, owner)?;
        if active_scope_count(&tx)? >= MAX_SCOPES {
            return Err(Fault::Capacity);
        }
        if count(&tx, "drafts")? >= MAX_SCOPES + MAX_ATTEMPTS {
            return Err(Fault::Capacity);
        }
        let draft = Draft::fresh(owner.clone(), writer, content)?;
        let current = scope.get_or_insert(Scope {
            owner: owner.clone(),
            active: None,
            retry_at: 0,
        });
        current.active = Some(draft.id.clone());
        insert(&tx, &draft)?;
        put(&tx, current)?;
        tx.commit()?;
        Ok(draft)
    }
    pub fn save(&mut self, expected: &Draft, writer: &str, content: Content) -> Result<Draft> {
        self.owner(&expected.owner)?;
        expected.validate()?;
        uuid(writer)?;
        content.validate()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        exact_active(&tx, expected)?;
        editable(&tx, expected)?;
        let mut after = expected.clone();
        after.revision = next(after.revision)?;
        after.writer = writer.into();
        after.content = content;
        after.attempt = None;
        put(&tx, &after)?;
        tx.commit()?;
        Ok(after)
    }
    /// Explicit exact discard only for editable, unarchived drafts. Attempts remain.
    pub fn discard(&mut self, expected: &Draft) -> Result<()> {
        self.owner(&expected.owner)?;
        expected.validate()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        exact_active(&tx, expected)?;
        editable(&tx, expected)?;
        // Conflicts require their own explicit review; never erase their base.
        let conflicts: i64 = tx.query_row(
            "SELECT count(*) FROM (SELECT body FROM conflicts WHERE json_extract(body,'$.draft_id')=?1 UNION ALL SELECT body FROM late_evidence WHERE json_extract(body,'$.original.draft_id')=?1)",
            [&expected.id],
            |r| r.get(0),
        )?;
        if conflicts != 0 {
            return Err(Fault::Blocked);
        }
        let historical: i64 = tx.query_row(
            "SELECT count(*) FROM attempts WHERE json_extract(body,'$.draft_id')=?1",
            [&expected.id],
            |r| r.get(0),
        )?;
        // A previously attempted draft retains its original content for history.
        // When a subsequent save changed its revision, there is no current attempt;
        // preserve the row without making it an active retry target.
        let mut scope = scope(&tx, &expected.owner)?.ok_or(Fault::Invalid)?;
        scope.active = None;
        put(&tx, &scope)?;
        if historical == 0 {
            tx.execute("DELETE FROM drafts WHERE id=?1", [&expected.id])?;
        } else {
            let mut archived = expected.clone();
            archived.archived = true;
            archived.decision_revision = next(archived.decision_revision)?;
            put(&tx, &archived)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn retry_at(&self, owner: &Owner) -> Result<u64> {
        self.owner(owner)?;
        self.read_snapshot(|conn| Ok(scope(conn, owner)?.map_or(0, |s| s.retry_at)))
    }
}

fn read_attempt(conn: &Connection, owner: &Owner, id: &str) -> Result<Option<Attempt>> {
    let attempt = read::<Attempt>(conn, owner, id)?;
    if let Some(attempt) = &attempt {
        let draft = read::<Draft>(conn, owner, &attempt.draft_id)?.ok_or(Fault::Invalid)?;
        if draft.revision < attempt.draft_revision
            || (draft.revision == attempt.draft_revision && draft.content != attempt.frozen)
        {
            return Err(Fault::Invalid);
        }
    }
    Ok(attempt)
}

fn count(conn: &Connection, table: &str) -> Result<usize> {
    let value: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
    usize::try_from(value).map_err(|_| Fault::Invalid)
}
fn encode<T: Record>(value: &T) -> Result<String> {
    value.validate()?;
    let body = serde_json::to_string(value).map_err(|_| Fault::Invalid)?;
    if body.len() > MAX_ROW_BYTES {
        Err(Fault::Capacity)
    } else {
        Ok(body)
    }
}
fn decode<T: Record>(body: &str, account: &str, key: &str) -> Result<T> {
    if body.len() > MAX_ROW_BYTES {
        return Err(Fault::Invalid);
    }
    let value: T = serde_json::from_str(body).map_err(|_| Fault::Invalid)?;
    value.validate()?;
    if value.owner().account != account || value.key() != key {
        return Err(Fault::Invalid);
    }
    Ok(value)
}
fn read<T: Record>(conn: &Connection, owner: &Owner, key: &str) -> Result<Option<T>> {
    let body: Option<String> = conn.query_row(&format!("SELECT CASE WHEN length(CAST(body AS BLOB))<=?2 THEN body ELSE NULL END FROM {} WHERE id=?1",T::TABLE),
        params![key,MAX_ROW_BYTES as i64],|r|r.get(0)).optional()?;
    body.map(|body| {
        let value: T = decode(&body, &owner.account, key)?;
        if value.owner() != owner {
            return Err(Fault::Invalid);
        }
        Ok(value)
    })
    .transpose()
}
fn all<T: Record>(conn: &Connection, account: &str) -> Result<Vec<T>> {
    let max = match T::TABLE {
        "conflicts" | "late_evidence" => MAX_CONFLICTS,
        "scopes" => MAX_SCOPES,
        "drafts" => MAX_SCOPES + MAX_ATTEMPTS,
        _ => MAX_ATTEMPTS,
    };
    if count(conn, T::TABLE)? > max {
        return Err(Fault::Invalid);
    }
    let mut stmt = conn.prepare(&format!("SELECT id,CASE WHEN length(CAST(body AS BLOB))<=?1 THEN body ELSE NULL END FROM {} ORDER BY id",T::TABLE))?;
    let rows = stmt.query_map([MAX_ROW_BYTES as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.map(|r| {
        let (id, body) = r?;
        decode(&body, account, &id)
    })
    .collect()
}
fn insert<T: Record>(conn: &Connection, value: &T) -> Result<()> {
    if matches!(
        T::TABLE,
        "drafts" | "attempts" | "conflicts" | "decisions" | "late_evidence"
    ) {
        reject_retired_key(conn, T::TABLE, &value.key())?;
    }
    conn.execute(
        &format!("INSERT INTO {}(id,body) VALUES(?1,?2)", T::TABLE),
        params![value.key(), encode(value)?],
    )?;
    Ok(())
}
fn put<T: Record>(conn: &Connection, value: &T) -> Result<()> {
    conn.execute(&format!("INSERT INTO {}(id,body) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",T::TABLE),params![value.key(),encode(value)?])?;
    Ok(())
}
fn scope(conn: &Connection, owner: &Owner) -> Result<Option<Scope>> {
    read(conn, owner, &owner.key())
}
fn active(conn: &Connection, owner: &Owner) -> Result<Option<Draft>> {
    let Some(scope) = scope(conn, owner)? else {
        return Ok(None);
    };
    let Some(id) = scope.active else {
        return Ok(None);
    };
    let draft = read::<Draft>(conn, owner, &id)?.ok_or(Fault::Invalid)?;
    if draft.archived {
        return Err(Fault::Invalid);
    }
    association(conn, &draft)?;
    Ok(Some(draft))
}
fn exact_active(conn: &Connection, expected: &Draft) -> Result<()> {
    if active(conn, &expected.owner)?.as_ref() != Some(expected) {
        Err(Fault::Stale)
    } else {
        Ok(())
    }
}
fn association(conn: &Connection, draft: &Draft) -> Result<Option<Attempt>> {
    let Some(id) = &draft.attempt else {
        return Ok(None);
    };
    let attempt = read::<Attempt>(conn, &draft.owner, id)?.ok_or(Fault::Invalid)?;
    if attempt.draft_id != draft.id
        || attempt.draft_revision != draft.revision
        || attempt.frozen != draft.content
    {
        return Err(Fault::Invalid);
    }
    Ok(Some(attempt))
}
fn editable(conn: &Connection, draft: &Draft) -> Result<()> {
    if association(conn, draft)?.is_some_and(|a| !a.delivery.editable()) {
        Err(Fault::Blocked)
    } else {
        Ok(())
    }
}

fn active_scope_count(conn: &Connection) -> Result<usize> {
    let value: i64 = conn.query_row(
        "SELECT count(*) FROM scopes WHERE json_extract(body,'$.active') IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    usize::try_from(value).map_err(|_| Fault::Invalid)
}
fn reject_retired_key(conn: &Connection, table: &str, id: &str) -> Result<()> {
    let retired = match table {
        "attempts" => "retired_attempts",
        "conflicts" => "dispositions",
        "decisions" => "retired_decisions",
        "late_evidence" => "evidence_dispositions",
        _ => return Ok(()),
    };
    let exists: bool = conn.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {retired} WHERE id=?1)"),
        [id],
        |r| r.get(0),
    )?;
    if exists { Err(Fault::Retired) } else { Ok(()) }
}
