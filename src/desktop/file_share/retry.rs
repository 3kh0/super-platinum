//! Storage-only recovery of a result already received by the one Share write.
//! This module has no request builder, Begin, policy read or Transport operation.
use super::*;
use recovery::{Custody, Retained, Storage};
use std::sync::Mutex;
use super_platinum_core::file_sharing::State as ReceiptState;

pub(super) struct Settlement {
    storage: Storage,
    received_at: u64,
    visit: Option<Arc<drafts::Visit>>,
    identity: Arc<()>,
    generation: u64,
    in_flight: bool,
}
impl Settlement {
    pub fn new(storage: Storage, received_at: u64, visit: Option<Arc<drafts::Visit>>) -> Self {
        Self {
            storage,
            received_at,
            visit,
            identity: Arc::new(()),
            generation: 0,
            in_flight: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetryState {
    Ready,
    Saving,
}

fn same_attempt(a: &Receipt, b: &Receipt) -> bool {
    a.scope == b.scope
        && a.attempt_id == b.attempt_id
        && a.intent == b.intent
        && a.file_title == b.file_title
        && a.destination_name == b.destination_name
        && a.created_at == b.created_at
}
fn matches(pending: &recovery::Pending, key: &admission::Key, receipt: &Receipt) -> bool {
    pending.key == *key
        && receipt.scope.team == key.team
        && receipt.scope.user == key.user
        && pending.storage_failed
        && pending.outcome.is_some()
        && pending
            .receipt
            .as_ref()
            .is_some_and(|r| same_attempt(r, receipt))
        && pending
            .settlement
            .as_ref()
            .is_some_and(|s| s.storage.key == *key)
}
pub(crate) fn retry_status(shell: &ShellState, id: &str) -> Option<RetryState> {
    let dialog = visible(shell).filter(|d| {
        d.id == id && !d.closing && matches!(d.phase, Phase::StorageFailed | Phase::Outcome)
    })?;
    let receipt = dialog.receipt.as_ref()?;
    let context = admission::Context::capture(shell)?;
    let held = shell.file_share.custody.lock().ok()?;
    let pending = held
        .pending
        .values()
        .find(|p| matches(p, &context.key, receipt))?;
    let saved = pending.settlement.as_ref()?;
    if saved.in_flight {
        Some(RetryState::Saving)
    } else if saved.generation < u64::MAX {
        Some(RetryState::Ready)
    } else {
        None
    }
}

pub(super) struct Job {
    id: String,
    custody: Arc<Mutex<Custody>>,
    identity: Arc<()>,
    generation: u64,
    storage: Storage,
    receipt: Receipt,
    outcome: Retained,
    received_at: u64,
    visit: Option<Arc<drafts::Visit>>,
}
impl Job {
    pub(super) fn capture(shell: &ShellState, id: &str, attempt: &str) -> Result<Self, Refusal> {
        let dialog = visible(shell)
            .filter(|d| {
                d.id == id && !d.closing && matches!(d.phase, Phase::StorageFailed | Phase::Outcome)
            })
            .ok_or(Refusal::Changed)?;
        let receipt = dialog
            .receipt
            .as_ref()
            .filter(|r| r.attempt_id == attempt)
            .ok_or(Refusal::Changed)?;
        let context = admission::Context::capture(shell).ok_or(Refusal::Owner)?;
        let custody = shell.file_share.custody.clone();
        let mut held = custody.lock().map_err(|_| Refusal::Storage)?;
        let (pending_id, pending) = held
            .pending
            .iter_mut()
            .find(|(_, p)| matches(p, &context.key, receipt))
            .ok_or(Refusal::Changed)?;
        let saved = pending.settlement.as_mut().ok_or(Refusal::Changed)?;
        if saved.in_flight {
            return Err(Refusal::Busy);
        }
        let generation = saved.generation.checked_add(1).ok_or(Refusal::Capacity)?;
        let result = Self {
            id: pending_id.clone(),
            custody: custody.clone(),
            identity: saved.identity.clone(),
            generation,
            storage: saved.storage.clone(),
            receipt: pending.receipt.clone().ok_or(Refusal::Changed)?,
            outcome: pending.outcome.clone().ok_or(Refusal::Changed)?,
            received_at: saved.received_at,
            visit: saved.visit.clone(),
        };
        saved.generation = generation;
        saved.in_flight = true;
        Ok(result)
    }
    fn owns(&self, pending: &recovery::Pending) -> bool {
        pending.key == self.storage.key
            && pending.receipt.as_ref() == Some(&self.receipt)
            && pending.settlement.as_ref().is_some_and(|s| {
                s.in_flight
                    && s.generation == self.generation
                    && Arc::ptr_eq(&s.identity, &self.identity)
            })
    }
    /// Captured storage and evidence only. Account, dialog and original task may
    /// disappear while the transaction is running without dropping this result.
    pub(super) async fn run(&self) -> Result<Receipt, Refusal> {
        let expected = self.receipt.clone();
        let outcome = self.outcome.clone();
        let received_at = self.received_at;
        let saved = self
            .storage
            .run(move |journal| match outcome {
                Retained::Result(result) => journal.retry_result(&expected, result, received_at),
                Retained::Limited => journal.retry_response_limit(&expected, received_at),
            })
            .await?
            .ok_or(Refusal::Storage)?;
        if !same_attempt(&saved, &self.receipt) || !agrees(&self.outcome, &saved) {
            return Err(Refusal::Changed);
        }
        let mut held = self.custody.lock().map_err(|_| Refusal::Storage)?;
        if !held.pending.get(&self.id).is_some_and(|p| self.owns(p)) {
            return Err(Refusal::Changed);
        }
        if let Some(visit) = &self.visit {
            held.accepted_note(visit, &saved);
        }
        held.release(&self.id);
        Ok(saved)
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Ok(mut held) = self.custody.lock() {
            if let Some(pending) = held.pending.get_mut(&self.id) {
                if self.owns(pending) {
                    if let Some(saved) = &mut pending.settlement {
                        saved.in_flight = false;
                    }
                }
            }
        }
    }
}

fn agrees(outcome: &Retained, receipt: &Receipt) -> bool {
    match outcome {
        Retained::Result(Ok(raw)) => receipt.raw_response() == Some(raw),
        Retained::Result(Err(Error::RateLimited { retry_after_secs })) => {
            receipt.state()
                == ReceiptState::RateLimited {
                    retry_after_secs: *retry_after_secs,
                }
        }
        Retained::Result(Err(Error::Api(code))) if code == "draft_mutation_not_sent_offline" => {
            receipt.state()
                == ReceiptState::NotSent(super_platinum_core::file_sharing::NotSent::Offline)
        }
        Retained::Result(Err(_)) => receipt.state() == ReceiptState::Unconfirmed,
        Retained::Limited => {
            receipt.state()
                == ReceiptState::Server(api::file_share::FileShareDisposition::Uncertain(
                    api::file_share::FileShareUncertainty::ResponseLimit,
                ))
        }
    }
}

pub(crate) fn retry_result(mut signal: Signal<ShellState>, id: String, attempt: String) {
    let captured = {
        let mut shell = signal.write();
        match Job::capture(&shell, &id, &attempt) {
            Ok(job) => {
                if let Some(d) = shell.file_share.dialog.as_mut() {
                    d.error = None;
                }
                Some(job)
            }
            Err(_) => None, // A retained callback cannot affect replacement UI.
        }
    };
    let Some(job) = captured else { return };
    dioxus::core::spawn_forever(async move {
        let result = job.run().await;
        let key = job.storage.key.clone();
        let original = job.receipt.clone();
        // Clear a failed attempt's busy marker before any UI samples its state.
        drop(job);
        publish(&mut signal.write(), &key, &original, result);
    });
}
pub(super) fn publish(
    shell: &mut ShellState,
    key: &admission::Key,
    original: &Receipt,
    result: Result<Receipt, Refusal>,
) {
    if !admission::Context::capture(shell).is_some_and(|c| c.key == *key) {
        return;
    }
    // Updating an exact read-only receipt is safe even if Review opened it in a
    // new dialog while the save ran. Never replace an editing note or open UI.
    if let Some(dialog) = shell.file_share.dialog.as_mut().filter(|d| {
        matches!(d.phase, Phase::Outcome | Phase::StorageFailed)
            && d.receipt
                .as_ref()
                .is_some_and(|r| same_attempt(r, original))
    }) {
        match &result {
            Ok(saved) => {
                dialog.receipt = Some(saved.clone());
                dialog.phase = Phase::Outcome;
                dialog.error = None;
            }
            Err(_) => {
                dialog.error = Some("The result could not be saved. You can try saving it again.");
            }
        }
    }
    if let Ok(saved) = result {
        // Only this exact existing row changes. Newer recovery generations and
        // unrelated account/source records are not replaced or automatically read.
        shell.file_share.recovery.receipts.retain_mut(|r| {
            if !same_attempt(r, original) {
                return true;
            }
            if saved.unresolved() {
                *r = saved.clone();
                true
            } else {
                false
            }
        });
    }
}
