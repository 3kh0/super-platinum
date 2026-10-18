//! Explicit observation of closed baseline terminal history only.
use super::*;

impl Worker {
    pub(in crate::drafts::store::worker) fn observe_baseline_cooldown(
        &self,
        step: &ReservedBaseline,
        expected: Option<(u64, u64)>,
        retire: bool,
    ) -> Result<(u64, u64), Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        // Observation retires historical memory, never admits new work. A canceled
        // fence advances admission generation while retaining these exact slots.
        a.check(step.key.route.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if entry.family != "protected-reserve-baseline" || entry.parent.is_some() {
            return Err(Reason::Stale);
        }
        let actual = entry
            .retained
            .clone()
            .downcast::<StepHeld<BaselineInput, p::BaselineTicket>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &step.held) {
            return Err(Reason::Stale);
        }
        let mut s = actual.status.lock().map_err(|_| Reason::Incomplete)?;
        let StepState::Failed(p::Error::Cooldown(until)) = s.poll() else {
            return Err(Reason::Incomplete);
        };
        if s.next.is_some()
            || s.request.is_some()
            || s.disconnected
            || !s.failures.is_empty()
            || s.tail.len() != 1
        {
            return Err(Reason::Incomplete);
        }
        let claim = s.reserved.as_ref().ok_or(Reason::Incomplete)?;
        let retain = &s.tail[0];
        if a.slots
            .values()
            .filter(|e| e.parent == Some(step.key.id))
            .count()
            != 2
        {
            return Err(Reason::Incomplete);
        }
        // These are never-materialized placeholders reserved atomically before
        // the failed Reserve, not issued claim/completion work.
        for (key, family) in [
            (claim, "protected-claim-baseline"),
            (retain, "protected-retain-baseline"),
        ] {
            let e = a.entry(key)?;
            if e.parent != Some(step.key.id)
                || e.family != family
                || e.generation != entry.generation
                || !e.retained.is::<ReservedSlot>()
            {
                return Err(Reason::Stale);
            }
        }
        if claim.id == retain.id || until <= actual.input.now_ms {
            return Err(Reason::Incomplete);
        }
        let observed = (s.submitted.ok_or(Reason::Incomplete)?, until);
        if (retire && expected.is_none()) || expected.is_some_and(|v| v != observed) {
            return Err(Reason::Stale);
        }
        if !pending
            .accounting
            .as_mut()
            .ok_or(Reason::Incomplete)?
            .baseline_cooldown(observed.0, retire)
        {
            return Err(Reason::Incomplete);
        }
        if retire {
            let a = pending.admission.as_mut().unwrap();
            for id in [step.key.id, claim.id, retain.id] {
                a.slots.remove(&id);
            }
        }
        Ok(observed)
    }
    pub(in crate::drafts::store::worker) fn observe_baseline_finish_repair(
        &self,
        step: &Step<BaselineFinishInput, p::Review>,
        expected: Option<(&p::Review, &[(u64, terminal::Report)])>,
        retire: bool,
    ) -> Result<(p::Review, Vec<(u64, terminal::Report)>), Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        // Observation retires historical memory, never admits new work. A canceled
        // fence advances admission generation while retaining these exact slots.
        a.check(step.key.route.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if entry.family != "protected-finish-baseline"
            || entry.parent.is_some()
            || a.slots.values().any(|e| e.parent == Some(step.key.id))
        {
            return Err(Reason::Unsupported);
        }
        let actual = entry
            .retained
            .clone()
            .downcast::<StepHeld<BaselineFinishInput, p::Review>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &step.held) {
            return Err(Reason::Stale);
        }
        let mut s = actual.status.lock().map_err(|_| Reason::Incomplete)?;
        if s.poll() != StepState::Ready
            || s.failures.is_empty()
            || s.failures.iter().any(|(_, e)| *e != p::Error::Storage)
            || s.next.is_some()
            || s.reserved.is_some()
            || !s.tail.is_empty()
            || s.request.is_some()
            || s.disconnected
        {
            return Err(Reason::Incomplete);
        }
        let Some(Ok(value)) = &s.result else {
            return Err(Reason::Incomplete);
        };
        let observed = value.clone();
        let mut reports: Vec<_> = s
            .failures
            .iter()
            .map(|(id, _)| {
                (
                    *id,
                    terminal::Report::new(terminal::Execution::StorageUnresolved),
                )
            })
            .collect();
        reports.push((
            s.submitted.ok_or(Reason::Incomplete)?,
            terminal::Report::new(terminal::Execution::StorageAccepted),
        ));
        if (retire && expected.is_none())
            || expected.is_some_and(|(v, r)| !v.same(&observed) || r != reports.as_slice())
        {
            return Err(Reason::Stale);
        }
        if !pending
            .accounting
            .as_mut()
            .ok_or(Reason::Incomplete)?
            .baseline_finish_repair(&reports, retire)
        {
            return Err(Reason::Incomplete);
        }
        if retire {
            pending
                .admission
                .as_mut()
                .unwrap()
                .slots
                .remove(&step.key.id);
        }
        Ok((observed, reports))
    }
}
