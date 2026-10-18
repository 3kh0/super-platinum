//! Exact terminal history; its lifetime is the accepted Share receipt's lifetime.
use super::*;
const MAX_COMPLETED_BODY: usize = BODY + super::super::MAX_BODY + 512 * 1024;
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    version: u32,
    open: Record,
    linked: drafts::Draft,
    receipt: Receipt,
}
/// Completed acknowledges historical custody only. It is not an active open
/// Record and cannot continue into file dispatch or clear a current editor.
pub enum Returned {
    Active(Record),
    Completed,
}

pub(super) fn migrate(tx: &Transaction<'_>, version: i64) -> Result<(), AppError> {
    let sql = "CREATE TABLE file_share_direct_completed (open_attempt TEXT PRIMARY KEY,share_attempt TEXT NOT NULL UNIQUE,body TEXT NOT NULL)";
    let existing:Option<String>=tx.query_row("SELECT sql FROM sqlite_schema WHERE type='table' AND name='file_share_direct_completed'",[],|r|r.get(0)).optional()?;
    match existing {
        Some(value) if value == sql => (),
        None if version < 4 => tx.execute_batch(sql)?,
        _ => return Err(invalid()),
    }
    Ok(())
}
pub(super) fn confirms(record: &Record, destination: &str) -> bool {
    id(destination, b'D')
        && (record
            .resolved
            .as_ref()
            .is_some_and(|v| v["channel"]["id"] == destination)
            || record
                .returned
                .as_ref()
                .is_some_and(|v| matching_return(v, &record.peer, destination)))
}
fn matching_return(raw: &Value, peer: &str, destination: &str) -> bool {
    envelope(raw)
        && raw["channel"]["id"] == destination
        && raw["channel"]
            .get("user")
            .is_none_or(|v| v.as_str() == Some(peer))
        && raw["channel"]
            .get("is_im")
            .is_none_or(|v| v == &Value::Bool(true))
        && ["is_mpim", "is_channel", "is_archived", "is_ext_shared"]
            .iter()
            .all(|k| {
                raw["channel"]
                    .get(*k)
                    .is_none_or(|v| v == &Value::Bool(false))
            })
}
fn validate_completion(value: &Completion) -> Result<(), AppError> {
    validate(&value.open)?;
    drafts::encode(&value.linked)?;
    super::super::validate(&value.receipt)?;
    let original = &value.open.draft;
    let linked = &value.linked;
    let receipt = &value.receipt;
    if value.version != 1
        || !matches!(
            receipt.state(),
            State::Server(FileShareDisposition::Accepted { .. })
        )
        || original.scope != linked.scope
        || original.content.source != linked.content.source
        || original.content.title != linked.content.title
        || original.content.note != linked.content.note
        || linked.scope.team != receipt.scope.team
        || linked.scope.user != receipt.scope.user
        || linked.scope.file != receipt.scope.file
        || linked.content.title != receipt.file_title
        || linked.content.note.as_slice() != receipt.intent.note().unwrap_or(&[])
        || linked
            .content
            .destination
            .as_ref()
            .is_none_or(|d| d.id != receipt.scope.destination || d.name != receipt.destination_name)
        || !confirms(&value.open, &receipt.scope.destination)
    {
        return Err(invalid());
    }
    Ok(())
}
fn encode_completion(value: &Completion) -> Result<String, AppError> {
    validate_completion(value)?;
    let body = serde_json::to_string(value)?;
    if body.len() > MAX_COMPLETED_BODY {
        return Err(invalid());
    }
    Ok(body)
}
fn get(conn: &Connection, attempt: &str) -> Result<Option<Completion>, AppError> {
    let row:Option<(String,Option<String>)>=conn.query_row("SELECT share_attempt,CASE WHEN length(CAST(body AS BLOB))<=?2 THEN body ELSE NULL END FROM file_share_direct_completed WHERE open_attempt=?1",params![attempt,MAX_COMPLETED_BODY as i64],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    row.map(|(share, body)| {
        let value: Completion = serde_json::from_str(&body.ok_or_else(invalid)?)?;
        validate_completion(&value)?;
        if value.open.id != attempt
            || value.receipt.attempt_id != share
            || super::super::get(conn, &value.receipt.scope)?.as_ref() != Some(&value.receipt)
        {
            return Err(invalid());
        }
        Ok(value)
    })
    .transpose()
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    open: &Record,
    linked: &drafts::Draft,
    receipt: &Receipt,
) -> Result<(), AppError> {
    let value = Completion {
        version: 1,
        open: open.clone(),
        linked: linked.clone(),
        receipt: receipt.clone(),
    };
    let body = encode_completion(&value)?;
    if super::super::get(tx, &receipt.scope)?.as_ref() != Some(receipt) {
        return Err(invalid());
    }
    let count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM file_share_direct_completed",
        [],
        |r| r.get(0),
    )?;
    if count < 0 || count >= super::super::MAX_TERMINAL as i64 {
        return Err(invalid());
    }
    tx.execute(
        "INSERT INTO file_share_direct_completed(open_attempt,share_attempt,body) VALUES(?1,?2,?3)",
        params![open.id, receipt.attempt_id, body],
    )?;
    Ok(())
}
pub(in crate::file_sharing::store) fn retire_completed(
    tx: &Transaction<'_>,
    receipt: &Receipt,
) -> Result<(), AppError> {
    let open: Option<String> = tx
        .query_row(
            "SELECT open_attempt FROM file_share_direct_completed WHERE share_attempt=?1",
            [&receipt.attempt_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(open) = open else { return Ok(()) };
    let value = get(tx, &open)?.ok_or_else(invalid)?;
    if &value.receipt != receipt {
        return Err(invalid());
    }
    if tx.execute(
        "DELETE FROM file_share_direct_completed WHERE open_attempt=?1 AND share_attempt=?2",
        params![open, receipt.attempt_id],
    )? != 1
    {
        return Err(invalid());
    }
    Ok(())
}
fn returned(tx: &Transaction<'_>, expected: &Record, raw: Value) -> Result<Returned, AppError> {
    validate(expected)?;
    let mut value = get(tx, &expected.id)?.ok_or_else(invalid)?;
    let before = encode_completion(&value)?;
    let mut original = value.open.clone();
    original.returned = expected.returned.clone();
    original.resolved = expected.resolved.clone();
    if &original != expected
        || expected
            .returned
            .as_ref()
            .is_some_and(|v| value.open.returned.as_ref() != Some(v))
        || expected
            .resolved
            .as_ref()
            .is_some_and(|v| value.open.resolved.as_ref() != Some(v))
        || !matching_return(&raw, &expected.peer, &value.receipt.scope.destination)
        || value.open.returned.as_ref().is_some_and(|v| v != &raw)
    {
        return Err(invalid());
    }
    value.open.returned = Some(raw);
    let after = encode_completion(&value)?;
    if tx.execute("UPDATE file_share_direct_completed SET body=?1 WHERE open_attempt=?2 AND share_attempt=?3 AND body=?4",params![after,expected.id,value.receipt.attempt_id,before])?!=1{return Err(invalid());}
    Ok(Returned::Completed)
}
impl Journal {
    /// A late response can attach only to the exact active record or its exact
    /// retained accepted association. Missing/pruned history is never success.
    pub fn settle_direct_returned(
        &mut self,
        expected: &Record,
        raw: Value,
    ) -> Result<Returned, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = super::get(
            &tx,
            &expected.draft.scope.team,
            &expected.draft.scope.user,
            &expected.peer,
        )?;
        let result = if current.as_ref().is_some_and(|v| v.id == expected.id) {
            Returned::Active(super::write_returned(&tx, expected, raw)?)
        } else {
            returned(&tx, expected, raw)?
        };
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
