//! Pre-reserved owned claim/completion. Raw replies stay in the original holder.
use super::*;
#[path = "hosted_draft_retry.rs"]
mod draft_retry;
#[path = "hosted_retirement.rs"]
mod retirement;
use p::hosted_files::{
    Claim, Record, Returned,
    runtime::{Command as HostedCommand, Prepared as HostedInput, QueuedClaim},
};
pub(in crate::drafts::store::worker) type HostedClaimed = Step<Arc<HostedInput>, QueuedClaim>;
struct Held {
    input: Arc<HostedInput>,
    claim: Arc<Claim>,
    status: Mutex<Status>,
    changed: Condvar,
}
struct Status {
    activity: u64,
    handed: bool,
    returned: Option<Arc<Returned>>,
    request: Option<Request>,
    receiver: Option<mpsc::Receiver<Result<Record, p::Error>>>,
    result: Option<Result<Record, p::Error>>,
    submitted: Option<u64>,
    failures: Vec<u64>,
    disconnected: bool,
    reserved: Key,
    cleanup: Option<Step<Arc<HostedInput>, Arc<Mutex<p::hosted_files::cleanup::Completed>>>>,
}
impl Status {
    fn poll(&mut self) -> StepState {
        if self.result.is_none() && !self.disconnected {
            if let Some(receiver) = &self.receiver {
                match receiver.try_recv() {
                    Ok(v) => {
                        self.result = Some(v);
                        self.receiver = None;
                    }
                    Err(mpsc::TryRecvError::Empty) => (),
                    Err(mpsc::TryRecvError::Disconnected) => self.disconnected = true,
                }
            }
        }
        match &self.result {
            Some(Ok(_)) => StepState::Ready,
            Some(Err(e)) => StepState::Failed(*e),
            None if self.disconnected => StepState::Disconnected,
            _ => StepState::Waiting,
        }
    }
}
#[derive(Clone)]
pub(in crate::drafts::store::worker) struct HostedCompletion {
    key: Key,
    held: Arc<Held>,
}
impl HostedCompletion {
    pub(in crate::drafts::store::worker) fn key(&self) -> Key {
        self.key.clone()
    }
    pub(in crate::drafts::store::worker) fn input(&self) -> Arc<HostedInput> {
        self.held.input.clone()
    }
    pub(in crate::drafts::store::worker) fn state(&self) -> StepState {
        self.held.status.lock().unwrap().poll()
    }
    pub(in crate::drafts::store::worker) fn result(&self) -> Option<Result<Record, p::Error>> {
        let mut s = self.held.status.lock().unwrap();
        s.poll();
        s.result.clone()
    }
    pub(in crate::drafts::store::worker) fn returned(&self) -> Option<Arc<Returned>> {
        self.held.status.lock().unwrap().returned.clone()
    }
}
impl Worker {
    pub(in crate::drafts::store::worker) fn hold_hosted_draft(
        &self,
        input: p::hosted_files::runtime::SaveDraft,
    ) -> Result<
        Step<p::hosted_files::runtime::SaveDraft, Option<p::hosted_files::editor::Draft>>,
        Refused<p::hosted_files::runtime::SaveDraft>,
    > {
        let scope = input.input.review().scope().clone();
        self.family_request(&scope, input, &["hosted-draft-save"], |input, reply| {
            Request::Hosted(Box::new(HostedCommand::SaveDraft {
                input: input.clone(),
                reply,
            }))
        })
    }
    pub(in crate::drafts::store::worker) fn hold_hosted_draft_load(
        &self,
        scope: Scope,
    ) -> Result<Step<Scope, Option<p::hosted_files::editor::Draft>>, Refused<Scope>> {
        self.family_request(
            &scope.clone(),
            scope,
            &["hosted-draft-load"],
            |scope, reply| {
                Request::Hosted(Box::new(HostedCommand::LoadDraft {
                    scope: scope.clone(),
                    reply,
                }))
            },
        )
    }
    pub(in crate::drafts::store::worker) fn hold_hosted(
        &self,
        input: Arc<HostedInput>,
    ) -> Result<HostedClaimed, Refused<Arc<HostedInput>>> {
        let scope = input.expected.scope().clone();
        self.family_request(
            &scope,
            input,
            &["hosted-claim", "hosted-complete", "hosted-cleanup"],
            |input, reply| {
                Request::Hosted(Box::new(HostedCommand::Claim {
                    input: input.clone(),
                    reply,
                }))
            },
        )
    }
    pub(in crate::drafts::store::worker) fn begin_hosted_completion(
        &self,
        parent: &HostedClaimed,
    ) -> Result<HostedCompletion, Reason> {
        let input = parent.input().as_ref().clone();
        let (key, held) = self.transfer(parent, "hosted-complete", false, |claim, mut tail| {
            Arc::new(Held {
                input,
                claim: Arc::new(claim.claim),
                changed: Condvar::new(),
                status: Mutex::new(Status {
                    activity: claim.activity,
                    handed: false,
                    returned: None,
                    request: None,
                    receiver: None,
                    result: None,
                    submitted: None,
                    failures: vec![],
                    disconnected: false,
                    reserved: tail.pop_front().expect("reserved hosted cleanup"),
                    cleanup: None,
                }),
            })
        })?;
        Ok(HostedCompletion { key, held })
    }
    pub(in crate::drafts::store::worker) fn resume_hosted_completion(
        &self,
        key: &Key,
    ) -> Result<HostedCompletion, Reason> {
        let pending = self.pending.lock().unwrap();
        let held = pending
            .admission
            .as_ref()
            .ok_or(Reason::Unsupported)?
            .entry(key)?
            .retained
            .clone()
            .downcast::<Held>()
            .map_err(|_| Reason::Stale)?;
        Ok(HostedCompletion {
            key: key.clone(),
            held,
        })
    }
    fn exact_hosted<'a>(
        &self,
        pending: &'a Pending,
        c: &HostedCompletion,
    ) -> Result<&'a Entry, Reason> {
        let entry = pending
            .admission
            .as_ref()
            .ok_or(Reason::Unsupported)?
            .entry(&c.key)?;
        let held = entry
            .retained
            .clone()
            .downcast::<Held>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&held, &c.held) {
            return Err(Reason::Stale);
        }
        Ok(entry)
    }
    /// Synchronous final worker/stream fence. The caller must have just checked
    /// native owner/source/policy; no await is permitted between that and arm.
    pub(in crate::drafts::store::worker) fn arm_hosted(
        &self,
        c: &HostedCompletion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        let entry = self.exact_hosted(&pending, c)?;
        let admission = pending.admission.as_ref().unwrap();
        if entry.generation != admission.generation {
            return Err(Reason::Stale);
        }
        admission.check(c.held.input.account(), false)?;
        if self.slash_failed_inputs(&pending)
            || pending.failed.contains(c.held.input.account())
            || pending.blocked.contains(c.held.input.account())
            || pending
                .updates
                .keys()
                .any(|(a, _)| a == c.held.input.account())
            || pending
                .requests
                .iter()
                .any(|r| r.slash_fence_account() == Some(c.held.input.account()))
        {
            return Err(Reason::Incomplete);
        }
        let mut s = c.held.status.lock().unwrap();
        if s.activity != pending.upgrade_generation
            || pending
                .accounting
                .as_ref()
                .is_none_or(|b| b.slash_inflight())
        {
            return Err(Reason::Incomplete);
        }
        if s.handed || s.returned.is_some() || !c.held.input.stream.is_live() {
            return Err(Reason::Stale);
        }
        s.handed = true;
        Ok(())
    }
    /// Retain immediately after the one HTTP future resolves, before any await.
    /// Current owner/generation/stream/source is intentionally not re-admitted.
    pub(in crate::drafts::store::worker) fn bind_hosted_returned(
        &self,
        c: &HostedCompletion,
        value: Returned,
    ) -> Result<(), Refused<Returned>> {
        let pending = self.pending.lock().unwrap();
        let fail = |reason, input| Refused {
            reason,
            generation: pending.admission.as_ref().map(|a| a.generation),
            input,
        };
        if let Err(e) = self.exact_hosted(&pending, c) {
            return Err(fail(e, value));
        }
        let mut s = c.held.status.lock().unwrap();
        if !s.handed || s.returned.is_some() {
            return Err(fail(Reason::Stale, value));
        }
        s.returned = Some(Arc::new(value));
        Ok(())
    }
    pub(in crate::drafts::store::worker) fn submit_hosted_completion(
        &self,
        c: &HostedCompletion,
    ) -> Result<(), Reason> {
        let mut pending = self.pending.lock().unwrap();
        self.exact_hosted(&pending, c)?;
        let mut s = c.held.status.lock().unwrap();
        if s.submitted.is_some() {
            return Ok(());
        }
        if s.request.is_none() {
            let returned = s.returned.clone().ok_or(Reason::Incomplete)?;
            let (reply, receiver) = mpsc::channel();
            s.receiver = Some(receiver);
            s.request = Some(Request::Hosted(Box::new(HostedCommand::Settle {
                claim: c.held.claim.clone(),
                scope: c.held.input.expected.scope().clone(),
                returned,
                reply,
            })));
        }
        let request = s.request.take().ok_or(Reason::Incomplete)?;
        match Self::try_submit_locked(&mut pending, &c.key, request) {
            Ok(id) => {
                s.submitted = Some(id);
                drop(s);
                drop(pending);
                self.ready.notify_one();
                Ok(())
            }
            Err(e) => {
                s.request = Some(e.input);
                Err(e.reason)
            }
        }
    }
    pub(in crate::drafts::store::worker) fn retry_hosted_completion(
        &self,
        c: &HostedCompletion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        self.exact_hosted(&pending, c)?;
        let mut s = c.held.status.lock().unwrap();
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
        s.failures.push(id);
        s.result = None;
        s.submitted = None;
        s.request = None;
        s.receiver = None;
        drop(s);
        drop(pending);
        self.submit_hosted_completion(c)
    }
    pub(in crate::drafts::store::worker) fn wait_hosted_completion(
        &self,
        c: &HostedCompletion,
    ) -> Result<StepState, Reason> {
        {
            let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
            self.exact_hosted(&pending, c)?;
        }
        loop {
            let mut s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
            let state = s.poll();
            let id = s.submitted;
            if state != StepState::Waiting || id.is_none() {
                drop(s);
                if matches!(state, StepState::Ready | StepState::Failed(_)) {
                    let id = id.ok_or(Reason::Incomplete)?;
                    self.wait_accounted(&c.key, id)?;
                    let mut s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
                    if s.submitted == Some(id) {
                        return Ok(s.poll());
                    }
                    continue;
                }
                return Ok(state);
            }
            if let Some(receiver) = s.receiver.take() {
                drop(s);
                let result = receiver.recv();
                s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
                match result {
                    Ok(v) => s.result = Some(v),
                    Err(_) => s.disconnected = true,
                };
                c.held.changed.notify_all();
            } else {
                s = c.held.changed.wait(s).map_err(|_| Reason::Incomplete)?;
            }
            drop(s);
        }
    }
}

#[path = "hosted_cleanup_steps.rs"]
mod cleanup;
