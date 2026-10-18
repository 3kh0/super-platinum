//! Successful single-step observation only; never discard a family or failure.
use super::*;
#[path = "writer_settlement.rs"]
mod writer_settlement;

fn simple(family: &str) -> bool {
    matches!(
        family,
        "hosted-draft-save"
            | "hosted-draft-load"
            | "slash-archive"
            | "protected-finish-baseline"
            | "protected-inventory"
            | "protected-local-inventory"
            | "protected-review"
            | "protected-checkpoint"
            | "protected-write-local"
            | "protected-renew-local"
            | "protected-enroll-local"
            | "protected-stream-state"
            | "protected-open-stream"
            | "protected-mark-gap"
            | "protected-verify-local"
            | "protected-register-writer"
    )
}
impl Worker {
    // Called only after the facade copied the exact successful result to the
    // consumer. Success has no retry/transfer path in these closed families.
    pub(in crate::drafts::store::worker) fn observe_simple_success<
        I: Send + Sync + 'static,
        T: Send + 'static,
    >(
        &self,
        step: &Step<I, T>,
    ) -> Result<(u64, terminal::Report), Reason> {
        let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        a.check(a.identity.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if !simple(entry.family)
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
            || s.next.is_some()
            || s.reserved.is_some()
            || !s.tail.is_empty()
            || !s.failures.is_empty()
        {
            return Err(Reason::Incomplete);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        let report = pending
            .accounting
            .as_ref()
            .and_then(|b| b.terminal(id))
            .ok_or(Reason::Incomplete)?;
        if !matches!(
            report.execution,
            terminal::Execution::ReadObserved | terminal::Execution::StorageAccepted
        ) || report.inventory != terminal::Inventory::NotAssessed
        {
            return Err(Reason::Incomplete);
        }
        Ok((id, report))
    }
    pub(in crate::drafts::store::worker) fn acknowledge_simple_success<
        I: Send + Sync + 'static,
        T: Send + 'static,
    >(
        &self,
        step: &Step<I, T>,
        id: u64,
        report: terminal::Report,
    ) -> Result<(), Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        // A fence may have captured this exact slot/accounting inventory. Do
        // not change it under that certificate, even for an observed success.
        a.check(a.identity.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if !simple(entry.family)
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
            || s.submitted != Some(id)
            || s.next.is_some()
            || s.reserved.is_some()
            || !s.tail.is_empty()
            || !s.failures.is_empty()
        {
            return Err(Reason::Incomplete);
        }
        if !pending
            .accounting
            .as_mut()
            .ok_or(Reason::Incomplete)?
            .acknowledge_success(id, report)
        {
            return Err(Reason::Incomplete);
        }
        // Pending -> Held is the existing lock order. Both removals happen
        // under Pending; neither old handles nor a competing submit can replay.
        pending
            .admission
            .as_mut()
            .unwrap()
            .slots
            .remove(&step.key.id);
        Ok(())
    }
}
