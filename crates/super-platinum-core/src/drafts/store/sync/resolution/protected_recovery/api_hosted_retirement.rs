//! Explicit local retirement of an exactly reviewed accepted receipt. No text clear.
use super::*;
#[derive(Clone)]
pub struct Retirement {
    receipt: Receipt,
    current: Option<editor::Draft>,
}
impl Retirement {
    pub fn new(receipt: Receipt, current: Option<editor::Draft>) -> Result<Self, Error> {
        disk(receipt.validate())?;
        if current
            .as_ref()
            .is_some_and(|d| d.scope() != receipt.record().scope())
        {
            return Err(Error::Stale);
        }
        Ok(Self { receipt, current })
    }
    pub fn receipt(&self) -> &Receipt {
        &self.receipt
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Removed,
    AlreadyAbsent,
    Preserved,
}
#[derive(Clone)]
pub struct Retired {
    disposition: Disposition,
    remaining: Option<editor::Draft>,
    claims: Vec<Record>,
    completed: Vec<Receipt>,
}
impl Retired {
    pub fn claims(&self) -> &[Record] {
        &self.claims
    }
    pub fn completed(&self) -> &[Receipt] {
        &self.completed
    }
    pub fn disposition(&self) -> Disposition {
        self.disposition
    }
    pub fn remaining(&self) -> Option<&editor::Draft> {
        self.remaining.as_ref()
    }
}
/// Absence is only a truthful no-op, never accepted-delivery or clear proof.
/// The existing exact receipt and row are both rechecked in one transaction.
pub(crate) fn retire(store: &DraftStore, input: &Retirement) -> Result<Retired, Error> {
    if !store.0.is_autocommit() {
        return Err(Error::Storage);
    }
    disk(input.receipt.validate())?;
    let scope = input.receipt.record().scope();
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let history = disk(all(&tx, &scope.account_id))?;
    let actual = history
        .iter()
        .find(|r| r.record().id() == input.receipt.record().id());
    let drafts = disk(editor::all(&tx, &scope.account_id))?;
    let current = drafts.iter().find(|d| d.scope() == scope);
    let claims: Vec<_> = disk(journal::all(&tx, &scope.account_id))?
        .into_iter()
        .filter(|r| r.scope() == scope)
        .collect();
    let same_current =
        disk(serde_json::to_value(current))? == disk(serde_json::to_value(&input.current))?;
    let matches_original = disk(editor::matching_claim(&tx, input.receipt.record()))?;
    let Some(actual) = actual else {
        if matches_original {
            return Err(Error::ReviewRequired);
        }
        // Missing history conveys no accepted-delivery or clear authority. The
        // explicit removal is a no-op; actual newer/uncertain content is returned.
        return Ok(Retired {
            disposition: Disposition::AlreadyAbsent,
            remaining: current.cloned(),
            claims,
            completed: history
                .into_iter()
                .filter(|r| r.record().scope() == scope)
                .collect(),
        });
    };
    if disk(serde_json::to_value(actual))? != disk(serde_json::to_value(&input.receipt))? {
        return Err(Error::Stale);
    }
    if matches_original && (!same_current || !claims.is_empty()) {
        // Keep the association while old attachments remain: a stale removal
        // must not turn a sent set back into new input.
        return Ok(Retired {
            disposition: Disposition::Preserved,
            remaining: current.cloned(),
            claims,
            completed: history
                .into_iter()
                .filter(|r| r.record().scope() == scope)
                .collect(),
        });
    }
    let remove = matches_original;
    if remove {
        disk(editor::remove_claim(&tx, actual.record()))?;
    }
    let raw = disk(serde_json::to_string(actual))?;
    if disk(tx.execute(
        "DELETE FROM composer_draft_hosted_completions WHERE id=?1 AND content=?2",
        params![actual.record().id(), raw],
    ))? != 1
    {
        return Err(Error::Stale);
    }
    let completed = disk(all(&tx, &scope.account_id))?
        .into_iter()
        .filter(|r| r.record().scope() == scope)
        .collect();
    disk(editor::all(&tx, &scope.account_id))?;
    disk(faults::before_commit(faults::FaultPoint::MutationReceipt))?;
    disk(tx.commit())?;
    Ok(Retired {
        disposition: Disposition::Removed,
        remaining: if remove { None } else { current.cloned() },
        claims,
        completed,
    })
}
