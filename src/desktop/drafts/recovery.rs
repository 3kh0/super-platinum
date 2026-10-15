//! Explicit bounded readback of a frozen Send, Update or known-ID Schedule, never another write.
use super::*;
mod kind;
mod return_draft;
pub(crate) use kind::CheckKind;
pub(crate) mod schedule_check;
pub(crate) mod schedule_fixture;
mod ui;
pub(crate) use ui::reconcile as protected_reconcile;
pub(crate) mod update_check;
pub(crate) mod update_fixture;
use super::{
    dispatch::Outcome,
    network::{Binding, Readback},
};
pub(crate) use return_draft::{rejected, return_to_draft};
use super_platinum_core::drafts::sync::{CheckBlock, CheckTicket, OperationKind, Workflow};
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
        Workflow::Uncertain(op) if CheckKind::for_operation(op).is_some() => Some(op.clone()),
        _ => None,
    }
}

// A failed final Retry-After save must not be bypassed by another view or a new
// Binding. Retry this disk write before any later HTTP check. An admitted read
// also retains its unresolved ticket across restart; no check resends content.
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
    /// A read was admitted durably but its completion has not been saved.
    Pending,
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
    pub async fn reserve_check(
        &self,
        op: &Operation,
        now_ms: u64,
    ) -> Result<CheckTicket, CheckError> {
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
        for _ in 0..4 {
            let view = self.current().await?;
            let mut stored = view.stored.ok_or(Fault::Unmanaged)?;
            let ticket = stored
                .state
                .reserve_check(op, now_ms)
                .map_err(|block| match block {
                    CheckBlock::Ineligible => CheckError::Fault(Fault::Stale),
                    CheckBlock::Pending => CheckError::Pending,
                    CheckBlock::Cooldown(until) => CheckError::Cooldown(until),
                })?;
            match self
                .save(Some(stored.revision), view.local, stored.state)
                .await
            {
                Ok(_) => {
                    issued()
                        .lock()
                        .map_err(|_| Fault::Storage)?
                        .insert(ticket.id().into(), ticket.clone());
                    return Ok(ticket);
                }
                Err(Fault::Stale) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Fault::Stale.into())
    }
    /// Run in a task that outlives the view. Captured account persistence always
    /// precedes caller UI checks; a positive read is the only resolving outcome.
    pub async fn finish_check(
        &self,
        op: Operation,
        binding: Binding,
        ticket: CheckTicket,
        now_ms: u64,
    ) -> Result<Checked, Fault> {
        self.finish_check_with(op.clone(), ticket, || binding.check_operation(&op, now_ms))
            .await
    }
    pub(crate) async fn finish_check_with<F, Fut>(
        &self,
        op: Operation,
        ticket: CheckTicket,
        read: F,
    ) -> Result<Checked, Fault>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Readback>,
    {
        if ticket.operation() != &op || op.scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        // A loaded ticket proves an unfinished read, never permission to repeat
        // it. Only this process's newly persisted reservation can dispatch once.
        if issued()
            .lock()
            .map_err(|_| Fault::Storage)?
            .remove(ticket.id())
            .as_ref()
            != Some(&ticket)
        {
            return Err(Fault::Pending);
        }
        {
            let _guard = self.gate.lock().await;
            let view = self.current().await?;
            if view
                .stored
                .as_ref()
                .and_then(|s| s.state.pending_check(&op))
                != Some(&ticket)
            {
                return Err(Fault::Stale);
            }
        }
        let read = read().await;
        self.complete_check(ticket, read).await
    }
}
#[derive(Clone)]
struct Completion {
    ticket: CheckTicket,
    read: Readback,
}
fn issued() -> &'static Mutex<BTreeMap<String, CheckTicket>> {
    static ISSUED: OnceLock<Mutex<BTreeMap<String, CheckTicket>>> = OnceLock::new();
    ISSUED.get_or_init(Mutex::default)
}
fn completions() -> &'static Mutex<BTreeMap<BarrierKey, Completion>> {
    static COMPLETIONS: OnceLock<Mutex<BTreeMap<BarrierKey, Completion>>> = OnceLock::new();
    COMPLETIONS.get_or_init(Mutex::default)
}
impl Owner {
    /// A later explicit click can retry storage only. It never repeats the read.
    pub async fn retry_check_commit(&self, op: &Operation) -> Result<Option<Checked>, Fault> {
        if op.scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        let completion = completions()
            .lock()
            .map_err(|_| Fault::Storage)?
            .get(&barrier_key(op))
            .cloned();
        let Some(completion) = completion.filter(|c| c.ticket.operation() == op) else {
            return Ok(None);
        };
        self.commit_check(completion).await.map(Some)
    }
    pub(super) async fn complete_check(
        &self,
        ticket: CheckTicket,
        read: Readback,
    ) -> Result<Checked, Fault> {
        if ticket.operation().scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        issued()
            .lock()
            .map_err(|_| Fault::Storage)?
            .remove(ticket.id());
        let completion = Completion { ticket, read };
        completions().lock().map_err(|_| Fault::Storage)?.insert(
            barrier_key(completion.ticket.operation()),
            completion.clone(),
        );
        self.commit_check(completion).await
    }
    async fn commit_check(&self, completion: Completion) -> Result<Checked, Fault> {
        let op = completion.ticket.operation();
        update_fixture::commit_checkpoint(&self.scope).await;
        schedule_fixture::checkpoint(&self.scope, "commit").await;
        let _guard = self.gate.lock().await;
        for _ in 0..4 {
            let view = self.current().await?;
            let mut stored = view.stored.ok_or(Fault::Unmanaged)?;
            if !stored.state.finish_check(
                &completion.ticket,
                completion.read.cooldown_until_ms.unwrap_or(0),
            ) {
                return Err(Fault::Stale);
            }
            let confirmed = completion
                .read
                .outcome
                .as_ref()
                .is_some_and(|outcome| super::dispatch::apply(&mut stored.state, op, outcome));
            let local = view.local;
            match self
                .save(Some(stored.revision), local.clone(), stored.state)
                .await
            {
                Ok(stored) => {
                    let key = barrier_key(op);
                    let mut saved = completions().lock().map_err(|_| Fault::Storage)?;
                    if saved
                        .get(&key)
                        .is_some_and(|c| c.ticket == completion.ticket)
                    {
                        saved.remove(&key);
                    }
                    return Ok(Checked {
                        view: View {
                            local,
                            stored: Some(stored),
                            blocked: view.blocked,
                            uncommitted: view.uncommitted,
                        },
                        read: completion.read,
                        confirmed,
                    });
                }
                Err(Fault::Stale) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Fault::Stale)
    }
}
pub(crate) struct Checked {
    pub view: View,
    pub read: Readback,
    pub confirmed: bool,
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod update_tests;

#[cfg(test)]
mod schedule_tests;
