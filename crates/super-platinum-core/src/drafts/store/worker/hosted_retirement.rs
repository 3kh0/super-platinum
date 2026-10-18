//! Closed accepted-history removal, exact storage retry and observed acknowledgment.
use super::*;
type Retirement = p::hosted_files::cleanup::retirement::Retirement;
type Retired = p::hosted_files::cleanup::retirement::Retired;
type RetirementStep = Step<Retirement, Retired>;
impl Worker {
    pub(in crate::drafts::store::worker) fn hold_hosted_retirement(
        &self,
        input: Retirement,
    ) -> Result<RetirementStep, Refused<Retirement>> {
        let scope = input.receipt().record().scope().clone();
        self.family_request(&scope, input, &["hosted-retirement"], |input, reply| {
            Request::Hosted(Box::new(HostedCommand::Retire {
                input: input.clone(),
                reply,
            }))
        })
    }

    pub(in crate::drafts::store::worker) fn retry_hosted_retirement(
        &self,
        step: &RetirementStep,
    ) -> Result<(), Reason> {
        let input = step.input().as_ref().clone();
        let (reply, receiver) = mpsc::channel();
        let request = Request::Hosted(Box::new(HostedCommand::Retire { input, reply }));
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        a.check(step.key.route.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if entry.family != "hosted-retirement"
            || entry.parent.is_some()
            || a.slots.values().any(|e| e.parent == Some(step.key.id))
        {
            return Err(Reason::Unsupported);
        }
        let held = entry
            .retained
            .clone()
            .downcast::<StepHeld<Retirement, Retired>>()
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
        // Keep the actual failed attempt intact until the replacement request is
        // accepted under the same Pending lock. Fence/capacity/counter refusal
        // cannot hide the original Storage failure behind an unsubmitted Waiting.
        let submitted = Self::try_submit_locked(&mut pending, &step.key, request)
            .map_err(|refused| refused.reason)?;
        s.failures.push((id, p::Error::Storage));
        s.result = None;
        s.submitted = Some(submitted);
        s.receiver = Some(receiver);
        s.request = None;
        drop(s);
        drop(pending);
        self.ready.notify_one();
        Ok(())
    }
    pub(in crate::drafts::store::worker) fn observe_hosted_retirement(
        &self,
        step: &RetirementStep,
        expected: Option<&[(u64, terminal::Report)]>,
        retire: bool,
    ) -> Result<Vec<(u64, terminal::Report)>, Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        a.check(step.key.route.owner.account(), false)?;
        let entry = a.entry(&step.key)?;
        if entry.family != "hosted-retirement"
            || entry.parent.is_some()
            || a.slots.values().any(|e| e.parent == Some(step.key.id))
        {
            return Err(Reason::Unsupported);
        }
        let held = entry
            .retained
            .clone()
            .downcast::<StepHeld<Retirement, Retired>>()
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
        if (retire && expected.is_none()) || expected.is_some_and(|e| e != reports.as_slice()) {
            return Err(Reason::Stale);
        }
        if !pending
            .accounting
            .as_mut()
            .ok_or(Reason::Incomplete)?
            .hosted_draft_saved(held.input.receipt().record().scope(), &reports, retire)
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
