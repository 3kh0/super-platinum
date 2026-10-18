//! Accepted-request settlement; no replay, invented per-file ACK or restored clear.
use super::*;
#[path = "api_hosted_retirement.rs"]
pub mod retirement;
use crate::slack::api::file_share::{FileShareDisposition, decode_existing_file_share};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    ClearedExact,
    PreservedCurrent,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    version: u32,
    record: Record,
    effect: Effect,
    before: views::InventoryRecord,
    after: views::InventoryRecord,
}
impl Receipt {
    pub fn record(&self) -> &Record {
        &self.record
    }
    pub fn effect(&self) -> Effect {
        self.effect
    }
    fn validate(&self) -> Result<(), AppError> {
        self.record.validate()?;
        let before = views::managed(self.before.restore()?)
            .map_err(|_| invalid("hosted completion before"))?;
        let after = views::managed(self.after.restore()?)
            .map_err(|_| invalid("hosted completion after"))?;
        if self.version != 1
            || !accepted(&self.record)
            || before.scope() != self.record.scope()
            || after.scope() != self.record.scope()
        {
            return Err(invalid("hosted completion identity"));
        }
        match self.effect {
            Effect::PreservedCurrent if !before.same(&after) => {
                return Err(invalid("hosted preserve changed input"));
            }
            Effect::ClearedExact => {
                if !before.same(&self.record.claimed()?) {
                    return Err(invalid("hosted clear basis changed"));
                }
                let expected = clear_row(&before)?;
                if !after.row().inner.matches(&expected) || !after.local().empty() {
                    return Err(invalid("hosted clear result changed"));
                }
                // Full inventory equality, including alternates, origins, clock
                // and writer-local metadata, is checked against the actual after.
                let mut expected_inventory = views::local_from_review(&before);
                expected_inventory.review.local = LocalDraft::default();
                expected_inventory.review.stored = Some(expected.stored.clone());
                expected_inventory.row = Some(Row::new(expected));
                if !before.local().empty() {
                    expected_inventory.clock = expected_inventory
                        .clock
                        .checked_add(2)
                        .ok_or_else(|| invalid("hosted local clock overflow"))?;
                    expected_inventory.generation = None;
                }
                if !views::managed(expected_inventory)
                    .map_err(|_| invalid("hosted clear inventory"))?
                    .same(&after)
                {
                    return Err(invalid("hosted clear inventory changed"));
                }
            }
            _ => (),
        }
        Ok(())
    }
}
pub struct FreshClear {
    receipt: Receipt,
}
impl FreshClear {
    pub fn original(&self) -> &Input {
        self.receipt.record.input()
    }
    pub fn claim_id(&self) -> &str {
        self.receipt.record.id()
    }
    pub fn after(&self) -> Result<Review, Error> {
        views::managed(disk(self.receipt.after.restore())?)
    }
}
pub(crate) struct Completed {
    pub(crate) receipt: Receipt,
    pub(crate) fresh: Option<FreshClear>,
}
fn accepted(record: &Record) -> bool {
    matches!(record.returned(),Some(Returned::Response(raw)) if matches!(decode_existing_file_share(raw.clone()).disposition(),FileShareDisposition::Accepted{..}))
}
fn clear_row(before: &Review) -> Result<ProtectedRow, AppError> {
    let state = if before.row().state().desired().is_some() {
        before
            .row()
            .state()
            .clear_acknowledged_slash_input()
            .ok_or_else(|| invalid("hosted clear not idle"))?
    } else {
        before.row().state().clone()
    };
    successor(&before.row().inner, state)
}
fn present(conn: &Connection) -> Result<bool, AppError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='composer_draft_hosted_completions')",[],|r|r.get(0))?)
}
pub(super) fn install(conn: &Connection, account: &str) -> Result<(), AppError> {
    journal::require(conn, account)?;
    if !present(conn)? {
        conn.execute_batch("CREATE TABLE composer_draft_hosted_completions(id TEXT PRIMARY KEY,destination TEXT NOT NULL,content TEXT NOT NULL);")?;
    }
    all(conn, account)?;
    Ok(())
}
pub(crate) fn all(conn: &Connection, account: &str) -> Result<Vec<Receipt>, AppError> {
    journal::require(conn, account)?;
    if !present(conn)? {
        return Err(invalid("hosted completion schema missing"));
    }
    let (count,bytes,max):(i64,i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(length(CAST(content AS BLOB))+length(CAST(id AS BLOB))+length(CAST(destination AS BLOB))),0),COALESCE(MAX(length(CAST(content AS BLOB))),0) FROM composer_draft_hosted_completions",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if count > 32 || bytes > 256 * 1024 * 1024 || max > 8 * 1024 * 1024 {
        return Err(invalid("hosted completion bound"));
    }
    let mut q = conn.prepare(
        "SELECT id,destination,content FROM composer_draft_hosted_completions ORDER BY id",
    )?;
    let mut out = vec![];
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, key, raw) = row?;
        let receipt: Receipt = wire::strict(&raw)?;
        receipt.validate()?;
        if receipt.record.id() != id
            || receipt.record.scope().account_id != account
            || serde_json::to_string(&receipt.record.scope().key)? != key
        {
            return Err(invalid("hosted completion index"));
        }
        out.push(receipt);
    }
    Ok(out)
}
pub(super) fn historical_record(
    conn: &Connection,
    account: &str,
    id: &str,
) -> Result<Option<Record>, AppError> {
    if !present(conn)? {
        return Ok(None);
    }
    Ok(all(conn, account)?
        .into_iter()
        .find(|r| r.record.id() == id)
        .map(|r| r.record))
}
/// Only the original owned Claim and stream reach this entry point. A restored
/// receipt is read-only and cannot create FreshClear or another dispatch.
pub(crate) fn complete(
    store: &DraftStore,
    claim: &Claim,
    stream: &Stream,
) -> Result<Completed, Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let scope = claim.record().scope();
    let history = disk(all(&tx, &scope.account_id))?;
    if let Some(prior) = history
        .iter()
        .find(|r| r.record.id() == claim.record().id())
    {
        if !disk(prior.record.same_claim(claim.record()))? {
            return Err(Error::Stale);
        }
        return Ok(Completed {
            receipt: prior.clone(),
            fresh: None,
        });
    }
    if history.len() >= 32 {
        return Err(Error::Storage);
    }
    let record = disk(journal::all(&tx, &scope.account_id))?
        .into_iter()
        .find(|r| r.id() == claim.record().id())
        .ok_or(Error::Stale)?;
    if !disk(record.same_claim(claim.record()))? || !accepted(&record) {
        return Err(Error::Pending);
    }
    let before = views::review(&tx, scope)?;
    let editor = disk(editor::matching_claim(&tx, claim.record()))?;
    let live = stream.is_live() && disk(record.stream_matches(&tx, stream))?;
    let exact =
        before.same(&disk(record.claimed())?) && disk(record.writer_matches(&tx))? && editor;
    let effect = if live && exact {
        Effect::ClearedExact
    } else {
        Effect::PreservedCurrent
    };
    if effect == Effect::ClearedExact {
        let row = disk(clear_row(&before))?;
        if !before.local().empty() {
            disk(write_local(&tx, &scope.key, &LocalDraft::default()))?;
        }
        disk(storage::write_row(&tx, &row))?;
        disk(editor::remove_claim(&tx, claim.record()))?;
    }
    let after = views::review(&tx, scope)?;
    let receipt = Receipt {
        version: 1,
        record,
        effect,
        before: disk(views::InventoryRecord::capture(&views::local_from_review(
            &before,
        )))?,
        after: disk(views::InventoryRecord::capture(&views::local_from_review(
            &after,
        )))?,
    };
    disk(receipt.validate())?;
    let raw = disk(serde_json::to_string(&receipt))?;
    if raw.len() > 8 * 1024 * 1024 {
        return Err(Error::Unsupported);
    }
    disk(tx.execute(
        "INSERT INTO composer_draft_hosted_completions VALUES(?1,?2,?3)",
        params![
            receipt.record.id(),
            disk(serde_json::to_string(&scope.key))?,
            raw
        ],
    ))?;
    if disk(tx.execute(
        "DELETE FROM composer_draft_hosted_records WHERE id=?1",
        [claim.record().id()],
    ))? != 1
    {
        return Err(Error::Stale);
    }
    disk(all(&tx, &scope.account_id))?;
    if effect == Effect::ClearedExact && !stream.is_live() {
        return Err(Error::Gap);
    }
    disk(faults::before_commit(faults::FaultPoint::MutationReceipt))?;
    disk(tx.commit())?;
    let fresh = (effect == Effect::ClearedExact).then(|| FreshClear {
        receipt: receipt.clone(),
    });
    Ok(Completed { receipt, fresh })
}
