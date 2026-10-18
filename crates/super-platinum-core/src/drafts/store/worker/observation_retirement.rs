//! Exact successful observation family, after consumer projection handling.
use super::family_retirement::{finish, keys, transferred};
use super::*;

impl Worker {
    pub(in crate::drafts::store::worker) fn observe_observation_family(
        &self,
        c: &Completion,
        expected: Option<&Arc<Finished>>,
        retire: bool,
    ) -> Result<Arc<Finished>, Reason> {
        // Poll before Pending. No receiver wait occurs under either book lock.
        c.result()?;
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let keys = keys(
            &pending,
            &c.key,
            &[
                "protected-reserve-observation",
                "protected-finish-observation",
            ],
        )?;
        let actual = pending
            .admission
            .as_ref()
            .unwrap()
            .entry(&c.key)?
            .retained
            .clone()
            .downcast::<CompletionHeld>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &c.held) {
            return Err(Reason::Stale);
        }
        let s = actual.status.lock().map_err(|_| Reason::Incomplete)?;
        let Some(Finished::Observation(Ok(result))) = s.result.as_deref() else {
            return Err(Reason::Incomplete);
        };
        if !matches!(s.raw.as_deref(), Some(Raw::Observation))
            || !matches!(s.binding.as_deref(), Some(Binding::Observation(_)))
            || result.after().row().state().pending().is_some()
            || s.handed
            || s.building
            || s.disconnected
            || s.issued.is_some()
            || s.dispatch.is_some()
            || s.request.is_some()
            || s.attempts.len() != 1
        {
            return Err(Reason::Incomplete);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        if s.attempts[0].0 != id || !Arc::ptr_eq(&s.attempts[0].1, s.result.as_ref().unwrap()) {
            return Err(Reason::Incomplete);
        }
        let ids = [
            transferred::<ObservationInput, p::ObservationTicket>(&pending, &keys[0], &keys[1])?,
            id,
        ];
        let observed = s.result.as_ref().unwrap().clone();
        if retire && expected.is_none_or(|value| !Arc::ptr_eq(value, &observed)) {
            return Err(Reason::Stale);
        }
        finish(&mut pending, &keys, &ids, retire)?;
        Ok(observed)
    }
}
