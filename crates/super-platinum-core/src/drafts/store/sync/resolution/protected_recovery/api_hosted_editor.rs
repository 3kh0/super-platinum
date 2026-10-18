//! Exact pre-send hosted set; neither a remote file decoder nor dispatch permit.
use super::*;
const LIMIT: i64 = 16;
const BYTES: i64 = 8 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    binding: Record,
}
impl Draft {
    pub fn id(&self) -> &str {
        self.binding.id()
    }
    pub fn input(&self) -> &Input {
        self.binding.input()
    }
    pub fn scope(&self) -> &Scope {
        self.binding.scope()
    }
}
fn present(conn: &Connection) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='composer_draft_hosted_drafts')",[],|r|r.get(0))?)
}
pub(super) fn install(conn: &Connection, account: &str) -> Result<(), AppError> {
    journal::require(conn, account)?;
    if !present(conn)? {
        conn.execute_batch("CREATE TABLE composer_draft_hosted_drafts(destination TEXT PRIMARY KEY,id TEXT NOT NULL UNIQUE,content TEXT NOT NULL);")?;
    }
    all(conn, account)?;
    Ok(())
}
pub(crate) fn all(conn: &Connection, account: &str) -> Result<Vec<Draft>, AppError> {
    journal::require(conn, account)?;
    if !present(conn)? {
        return Err(invalid("hosted editor schema missing"));
    }
    let (count,bytes,max):(i64,i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(length(CAST(content AS BLOB))+length(CAST(destination AS BLOB))+length(CAST(id AS BLOB))),0),COALESCE(MAX(length(CAST(content AS BLOB))),0) FROM composer_draft_hosted_drafts",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if count > LIMIT || bytes > LIMIT * BYTES || max > BYTES {
        return Err(invalid("hosted editor inventory bound"));
    }
    let mut q = conn.prepare(
        "SELECT destination,id,content FROM composer_draft_hosted_drafts ORDER BY destination",
    )?;
    let mut out = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (key, id, raw) = row?;
        let draft: Draft = wire::strict(&raw)?;
        draft.binding.validate()?;
        if draft.scope().account_id != account
            || draft.id() != id
            || draft.binding.returned().is_some()
            || serde_json::to_string(&draft.scope().key)? != key
            || !keys.insert(key)
            || !ids.insert(id)
        {
            return Err(invalid("hosted editor identity changed"));
        }
        out.push(draft);
    }
    Ok(out)
}
pub(crate) fn load(store: &DraftStore, scope: &Scope) -> Result<Option<Draft>, Error> {
    Ok(disk(all(&store.0, &scope.account_id))?
        .into_iter()
        .find(|d| d.scope() == scope))
}
/// Each edit compares the exact previously observed hosted row. Full caption,
/// writer receipt and protected inventory are rebound in the same transaction.
/// Clear removes the hosted set only; the original text draft stays intact.
pub(crate) fn save(
    store: &DraftStore,
    input: &runtime::Prepared,
    prior: Option<&Draft>,
    clear: bool,
) -> Result<Option<Draft>, Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let _live = input.stream.0.live_guard().map_err(|_| Error::Gap)?;
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let (binding, _) = prepare_record(
        &tx,
        &input.stream,
        &input.expected,
        &input.writer,
        input.serial,
        input.input.clone(),
    )?;
    let inventory = disk(all(&tx, input.account()))?;
    let current = inventory
        .iter()
        .find(|d| d.scope() == input.expected.scope());
    if disk(serde_json::to_value(current))? != disk(serde_json::to_value(prior))? {
        // An explicit immutable retry may observe a commit whose reply was
        // lost. Reconcile only the exact full requested writer/stream/inventory
        // binding above; never adopt a newer caption or invent a saved-row ID.
        if prior.is_some_and(|d| d.scope() != input.expected.scope()) {
            return Err(Error::Stale);
        }
        if clear && prior.is_some() && current.is_none() {
            return Ok(None);
        }
        if !clear && current.is_some_and(|d| d.binding.same_original(&binding).unwrap_or(false)) {
            return Ok(current.cloned());
        }
        return Err(Error::Stale);
    }
    if current.is_none() && inventory.len() >= LIMIT as usize {
        return Err(Error::Storage);
    }
    let key = disk(serde_json::to_string(&input.expected.scope().key))?;
    let result = if clear {
        if current.is_none() {
            return Err(Error::Stale);
        }
        disk(tx.execute(
            "DELETE FROM composer_draft_hosted_drafts WHERE destination=?1",
            [key],
        ))?;
        None
    } else {
        let draft = Draft { binding };
        let raw = disk(serde_json::to_string(&draft))?;
        if raw.len() > BYTES as usize {
            return Err(Error::Unsupported);
        }
        disk(tx.execute("INSERT INTO composer_draft_hosted_drafts VALUES(?1,?2,?3) ON CONFLICT(destination) DO UPDATE SET id=excluded.id,content=excluded.content",params![key,draft.id(),raw]))?;
        Some(draft)
    };
    disk(all(&tx, input.account()))?;
    disk(faults::before_commit(faults::FaultPoint::MutationReceipt))?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn check_claim(
    conn: &Connection,
    record: &Record,
    required: bool,
) -> Result<(), AppError> {
    let draft = if present(conn)? {
        all(conn, &record.scope().account_id)?
            .into_iter()
            .find(|d| d.scope() == record.scope())
    } else {
        None
    };
    match draft {
        Some(draft) if draft.binding.same_original(record)? => Ok(()),
        None if !required => Ok(()),
        _ => Err(invalid("hosted editor changed before claim")),
    }
}
pub(super) fn guard(conn: &Connection, scope: &Scope) -> Result<(), AppError> {
    if !present(conn)? {
        return Ok(());
    }
    if all(conn, &scope.account_id)?
        .iter()
        .any(|d| d.scope() == scope)
    {
        return Err(invalid(
            "hosted editor requires hosted dispatch or explicit removal",
        ));
    }
    Ok(())
}

pub(super) fn matching_claim(conn: &Connection, record: &Record) -> Result<bool, AppError> {
    Ok(all(conn, &record.scope().account_id)?
        .into_iter()
        .find(|d| d.scope() == record.scope())
        .map(|d| record.matches_saved_draft(&d.binding))
        .transpose()?
        .unwrap_or(false))
}
pub(super) fn remove_claim(conn: &Connection, record: &Record) -> Result<(), AppError> {
    if !matching_claim(conn, record)? {
        return Err(invalid("hosted clear editor changed"));
    }
    if conn.execute(
        "DELETE FROM composer_draft_hosted_drafts WHERE destination=?1",
        [serde_json::to_string(&record.scope().key)?],
    )? != 1
    {
        return Err(invalid("hosted clear editor missing"));
    }
    Ok(())
}
