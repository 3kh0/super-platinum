//! Actual bounded payload ownership, not a count-only idle assertion.
use super::super::source::inventory::Refused;
use super::*;

pub(crate) struct Snapshot {
    pub(crate) running: bool,
    pub(crate) failed: bool,
    pub(crate) unknown: bool,
    // These are the original accepted objects, not reconstructed identities.
    raw: Option<Arc<RawEvent>>,
    pending: Vec<Arc<Work>>,
    active: Option<Arc<Work>>,
    abandoned: Vec<Arc<Work>>,
    results: Vec<Arc<Completed>>,
    delivered: Vec<Arc<Completed>>,
}
impl ObservationQueue {
    pub(in super::super) fn inventory_known(&self) -> bool {
        !self.state.is_poisoned()
    }
    pub(in super::super) fn inventory(&self) -> Result<Snapshot, Refused> {
        let queue = self.state.lock().map_err(|_| Refused::Unknown)?;
        Ok(Snapshot {
            running: queue.running,
            failed: queue.failed,
            unknown: queue.unknown,
            raw: queue.raw.clone(),
            pending: queue.pending.iter().cloned().collect(),
            active: queue.active.clone(),
            abandoned: queue.abandoned.clone(),
            results: queue.results.iter().cloned().collect(),
            delivered: queue.delivered.clone(),
        })
    }
}
impl Snapshot {
    pub(crate) fn has_raw_frame(&self) -> bool {
        self.raw.is_some()
    }
    pub(crate) fn pending(&self) -> usize {
        self.pending.len() + usize::from(self.raw.is_some())
    }
    pub(crate) fn active(&self) -> bool {
        self.active.is_some()
    }
    pub(crate) fn abandoned(&self) -> usize {
        self.abandoned.len()
    }
    pub(crate) fn delivered(&self) -> usize {
        self.delivered.len()
    }
    pub(crate) fn results(&self) -> usize {
        self.results.len()
    }
}

/// Constructed before spawn: cancellation before the first poll is observed too.
pub(super) struct Running {
    pub(super) queue: Arc<ObservationQueue>,
    pub(super) finished: bool,
}
impl Drop for Running {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        match self.queue.state.lock() {
            Ok(mut queue) => {
                self.queue.source.inventory.clock.change();
                queue.unknown = true;
                queue.failed = true;
                queue.running = false;
                queue.abandon_pending();
                // active remains retained: a dropped waiter is not a receipt.
            }
            Err(_) => self.queue.source.inventory.clock.unknown(),
        }
        self.queue
            .source
            .gap(durable::GapReason::InterruptedAdmission);
    }
}

#[cfg(test)]
mod tests;
