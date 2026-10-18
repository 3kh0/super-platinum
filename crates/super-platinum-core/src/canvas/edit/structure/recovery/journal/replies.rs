//! Canvas reply drafts and immutable one-attempt receipts use the existing
//! account journal. Restored attempts are evidence, never dispatch authority.
use super::*;
use crate::canvas::comments::reply::{ReplyAccepted, ReplyRecord, valid_text};
use serde::{Deserialize, Serialize};
mod delivery;
mod drafts;
#[cfg(test)]
mod tests;
const MAX_VERSIONS: usize = 16;
const MAX_REPLY_BODY: usize = 192 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyDraft {
    id: String,
    revision: u64,
    capture: String,
    text: String,
}
impl ReplyDraft {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn capture(&self) -> &str {
        &self.capture
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyReceipt {
    id: String,
    record: ReplyRecord,
    draft: ReplyDraft,
}
impl ReplyReceipt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn record(&self) -> &ReplyRecord {
        &self.record
    }
    pub fn draft(&self) -> &ReplyDraft {
        &self.draft
    }
}
#[derive(Clone)]
pub struct ReplyInventory {
    scope: Scope,
    root: String,
    revision: u64,
    drafts: Vec<ReplyDraft>,
    attempt: Option<ReplyReceipt>,
}
impl ReplyInventory {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn drafts(&self) -> &[ReplyDraft] {
        &self.drafts
    }
    pub fn attempt(&self) -> Option<&ReplyReceipt> {
        self.attempt.as_ref()
    }
    pub fn root_ts(&self) -> &str {
        &self.root
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
}
pub enum ReplySave {
    Stored(ReplyDraft),
    Retired,
}
fn uuid(value: &str) -> Result<(), AppError> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid())
}
fn owner(scope: &Scope, root: &str) -> Result<(), AppError> {
    scope_valid(scope)?;
    crate::canvas::comments::channel(&scope.file).map_err(|_| invalid())?;
    if !crate::canvas::comments::timestamp(root) {
        return Err(invalid());
    }
    Ok(())
}
fn encoded<T: Serialize>(value: &T) -> Result<String, AppError> {
    let body = serde_json::to_string(value)?;
    if body.len() > MAX_REPLY_BODY {
        return Err(invalid());
    }
    Ok(body)
}
fn draft_valid(value: &ReplyDraft) -> Result<(), AppError> {
    uuid(&value.id)?;
    uuid(&value.capture)?;
    if value.revision == 0 || value.revision > i64::MAX as u64 || !valid_text(&value.text) {
        return Err(invalid());
    }
    Ok(())
}
fn receipt_valid(value: &ReplyReceipt, scope: &Scope) -> Result<(), AppError> {
    uuid(&value.id)?;
    draft_valid(&value.draft)?;
    value.record.validate().map_err(|_| invalid())?;
    if value.id != value.record.client_msg_id()
        || value.record.workspace() != scope.workspace
        || value.record.user() != scope.user
        || value.record.file() != scope.file
        || value.record.text() != value.draft.text
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn present(conn: &Connection, scope: &Scope) -> Result<bool, AppError> {
    scope_valid(scope)?;
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM reply_attempts WHERE workspace=?1 AND user=?2 AND file=?3)",
        params![scope.workspace, scope.user, scope.file],
        |r| r.get(0),
    )?)
}
fn read(conn: &Connection, scope: &Scope, root: &str) -> Result<ReplyInventory, AppError> {
    owner(scope, root)?;
    let mut statement=conn.prepare("SELECT id,CASE WHEN length(CAST(body AS BLOB))<=?5 THEN body ELSE NULL END FROM reply_drafts WHERE workspace=?1 AND user=?2 AND file=?3 AND root=?4 ORDER BY id LIMIT 17")?;
    let values = statement.query_map(
        params![
            scope.workspace,
            scope.user,
            scope.file,
            root,
            MAX_REPLY_BODY as i64
        ],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    let mut drafts = Vec::new();
    for value in values {
        let (id, body) = value?;
        let value: ReplyDraft = serde_json::from_str(&body)?;
        draft_valid(&value)?;
        if value.id != id {
            return Err(invalid());
        }
        drafts.push(value)
    }
    if drafts.len() > MAX_VERSIONS {
        return Err(invalid());
    }
    let row:Option<(String,String)>=conn.query_row("SELECT id,CASE WHEN length(CAST(body AS BLOB))<=?4 THEN body ELSE NULL END FROM reply_attempts WHERE workspace=?1 AND user=?2 AND file=?3",params![scope.workspace,scope.user,scope.file,MAX_REPLY_BODY as i64],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let attempt = match row {
        None => None,
        Some((id, body)) => {
            let value: ReplyReceipt = serde_json::from_str(&body)?;
            receipt_valid(&value, scope)?;
            if value.id != id {
                return Err(invalid());
            }
            Some(value)
        }
    };
    let revision: i64 =
        conn.query_row("SELECT value FROM reply_clock WHERE singleton=1", [], |r| {
            r.get(0)
        })?;
    Ok(ReplyInventory {
        scope: scope.clone(),
        root: root.into(),
        revision: u64::try_from(revision).map_err(|_| invalid())?,
        drafts,
        attempt,
    })
}
impl Journal {
    pub fn reply_inventory(&self, scope: &Scope, root: &str) -> Result<ReplyInventory, AppError> {
        let tx = self.conn.unchecked_transaction()?;
        let result = read(&tx, scope, root)?;
        tx.commit()?;
        Ok(result)
    }
}

fn advance(conn: &Connection) -> Result<(), AppError> {
    if conn.execute(
        "UPDATE reply_clock SET value=value+1 WHERE singleton=1 AND value<9223372036854775807",
        [],
    )? != 1
    {
        return Err(invalid());
    }
    Ok(())
}

impl PartialEq for ReplyInventory {
    fn eq(&self, o: &Self) -> bool {
        self.scope == o.scope
            && self.root == o.root
            && self.drafts == o.drafts
            && self.attempt == o.attempt
    }
}
impl Eq for ReplyInventory {}

#[derive(Clone)]
pub struct ReplyHistory {
    pub receipt: ReplyReceipt,
    pub status: String,
    pub observed: serde_json::Value,
}
impl Journal {
    pub fn reply_history(
        &self,
        scope: &Scope,
        after: Option<&str>,
    ) -> Result<Vec<ReplyHistory>, AppError> {
        scope_valid(scope)?;
        if let Some(id) = after {
            uuid(id)?
        }
        let mut query=self.conn.prepare("SELECT CASE WHEN length(CAST(body AS BLOB))<=196608 THEN body ELSE NULL END,CASE WHEN length(CAST(observed AS BLOB))<=196608 THEN observed ELSE NULL END,status FROM reply_history WHERE workspace=?1 AND user=?2 AND file=?3 AND id>?4 ORDER BY id LIMIT 20")?;
        let rows = query.query_map(
            params![scope.workspace, scope.user, scope.file, after.unwrap_or("")],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )?;
        let mut result = Vec::new();
        for row in rows {
            let (body, observed, status) = row?;
            if body.len() > MAX_REPLY_BODY
                || observed.len() > MAX_REPLY_BODY
                || !matches!(
                    status.as_str(),
                    "not_sent" | "acknowledged" | "reviewed_outcome_unknown"
                )
            {
                return Err(invalid());
            }
            let receipt: ReplyReceipt = serde_json::from_str(&body)?;
            receipt_valid(&receipt, scope)?;
            result.push(ReplyHistory {
                receipt,
                status,
                observed: serde_json::from_str(&observed)?,
            })
        }
        Ok(result)
    }
}
