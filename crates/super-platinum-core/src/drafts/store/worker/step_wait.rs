//! Receiver custody is separate from cache access; no Held/Pending across recv.
use super::*;

impl<I, T> Step<I, T> {
    fn wait_reply(&self) -> Result<(StepState, Option<u64>), Reason> {
        let mut s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
        loop {
            let state = s.poll();
            if state != StepState::Waiting || s.submitted.is_none() {
                return Ok((state, s.submitted));
            }
            if let Some(receiver) = s.receiver.take() {
                drop(s);
                let result = receiver.recv();
                s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
                match result {
                    Ok(result) => s.result = Some(result),
                    Err(_) => s.disconnected = true,
                }
                self.held.changed.notify_all();
            } else {
                // Another retained waiter owns the receiver. Poll sees Waiting,
                // never Disconnected merely because the receiver was moved.
                s = self.held.changed.wait(s).map_err(|_| Reason::Incomplete)?;
            }
        }
    }
}
impl Worker {
    pub(in crate::drafts::store::worker) fn wait_step<
        I: Send + Sync + 'static,
        T: Send + 'static,
    >(
        &self,
        step: &Step<I, T>,
    ) -> Result<StepState, Reason> {
        // Lookup verifies actual route/holder identity before blocking. No lock
        // is retained while awaiting reply delivery or the worker terminal.
        let actual = self.reattach_step::<I, T>(&step.key)?;
        if !Arc::ptr_eq(&actual.held, &step.held) {
            return Err(Reason::Stale);
        }
        loop {
            let (state, id) = step.wait_reply()?;
            if !matches!(state, StepState::Ready | StepState::Failed(_)) {
                return Ok(state);
            }
            let id = id.ok_or(Reason::Incomplete)?;
            self.wait_accounted(&step.key, id)?;
            let mut s = step.held.status.lock().map_err(|_| Reason::Incomplete)?;
            if s.submitted == Some(id) {
                return Ok(s.poll());
            }
            // A different owner explicitly retried while we awaited accounting.
            // Wait for the new actual attempt, never return a stale Ready state.
        }
    }
}

#[cfg(test)]
#[path = "retained_wait_tests.rs"]
mod tests;
