//! Explicit review commits serialize with typing and outlive their foreground UI.
use super::*;
use super_platinum_core::drafts::store::sync::{LocalChoice, LocalReview};

impl Editor {
    pub async fn settle(
        self: &Arc<Self>,
        review: LocalReview,
        choice: LocalChoice,
        expected_serial: u64,
    ) -> Result<View, Fault> {
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.settle_inner(review, choice, expected_serial).await })
            .await
            .map_err(|_| Fault::Storage)?
    }

    async fn settle_inner(
        &self,
        review: LocalReview,
        choice: LocalChoice,
        expected_serial: u64,
    ) -> Result<View, Fault> {
        let _guard = self.gate.lock().await;
        {
            let mut state = self.state.lock().unwrap();
            if review.scope != self.owner.scope
                || state.serial != expected_serial
                || state.running
                || state.settling
                || state.projection.is_some()
                || !state.local_writes.is_empty()
                || state
                    .pending
                    .as_ref()
                    .is_some_and(|(_, local)| !review.contains_local(local))
            {
                return Err(Fault::Stale);
            }
            // Released input normally queues synchronously. While this explicit
            // decision is on disk its predecessor is not known; retain newer
            // input in memory and enqueue it immediately after the receipt.
            state.settling = true;
        }
        let result = self.owner.settle_local(review, choice).await;
        {
            let mut state = self.state.lock().unwrap();
            state.settling = false;
            if let Ok(view) = &result {
                state.base = view.clone();
                state.error = None;
                state.return_handoff = None;
                // Reviewed writer identities stay retired on disk. Only this
                // serialized handoff can start a fresh writer on the chosen base.
                state.local_writer = Default::default();
                state.released = view.stored.is_none();
                state.released_tail = view.local.clone();
                // Exact persisted completion provenance can retain local-only
                // capability after consumption ends; checkpoint validates it.
                if let Some(intent) = view.stored.as_ref().and_then(local::direct_intent) {
                    state.direct_local = Some(intent);
                }
                if state.serial == expected_serial {
                    state.pending = None;
                }
            }
            if state.released
                && state.serial != expected_serial
                && let Some((_, local)) = state.pending.clone()
            {
                self.enqueue_released(&mut state, local);
            }
        }
        // A submit during the transaction already owns a run task waiting on
        // this gate. It now sees the settled base (or the unchanged failed base).
        self.changed.notify_waiters();
        result
    }
}

#[cfg(test)]
#[path = "checkpoint_settlement_tests.rs"]
mod tests;
