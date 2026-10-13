//! Explicit bounded readback of a frozen send, never a second write attempt.
use super::*;
mod return_draft;
mod ui;
use super::{
    dispatch::Outcome,
    network::{Binding, Readback},
};
pub(crate) use return_draft::{rejected, return_to_draft};
use super_platinum_core::drafts::sync::{OperationKind, Workflow};
pub(crate) use ui::{check, continue_locally};

pub(crate) fn clean_local(view: &View) -> bool {
    view.blocked.is_none()
        && view.uncommitted.is_none()
        && view.stored.as_ref().is_some_and(|stored| {
            matches!(stored.state.workflow(), Workflow::Idle)
                && stored.state.acknowledged().is_none()
                && stored.state.consumption().is_none()
        })
}

pub(crate) fn operation(view: &View) -> Option<Operation> {
    match view.stored.as_ref()?.state.workflow() {
        Workflow::Uncertain(op) if op.kind == OperationKind::Send => Some(op.clone()),
        _ => None,
    }
}

// A failed final Retry-After save must not be bypassed by another view or a new
// Binding. Retry this disk write before any later HTTP check. Across restart the
// pre-request durable spacing still applies; a read check never resends content.
type BarrierKey = (String, DraftKey, String);
fn barriers() -> &'static Mutex<BTreeMap<BarrierKey, u64>> {
    static BARRIERS: OnceLock<Mutex<BTreeMap<BarrierKey, u64>>> = OnceLock::new();
    BARRIERS.get_or_init(Mutex::default)
}
fn barrier_key(op: &Operation) -> BarrierKey {
    (
        op.scope.account_id.clone(),
        op.scope.key.clone(),
        op.id.clone(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckError {
    Fault(Fault),
    Cooldown(u64),
}
impl From<Fault> for CheckError {
    fn from(value: Fault) -> Self {
        Self::Fault(value)
    }
}

impl Owner {
    /// Call after recording Uncertain. On failure a shared barrier remembers the
    /// deadline and later explicit checks must successfully persist it first.
    pub async fn defer_check(&self, op: &Operation, until_ms: u64) -> Result<Stored, Fault> {
        if op.scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        let _guard = self.gate.lock().await;
        self.defer_locked(op, until_ms).await
    }
    async fn defer_locked(&self, op: &Operation, until_ms: u64) -> Result<Stored, Fault> {
        let until_ms = {
            let mut barriers = barriers().lock().map_err(|_| Fault::Storage)?;
            let until = barriers.entry(barrier_key(op)).or_default();
            *until = (*until).max(until_ms);
            *until
        };
        for _ in 0..4 {
            let view = self.current().await?;
            let mut stored = view.stored.ok_or(Fault::Unmanaged)?;
            if !stored.state.defer_check(op, until_ms) {
                return Err(Fault::Stale);
            }
            match self
                .save(Some(stored.revision), view.local, stored.state)
                .await
            {
                Ok(stored) => {
                    barriers()
                        .lock()
                        .map_err(|_| Fault::Storage)?
                        .remove(&barrier_key(op));
                    return Ok(stored);
                }
                Err(Fault::Stale) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Fault::Stale)
    }
    /// Atomic cross-view reservation before requesting history. Reopening cannot
    /// bypass a persisted deadline; the state remains Uncertain throughout.
    pub async fn reserve_check(&self, op: &Operation, now_ms: u64) -> Result<(), CheckError> {
        if op.scope != self.scope {
            return Err(Fault::InvalidReceipt.into());
        }
        let _guard = self.gate.lock().await;
        let remembered = barriers()
            .lock()
            .map_err(|_| Fault::Storage)?
            .get(&barrier_key(op))
            .copied();
        if let Some(until) = remembered {
            self.defer_locked(op, until).await?;
        }
        let view = self.current().await?;
        let stored = view.stored.ok_or(Fault::Unmanaged)?;
        let until = stored.state.check_retry_at_ms(op).ok_or(Fault::Stale)?;
        if until > now_ms {
            return Err(CheckError::Cooldown(until));
        }
        self.defer_locked(op, now_ms.saturating_add(30_000)).await?;
        Ok(())
    }
    /// Run in a task that outlives the view. Captured account persistence always
    /// precedes caller UI checks; a positive read is the only resolving outcome.
    pub async fn finish_check(
        &self,
        op: Operation,
        binding: Binding,
        now_ms: u64,
    ) -> Result<Checked, Fault> {
        let read = binding.check_send(&op, now_ms).await;
        if let Some(until) = read.cooldown_until_ms {
            self.defer_check(&op, until).await?;
        }
        let confirmed = if let Some(outcome) = read.outcome.clone() {
            self.record(op.clone(), outcome).await?.applied
        } else {
            false
        };
        Ok(Checked {
            view: self.load().await?,
            read,
            confirmed,
        })
    }
}
pub(crate) struct Checked {
    pub view: View,
    pub read: Readback,
    pub confirmed: bool,
}

#[cfg(test)]
mod tests;
