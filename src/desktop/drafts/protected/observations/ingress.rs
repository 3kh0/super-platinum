//! One retained frame while the actual receiver awaits its scope lookup.
//! Accepted work enters the existing ordered ObservationQueue, not another executor.
use super::*;

impl Account {
    pub(crate) fn retire_closed_ingress(&self) {
        let queues = match self.ingress.lock() {
            Ok(q) => q.clone(),
            Err(_) => return,
        };
        for queue in queues {
            let _ = self.retire_ingress(&queue);
        }
    }

    pub(crate) fn open_ingress(
        self: &Arc<Self>,
        source: Arc<Source>,
    ) -> Result<Arc<ObservationQueue>, durable::Error> {
        let mut retained = self.ingress.lock().map_err(|_| durable::Error::Storage)?;
        if let Some(queue) = retained.iter().find(|q| Arc::ptr_eq(&q.source, &source)) {
            return Ok(queue.clone());
        }
        if retained.len() >= 64 {
            return Err(durable::Error::Pending);
        }
        let queue = ObservationQueue::new(self.clone(), source);
        retained.push(queue.clone());
        Ok(queue)
    }
    pub(crate) fn retire_ingress(
        &self,
        queue: &Arc<ObservationQueue>,
    ) -> Result<(), durable::Error> {
        let mut retained = self.ingress.lock().map_err(|_| durable::Error::Storage)?;
        let index = retained
            .iter()
            .position(|q| Arc::ptr_eq(q, queue))
            .ok_or(durable::Error::Stale)?;
        let state = queue.state.lock().map_err(|_| durable::Error::Storage)?;
        if queue.source.live()
            || state.running
            || state.failed
            || state.unknown
            || state.raw.is_some()
            || state.active.is_some()
            || !state.pending.is_empty()
            || !state.results.is_empty()
            || !state.delivered.is_empty()
            || !state.abandoned.is_empty()
        {
            return Err(durable::Error::Pending);
        }
        // Unhandled input/results deliberately keep the original queue/account.
        retained.remove(index);
        Ok(())
    }
}

fn key_for(
    loaded: &durable::Loaded,
    team: &str,
    raw: &RawEvent,
) -> Result<Option<DraftKey>, durable::Error> {
    let id = raw
        .rest
        .get("draft_id")
        .and_then(serde_json::Value::as_str)
        .ok_or(durable::Error::Unsupported)?;
    let client = raw
        .rest
        .get("client_msg_id")
        .and_then(serde_json::Value::as_str)
        .or_else(|| raw.rest.get("draft")?.get("client_msg_id")?.as_str());
    let mut keys = loaded.managed().iter().filter_map(|(key, review)| {
        let state = review.row().state();
        (key.team == team
            && (state.acknowledged().is_some_and(|r| r.id == id)
                || state.observed_event_ts(id).is_some()
                || state
                    .blocked_observation()
                    .is_some_and(|event| event.draft_id == id)
                || client.is_some_and(|id| state.client_msg_id() == id)))
        .then_some(key.clone())
    });
    let key = keys.next();
    if keys.next().is_some() {
        return Err(durable::Error::Stale);
    }
    Ok(key)
}
impl ObservationQueue {
    pub(crate) async fn enqueue_frame(
        self: &Arc<Self>,
        state: Signal<ShellState>,
        team: &str,
        raw: RawEvent,
    ) -> Result<(), durable::Error> {
        let raw = Arc::new(raw);
        {
            let mut queue = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if queue.raw.is_some() || queue.failed {
                return Err(durable::Error::Gap);
            }
            self.source.inventory.clock.change();
            queue.raw = Some(raw.clone());
        }
        let result = async {
            if !state.try_peek().is_ok_and(|s| self.source.current(&s)) {
                return Err(durable::Error::Gap);
            }
            let loaded = self.account.inventory().await?;
            let key = key_for(&loaded, team, &raw)?;
            let editor = {
                let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
                if !self.source.current(&shell) {
                    return Err(durable::Error::Gap);
                }
                match &key {
                    Some(key) => super::super::production::registry(&shell)?.get(
                        self.account.id(),
                        shell.core.account_epoch,
                        key,
                    ),
                    None => None,
                }
            };
            if let Some(key) = key {
                // Actual decoder validates source, full identity and shape again.
                // Unsupported data for a known owned row gaps, never overwrites.
                self.enqueue(state, key, editor, &raw)?;
            }
            // No managed match does not enroll or verify a new remote row. The
            // ordinary list/catalog reducer still sees the original socket event.
            let mut queue = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if !queue.raw.as_ref().is_some_and(|v| Arc::ptr_eq(v, &raw)) {
                return Err(durable::Error::Stale);
            }
            self.source.inventory.clock.change();
            queue.raw = None;
            Ok(())
        }
        .await;
        if result.is_err() {
            // Keep the original frame for review; no retry or silent eviction.
            self.source.gap(durable::GapReason::InterruptedAdmission);
        }
        result
    }
}

#[cfg(test)]
#[path = "ingress_tests.rs"]
mod tests;
