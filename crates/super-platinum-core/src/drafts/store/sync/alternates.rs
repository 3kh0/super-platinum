//! One durable latest alternate per editor, never one record per keystroke.
use super::*;

#[derive(Clone, Debug)]
pub struct LocalWriter(String);
impl Default for LocalWriter {
    fn default() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}
impl LocalWriter {
    pub fn id(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct LocalAlternate {
    pub writer_id: String,
    pub revision: u64,
    pub draft: LocalDraft,
}

pub(super) fn has_alternates(conn: &Connection, scope: &Scope) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_alternates WHERE destination=?1)",
        [serde_json::to_string(&scope.key)?],
        |row| row.get(0),
    )?)
}

/// Settled revisions remain retired across restart and delayed worker replies.
pub(super) fn reviewed_revision(
    conn: &Connection,
    scope: &Scope,
    writer: &LocalWriter,
) -> Result<Option<u64>, AppError> {
    let revision: Option<i64> = conn.query_row(
        "SELECT revision FROM composer_draft_reviewed_writers WHERE destination=?1 AND writer_id=?2",
        params![serde_json::to_string(&scope.key)?, writer.id()], |row| row.get(0),
    ).optional()?;
    Ok(revision.map(|revision| revision as u64))
}

pub(super) fn preserve_owned_alternate(
    conn: &Connection,
    scope: &Scope,
    writer: &LocalWriter,
    revision: u64,
    local: &LocalDraft,
) -> Result<(), AppError> {
    if scope.key.team.is_empty()
        || scope.key.channel.is_empty()
        || scope.key.thread.as_ref().is_some_and(String::is_empty)
        || revision == 0
        || revision > i64::MAX as u64
    {
        return Err(invalid("invalid alternate input scope/revision"));
    }
    if reviewed_revision(conn, scope, writer)?.is_some_and(|floor| revision <= floor) {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO composer_draft_alternates(destination,writer_id,revision,content) VALUES(?1,?2,?3,?4)
         ON CONFLICT(destination,writer_id) DO UPDATE SET revision=excluded.revision,content=excluded.content
         WHERE excluded.revision>composer_draft_alternates.revision",
        params![serde_json::to_string(&scope.key)?,writer.id(),revision as i64,serde_json::to_string(local)?],
    )?;
    Ok(())
}

pub(super) fn read_alternates(
    conn: &Connection,
) -> Result<BTreeMap<DraftKey, Vec<LocalAlternate>>, AppError> {
    let mut query = conn.prepare("SELECT destination,writer_id,revision,content FROM composer_draft_alternates ORDER BY destination,writer_id")?;
    let rows = query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut result: BTreeMap<DraftKey, Vec<LocalAlternate>> = BTreeMap::new();
    for row in rows {
        let (key, writer_id, revision, content) = row?;
        if uuid::Uuid::parse_str(&writer_id).is_err() || revision <= 0 {
            return Err(invalid("invalid alternate writer/revision"));
        }
        result
            .entry(serde_json::from_str(&key)?)
            .or_default()
            .push(LocalAlternate {
                writer_id,
                revision: revision as u64,
                draft: serde_json::from_str(&content)?,
            });
    }
    Ok(result)
}

impl DraftStore {
    pub fn save_owned_local_alternate(
        &mut self,
        scope: &Scope,
        local: &LocalDraft,
        writer: &LocalWriter,
        revision: u64,
    ) -> Result<(), AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        if reviewed_revision(&tx, scope, writer)?.is_some_and(|floor| revision <= floor) {
            tx.commit()?;
            return Ok(());
        }
        if read(&tx, scope)?.is_none() {
            return Err(invalid("alternate input requires a managed draft"));
        }
        preserve_conflict(&tx, scope, local)?;
        preserve_owned_alternate(&tx, scope, writer, revision, local)?;
        tx.commit()?;
        Ok(())
    }

    pub(in crate::drafts::store) fn preserve_owned_local_conflict(
        &mut self,
        scope: &Scope,
        local: &LocalDraft,
        writer: &LocalWriter,
        revision: u64,
    ) -> Result<(), AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        if reviewed_revision(&tx, scope, writer)?.is_some_and(|floor| revision <= floor) {
            tx.commit()?;
            return Ok(());
        }
        preserve_conflict(&tx, scope, local)?;
        preserve_owned_alternate(&tx, scope, writer, revision, local)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
