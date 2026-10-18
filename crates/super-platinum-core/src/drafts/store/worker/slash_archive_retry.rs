//! Explicit local archive retry and separately observed repair retirement.
use super::*;
impl Worker {
    pub(in crate::drafts::store::worker) fn retry_slash_archive(
        &self,
        step: &ArchiveStep,
    ) -> Result<(), Reason> {
        let input = step.input().as_ref().clone();
        let (reply, receiver) = mpsc::channel();
        let request = Request::SlashProtected(Box::new(SlashCommand::Archive { input, reply }));
        let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        a.check(step.key.route.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if entry.family != "slash-archive"
            || entry.parent.is_some()
            || a.slots.values().any(|e| e.parent == Some(step.key.id))
        {
            return Err(Reason::Unsupported);
        }
        let held = entry
            .retained
            .clone()
            .downcast::<StepHeld<Arc<p::slash_runtime::ArchiveInput>, p::slash_runtime::Archived>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&held, &step.held) || entry.generation != a.generation {
            return Err(Reason::Stale);
        }
        let mut s = held.status.lock().map_err(|_| Reason::Incomplete)?;
        if s.poll() != StepState::Failed(p::Error::Storage)
            || s.next.is_some()
            || s.reserved.is_some()
            || !s.tail.is_empty()
        {
            return Err(Reason::Stale);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        let book = pending.accounting.as_ref().ok_or(Reason::Incomplete)?;
        if book.terminal(id)
            != Some(terminal::Report::new(
                terminal::Execution::StorageUnresolved,
            ))
            || !book.admission_room(1)
        {
            return Err(Reason::Incomplete);
        }
        s.failures.push((id, p::Error::Storage));
        s.result = None;
        s.submitted = None;
        s.receiver = Some(receiver);
        s.request = Some(request);
        drop(s);
        drop(pending);
        self.submit_step(step)
    }

    pub(in crate::drafts::store::worker) fn observe_slash_archive_repair(
        &self,
        step: &ArchiveStep,
        expected: Option<&[(u64, terminal::Report)]>,
        retire: bool,
    ) -> Result<Vec<(u64, terminal::Report)>, Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        a.check(step.key.route.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if entry.family != "slash-archive"
            || entry.parent.is_some()
            || a.slots.values().any(|e| e.parent == Some(step.key.id))
        {
            return Err(Reason::Unsupported);
        }
        let held = entry
            .retained
            .clone()
            .downcast::<StepHeld<Arc<p::slash_runtime::ArchiveInput>, p::slash_runtime::Archived>>()
            .map_err(|_| Reason::Stale)?;
        // Fence cancellation advances admission generation, but terminal
        // history remains the same exact slot. Observing/retiring it issues no
        // new request; only retry above requires current admission generation.
        if !Arc::ptr_eq(&held, &step.held) {
            return Err(Reason::Stale);
        }
        let mut s = held.status.lock().map_err(|_| Reason::Incomplete)?;
        if s.poll() != StepState::Ready
            || s.failures.is_empty()
            || s.failures.iter().any(|(_, e)| *e != p::Error::Storage)
            || s.next.is_some()
            || s.reserved.is_some()
            || !s.tail.is_empty()
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
        reports.push((
            s.submitted.ok_or(Reason::Incomplete)?,
            terminal::Report::new(terminal::Execution::StorageAccepted),
        ));
        if (retire && expected.is_none())
            || expected.is_some_and(|value| value != reports.as_slice())
        {
            return Err(Reason::Stale);
        }
        if !pending
            .accounting
            .as_mut()
            .ok_or(Reason::Incomplete)?
            .archive_repair(&held.input.expected.scope().key, &reports, retire)
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
