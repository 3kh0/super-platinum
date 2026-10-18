//! Closed writer-slot retirement. Never clears another pending operation.
use super::*;
impl Worker {
    pub(in crate::drafts::store::worker) fn observe_writer_settlement<
        I: Send + Sync + 'static,
        T: Send + 'static,
    >(
        &self,
        step: &Step<I, T>,
        family: &'static str,
        final_report: terminal::Report,
        expected: Option<&[(u64, terminal::Report)]>,
        retire: bool,
    ) -> Result<Vec<(u64, terminal::Report)>, Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        a.check(step.key.route.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if !matches!(family, "protected-checkpoint" | "protected-write-local")
            || entry.family != family
            || entry.parent.is_some()
            || a.slots.values().any(|e| e.parent == Some(step.key.id))
        {
            return Err(Reason::Unsupported);
        }
        let held = entry
            .retained
            .clone()
            .downcast::<StepHeld<I, T>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&held, &step.held) {
            return Err(Reason::Stale);
        }
        let mut s = held.status.lock().map_err(|_| Reason::Incomplete)?;
        if s.poll() != StepState::Ready
            || s.failures.iter().any(|(_, e)| *e != p::Error::Storage)
            || s.next.is_some()
            || s.reserved.is_some()
            || !s.tail.is_empty()
            || final_report.execution != terminal::Execution::StorageAccepted
            || !matches!(
                final_report.inventory,
                terminal::Inventory::NotAssessed | terminal::Inventory::OperationPending
            )
            || (final_report.inventory == terminal::Inventory::OperationPending
                && family != "protected-checkpoint")
            || (s.failures.is_empty() && final_report.inventory == terminal::Inventory::NotAssessed)
        {
            return Err(Reason::Incomplete);
        }
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
        reports.push((s.submitted.ok_or(Reason::Incomplete)?, final_report));
        if (retire && expected.is_none()) || expected.is_some_and(|seen| seen != reports.as_slice())
        {
            return Err(Reason::Stale);
        }
        if !pending
            .accounting
            .as_mut()
            .ok_or(Reason::Incomplete)?
            .writer_settlement(&reports, retire)
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
        Ok(reports)
    }
}
