//! Durable pre-open custody. Reading a record never authorizes another open.
use super::*;
use serde::{Deserialize, Serialize};
const LIMIT: usize = 32;
mod completed;
pub use completed::Returned;
pub(super) use completed::retire_completed;
const BODY: usize = 2 * 1024 * 1024;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    version: u32,
    id: String,
    draft: drafts::Draft,
    peer: String,
    peer_response: Value,
    created_at: u64,
    // None includes dispatched with no response, a crash, or predispatch abort.
    // All are non-replayable. The response is stored before checking the UI.
    #[serde(deserialize_with = "required_option")]
    returned: Option<Value>,
    #[serde(deserialize_with = "required_option")]
    resolved: Option<Value>,
}
impl Record {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn draft(&self) -> &drafts::Draft {
        &self.draft
    }
    pub fn peer(&self) -> &str {
        &self.peer
    }
    pub fn returned(&self) -> Option<&Value> {
        self.returned.as_ref()
    }
    pub fn resolved(&self) -> Option<&Value> {
        self.resolved.as_ref()
    }
    pub fn same_content(&self, draft: &drafts::Draft) -> bool {
        self.draft.scope == draft.scope && self.draft.content == draft.content
    }
}
pub enum Begin {
    Created(Record),
    Existing(Record),
    Capacity,
}
fn required_option<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Option::<Value>::deserialize(d)
}
pub(super) fn migrate(tx: &Transaction<'_>, version: i64) -> Result<(), AppError> {
    let sql = "CREATE TABLE file_share_direct_opens (team TEXT NOT NULL,user TEXT NOT NULL,peer TEXT NOT NULL,attempt TEXT NOT NULL UNIQUE,body TEXT NOT NULL,PRIMARY KEY(team,user,peer))";
    let existing: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='table' AND name='file_share_direct_opens'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(value) if value == sql => (),
        None if version < 3 => tx.execute_batch(sql)?,
        _ => return Err(invalid()),
    }
    completed::migrate(tx, version)?;
    Ok(())
}

fn id(s: &str, prefix: u8) -> bool {
    (9..=64).contains(&s.len())
        && s.as_bytes()[0] == prefix
        && s.bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
fn bounded(raw: &Value) -> bool {
    let mut stack = vec![(raw, 0)];
    let mut count = 0usize;
    while let Some((v, depth)) = stack.pop() {
        count += 1;
        if count > 65_536 || depth > 32 {
            return false;
        }
        match v {
            Value::Array(a) => stack.extend(a.iter().map(|v| (v, depth + 1))),
            Value::Object(a) => stack.extend(a.values().map(|v| (v, depth + 1))),
            _ => (),
        }
        if stack.len() > 65_536 {
            return false;
        }
    }
    serde_json::to_vec(raw).is_ok_and(|v| v.len() <= 512 * 1024)
}
fn envelope(raw: &Value) -> bool {
    raw["ok"] == true && raw.get("error").is_none_or(Value::is_null) && bounded(raw)
}
fn validate(record: &Record) -> Result<(), AppError> {
    drafts::encode(&record.draft)?;
    number(record.created_at)?;
    if record.version != 1
        || uuid::Uuid::parse_str(&record.id).is_err()
        || !id(&record.peer, b'U')
        || record
            .draft
            .content
            .destination
            .as_ref()
            .map(|d| d.id.as_str())
            != Some(format!("person:{}", record.peer).as_str())
        || !envelope(&record.peer_response)
        || record.peer_response["results"]
            .as_array()
            .is_none_or(|v| v.len() != 1)
        || record
            .peer_response
            .get("failed_ids")
            .is_some_and(|v| v.as_array().is_none_or(|v| !v.is_empty()))
        || record.peer_response["results"][0]["id"] != record.peer
        || record.peer_response["results"][0]["deleted"] != false
        || record.peer_response["results"][0]["is_bot"] != false
        || record
            .returned
            .as_ref()
            .is_some_and(|v| !v.is_object() || !bounded(v))
    {
        return Err(invalid());
    }
    if let Some(raw) = &record.resolved {
        if !envelope(raw)
            || !id(raw["channel"]["id"].as_str().unwrap_or_default(), b'D')
            || raw["channel"]["user"] != record.peer
            || raw["channel"]["is_im"] != true
            || raw["channel"]["is_ext_shared"] != false
            || raw["channel"]["is_open"] != true
            || [
                "is_mpim",
                "is_channel",
                "is_archived",
                "is_frozen",
                "is_pending_ext_shared",
                "is_file",
            ]
            .iter()
            .any(|k| {
                raw["channel"]
                    .get(*k)
                    .is_some_and(|v| v != &Value::Bool(false))
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn encode(record: &Record) -> Result<String, AppError> {
    validate(record)?;
    let body = serde_json::to_string(record)?;
    if body.len() > BODY {
        return Err(invalid());
    }
    Ok(body)
}
fn get(conn: &Connection, team: &str, user: &str, peer: &str) -> Result<Option<Record>, AppError> {
    let row: Option<(String, Option<String>)> = conn.query_row(
        "SELECT attempt,CASE WHEN length(CAST(body AS BLOB))<=?4 THEN body ELSE NULL END FROM file_share_direct_opens WHERE team=?1 AND user=?2 AND peer=?3",
        params![team,user,peer,BODY as i64], |r| Ok((r.get(0)?,r.get(1)?)),
    ).optional()?;
    row.map(|(attempt, body)| {
        let value: Record = serde_json::from_str(&body.ok_or_else(invalid)?)?;
        validate(&value)?;
        if value.id != attempt
            || value.draft.scope.team != team
            || value.draft.scope.user != user
            || value.peer != peer
        {
            return Err(invalid());
        }
        Ok(value)
    })
    .transpose()
}
impl Journal {
    /// Only called by explicit Share after saving the captured note. This durable
    /// unknown marker commits before the sole open request may be dispatched.
    pub fn begin_direct_open(
        &mut self,
        draft: &drafts::Draft,
        peer: Value,
        now: u64,
    ) -> Result<Begin, AppError> {
        let person = draft
            .content
            .destination
            .as_ref()
            .and_then(|d| d.id.strip_prefix("person:"))
            .ok_or_else(invalid)?
            .to_owned();
        let record = Record {
            version: 1,
            id: uuid::Uuid::new_v4().to_string(),
            draft: draft.clone(),
            peer: person,
            peer_response: peer,
            created_at: now,
            returned: None,
            resolved: None,
        };
        let body = encode(&record)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        drafts::check_current(&tx, draft)?;
        if let Some(previous) = get(&tx, &draft.scope.team, &draft.scope.user, &record.peer)? {
            return Ok(Begin::Existing(previous));
        }
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM file_share_direct_opens", [], |r| {
            r.get(0)
        })?;
        if count >= LIMIT as i64 {
            return Ok(Begin::Capacity);
        }
        tx.execute("INSERT INTO file_share_direct_opens(team,user,peer,attempt,body) VALUES(?1,?2,?3,?4,?5)",
            params![draft.scope.team,draft.scope.user,record.peer,record.id,body])?;
        tx.commit()?;
        Ok(Begin::Created(record))
    }
    /// Same returned bytes, same original owner. No draft/UI-current predicate:
    /// changing account or recipient must not discard the already-returned data.
    pub fn direct_open_returned(
        &mut self,
        expected: &Record,
        raw: Value,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = write_returned(&tx, expected, raw)?;
        tx.commit()?;
        Ok(current)
    }
    /// A fresh, already-open DM from im.list + conversations.info. This records
    /// the exact read response; it neither opens a DM nor authorizes file Share.
    pub fn resolve_direct_open(
        &mut self,
        expected: &Record,
        raw: Value,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = get(
            &tx,
            &expected.draft.scope.team,
            &expected.draft.scope.user,
            &expected.peer,
        )?
        .ok_or_else(invalid)?;
        if current.id != expected.id
            || current.draft != expected.draft
            || current.peer_response != expected.peer_response
            || current.returned != expected.returned
        {
            return Err(invalid());
        }
        if current.resolved.as_ref() == Some(&raw) {
            return Ok(current);
        }
        if current.resolved.is_some() {
            return Err(invalid());
        }
        current.resolved = Some(raw);
        write(&tx, &current)?;
        tx.commit()?;
        Ok(current)
    }
    pub fn direct_opens(&self, team: &str, user: &str) -> Result<Vec<Record>, AppError> {
        let mut stmt = self.conn.prepare("SELECT peer FROM file_share_direct_opens WHERE team=?1 AND user=?2 ORDER BY peer LIMIT ?3")?;
        let rows = stmt.query_map(params![team, user, (LIMIT + 1) as i64], |r| {
            r.get::<_, String>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(get(&self.conn, team, user, &row?)?.ok_or_else(invalid)?);
        }
        if out.len() > LIMIT {
            return Err(invalid());
        }
        Ok(out)
    }
}
fn write_returned(tx: &Transaction<'_>, expected: &Record, raw: Value) -> Result<Record, AppError> {
    let mut current = get(
        tx,
        &expected.draft.scope.team,
        &expected.draft.scope.user,
        &expected.peer,
    )?
    .ok_or_else(invalid)?;
    if current.returned.as_ref() == Some(&raw) && current.id == expected.id {
        let mut prior = current.clone();
        prior.returned = expected.returned.clone();
        prior.resolved = expected.resolved.clone();
        if &prior == expected {
            return Ok(current);
        }
    }
    let mut before = current.clone();
    before.resolved = expected.resolved.clone();
    if &before != expected || current.returned.is_some() {
        return Err(invalid());
    }
    current.returned = Some(raw);
    write(tx, &current)?;
    Ok(current)
}
fn write(tx: &Transaction<'_>, record: &Record) -> Result<(), AppError> {
    let changed = tx.execute("UPDATE file_share_direct_opens SET body=?1 WHERE team=?2 AND user=?3 AND peer=?4 AND attempt=?5",
        params![encode(record)?,record.draft.scope.team,record.draft.scope.user,record.peer,record.id])?;
    if changed != 1 {
        return Err(invalid());
    }
    Ok(())
}
mod retirement;
pub(super) use retirement::accepted;
#[cfg(test)]
mod tests;
