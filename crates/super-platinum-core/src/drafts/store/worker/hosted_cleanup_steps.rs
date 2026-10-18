//! Third pre-reserved slot: storage cleanup and observed family retirement.
use super::*;
use p::hosted_files::cleanup::{Completed, FreshClear, Receipt};
type Cleanup = Step<Arc<HostedInput>, Arc<Mutex<Completed>>>;
impl Worker {
    pub(in crate::drafts::store::worker) fn begin_hosted_cleanup(
        &self,
        c: &HostedCompletion,
    ) -> Result<Cleanup, Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        self.exact_hosted(&pending, c)?;
        let mut s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if let Some(step) = &s.cleanup {
            return Ok(step.clone());
        }
        if s.poll() != StepState::Ready {
            return Err(Reason::Incomplete);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        if pending
            .accounting
            .as_ref()
            .and_then(|b| b.terminal(id))
            .is_none()
        {
            return Err(Reason::Incomplete);
        }
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let slot = a.entry(&s.reserved)?;
        if slot.family != "hosted-cleanup"
            || slot.parent != a.entry(&c.key)?.parent
            || !slot.retained.is::<ReservedSlot>()
        {
            return Err(Reason::Stale);
        }
        let (reply, receiver) = mpsc::channel();
        let held = Arc::new(StepHeld {
            input: Arc::new(c.held.input.clone()),
            changed: Condvar::new(),
            status: Mutex::new(StepStatus {
                request: Some(Request::Hosted(Box::new(HostedCommand::Cleanup {
                    claim: c.held.claim.clone(),
                    stream: c.held.input.stream.clone(),
                    reply,
                }))),
                receiver: Some(receiver),
                result: None,
                failures: vec![],
                disconnected: false,
                submitted: None,
                next: None,
                reserved: None,
                tail: Default::default(),
            }),
        });
        let step = Step {
            key: s.reserved.clone(),
            held,
        };
        pending
            .admission
            .as_mut()
            .unwrap()
            .slots
            .get_mut(&step.key.id)
            .ok_or(Reason::Stale)?
            .retained = step.held.clone();
        s.cleanup = Some(step.clone());
        Ok(step)
    }
    pub(in crate::drafts::store::worker) fn observe_hosted_cleanup(
        &self,
        c: &HostedCompletion,
        step: &Cleanup,
        retire: bool,
    ) -> Result<Receipt, Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        self.exact_hosted(&pending, c)?;
        let keys = super::super::family_retirement::keys(
            &pending,
            &step.key,
            &["hosted-claim", "hosted-complete", "hosted-cleanup"],
        )?;
        let root = super::super::family_retirement::transferred::<Arc<HostedInput>, QueuedClaim>(
            &pending, &keys[0], &keys[1],
        )?;
        let mut parent = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if parent.poll() != StepState::Ready
            || parent
                .cleanup
                .as_ref()
                .is_none_or(|s| !Arc::ptr_eq(&s.held, &step.held))
        {
            return Err(Reason::Incomplete);
        }
        let second = parent.submitted.ok_or(Reason::Incomplete)?;
        let mut s = step.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if s.poll() != StepState::Ready
            || s.next.is_some()
            || s.reserved.is_some()
            || !s.tail.is_empty()
            || s.disconnected
        {
            return Err(Reason::Incomplete);
        }
        let final_id = s.submitted.ok_or(Reason::Incomplete)?;
        let done = s
            .result
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .ok_or(Reason::Incomplete)?
            .lock()
            .map_err(|_| Reason::Incomplete)?;
        let receipt = done.receipt.clone();
        if receipt.record().id() != c.held.claim.record().id() {
            return Err(Reason::Stale);
        }
        let book = pending.accounting.as_mut().ok_or(Reason::Incomplete)?;
        let returned = parent
            .failures
            .iter()
            .copied()
            .chain(std::iter::once(second))
            .map(|id| {
                book.terminal(id)
                    .map(|report| (id, report))
                    .ok_or(Reason::Incomplete)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if s.failures.iter().any(|(_, e)| *e != p::Error::Storage) {
            return Err(Reason::Incomplete);
        }
        let cleaned = s
            .failures
            .iter()
            .map(|(id, _)| *id)
            .chain(std::iter::once(final_id))
            .map(|id| {
                book.terminal(id)
                    .map(|report| (id, report))
                    .ok_or(Reason::Incomplete)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !book.hosted_settlement(
            c.held.input.review().scope(),
            root,
            &returned,
            &cleaned,
            retire,
        ) {
            return Err(Reason::Incomplete);
        }
        if retire {
            for key in keys {
                pending.admission.as_mut().unwrap().slots.remove(&key.id);
            }
        }
        Ok(receipt)
    }
    pub(in crate::drafts::store::worker) fn retry_hosted_cleanup(
        &self,
        c: &HostedCompletion,
        step: &Cleanup,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        self.exact_hosted(&pending, c)?;
        let parent = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if parent
            .cleanup
            .as_ref()
            .is_none_or(|s| !Arc::ptr_eq(&s.held, &step.held))
        {
            return Err(Reason::Stale);
        }
        let mut s = step.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if s.poll() != StepState::Failed(p::Error::Storage) {
            return Err(Reason::Stale);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        if pending
            .accounting
            .as_ref()
            .and_then(|b| b.terminal(id))
            .is_none()
        {
            return Err(Reason::Incomplete);
        }
        let (reply, receiver) = mpsc::channel();
        s.failures.push((id, p::Error::Storage));
        s.result = None;
        s.submitted = None;
        s.receiver = Some(receiver);
        s.request = Some(Request::Hosted(Box::new(HostedCommand::Cleanup {
            claim: c.held.claim.clone(),
            stream: c.held.input.stream.clone(),
            reply,
        })));
        drop(s);
        drop(parent);
        drop(pending);
        self.submit_step(step)
    }
    pub(in crate::drafts::store::worker) fn take_hosted_clear(
        &self,
        c: &HostedCompletion,
        step: &Cleanup,
    ) -> Result<Option<FreshClear>, Reason> {
        self.observe_hosted_cleanup(c, step, false)?;
        let s = step.held.status.lock().map_err(|_| Reason::Incomplete)?;
        let value = s
            .result
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .ok_or(Reason::Incomplete)?;
        Ok(value.lock().map_err(|_| Reason::Incomplete)?.fresh.take())
    }
}
