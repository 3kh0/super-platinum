//! Ordered handoff from managed ownership to ordinary local persistence.
use super::*;

impl Editor {
    pub(super) fn enqueue_released(&self, state: &mut State, local: LocalDraft) {
        let receipt = self.owner.account.backend.queue_local(
            self.owner.scope.key.clone(),
            state.released_tail.clone(),
            local.clone(),
            state.local_writer.clone(),
            state.serial,
        );
        state.released_tail = local.clone();
        state.local_writes.push_back((state.serial, local, receipt));
    }

    pub(super) async fn run_released(&self) {
        loop {
            let next = {
                let mut state = self.state.lock().unwrap();
                let next = state.local_writes.pop_front();
                if next.is_none() {
                    state.running = false;
                }
                next
            };
            let Some((serial, local, receipt)) = next else {
                return;
            };
            // Submission happened synchronously at input time. This task only
            // observes receipts, so dropping a Queue cannot reorder writes.
            let result = receipt.finish().await;
            let retained = if result.is_err() {
                self.owner.load().await.ok().and_then(|view| view.blocked)
            } else {
                None
            };
            let mut state = self.state.lock().unwrap();
            match result {
                Ok(()) => {
                    state.base.local = local;
                    state.error = None;
                    if state.pending.as_ref().is_some_and(|(id, _)| *id == serial) {
                        state.pending = None;
                    }
                }
                Err(error) => {
                    state.error = Some(error);
                    if retained.is_some() {
                        state.base.blocked = retained;
                    }
                }
            }
        }
    }

    /// Return ownership only after preceding input is durable. The detached
    /// handoff continues if its UI waiter disappears during SQLite I/O.
    pub async fn release(self: &Arc<Self>) -> Result<u64, Fault> {
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.release_inner().await })
            .await
            .map_err(|_| Fault::Storage)?
    }

    async fn release_inner(self: Arc<Self>) -> Result<u64, Fault> {
        loop {
            self.flush().await?;
            let guard = self.gate.lock().await;
            if self.state.lock().unwrap().running {
                drop(guard);
                continue;
            }
            let current = self.rebased().await?;
            let revision = current.stored.as_ref().unwrap().revision;
            let local = self.owner.release(revision).await?;
            {
                let mut state = self.state.lock().unwrap();
                state.base = View {
                    local: local.clone(),
                    stored: None,
                    blocked: None,
                    uncommitted: None,
                };
                state.released = true;
                state.return_handoff = None;
                state.released_tail = local;
                // submit() may have run during release I/O. Its detached run
                // is waiting on our gate; transfer that input before opening it.
                if let Some((_, pending)) = state.pending.clone() {
                    self.enqueue_released(&mut state, pending);
                }
            }
            drop(guard);
            self.changed.notify_waiters();
            return Ok(revision);
        }
    }
}
