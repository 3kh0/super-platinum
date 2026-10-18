//! Closed Slash family in the existing Owned queue. No public factory or route.
use super::*;
use crate::slash_commands::controller::{Controller, ManagedTerminal, Permit, Report};
use p::slash_runtime::{Command as SlashCommand, Input as SlashInput};

pub(in crate::drafts::store::worker) type SlashReserved = Step<Arc<SlashInput>, ()>;
pub(in crate::drafts::store::worker) type SlashClaimed =
    Step<Arc<SlashInput>, p::slash_runtime::QueuedClaim>;
type ArchiveStep = Step<Arc<p::slash_runtime::ArchiveInput>, p::slash_runtime::Archived>;
#[path = "slash_archive_retry.rs"]
mod archive_retry;
struct Held {
    archive: Mutex<Option<ArchiveStep>>,
    changed: Condvar,
    #[cfg(test)]
    wait_test: Mutex<Option<mpsc::Sender<&'static str>>>,
    input: Arc<SlashInput>,
    status: Mutex<Status>,
}
struct Status {
    claim: Option<p::SlashClaim>,
    activity: u64,
    target: Option<Arc<p::SlashTarget>>,
    handed: bool,
    report: Option<Arc<Report>>,
    outcome: Option<Arc<ManagedTerminal>>,
    request: Option<Request>,
    receiver: Option<mpsc::Receiver<Result<p::SlashSettlement, p::Error>>>,
    result: Option<Result<p::SlashSettlement, p::Error>>,
    submitted: Option<u64>,
    failures: Vec<(u64, p::Error)>,
    disconnected: bool,
    building: bool,
    fresh_outstanding: bool,
}
#[derive(Clone)]
pub(in crate::drafts::store::worker) struct SlashCompletion {
    key: Key,
    held: Arc<Held>,
}
/// Distinguish no handoff, returned evidence and an already-retained storage
/// refusal. No variant erases raw data or authorizes another HTTP attempt.
pub(in crate::drafts::store::worker) enum SlashAttemptError {
    BeforeDispatch(Reason),
    Returned(Refused<Report>),
    Storage(Reason),
}
impl std::fmt::Debug for SlashAttemptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SlashAttemptError(..)")
    }
}
impl SlashCompletion {
    pub(in crate::drafts::store::worker) fn same_holder(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.held, &other.held)
    }
    pub(in crate::drafts::store::worker) fn key(&self) -> Key {
        self.key.clone()
    }
    pub(in crate::drafts::store::worker) fn input(&self) -> Arc<SlashInput> {
        self.held.input.clone()
    }
    pub(in crate::drafts::store::worker) fn report(&self) -> Option<Arc<Report>> {
        self.held.status.lock().unwrap().report.clone()
    }
    pub(in crate::drafts::store::worker) fn state(&self) -> StepState {
        let mut s = self.held.status.lock().unwrap();
        s.poll()
    }
    /// This moves the original non-Clone witness once. Historical receipts have none.
    pub(in crate::drafts::store::worker) fn take_fresh_clear(&self) -> Option<p::FreshClear> {
        let mut s = self.held.status.lock().unwrap();
        s.poll();
        let witness = s.result.as_mut()?.as_mut().ok()?.take_fresh_clear()?;
        s.fresh_outstanding = true;
        Some(witness)
    }
    pub(in crate::drafts::store::worker) fn consume_fresh_clear(&self) -> bool {
        let Ok(mut s) = self.held.status.lock() else {
            return false;
        };
        if !s.fresh_outstanding {
            return false;
        }
        s.fresh_outstanding = false;
        true
    }
    pub(in crate::drafts::store::worker) fn terminal(
        &self,
    ) -> Option<crate::slash_commands::controller::Record> {
        let mut s = self.held.status.lock().unwrap();
        s.poll();
        s.result.as_ref()?.as_ref().ok().map(|v| v.record().clone())
    }
}
impl Status {
    fn poll(&mut self) -> StepState {
        if self.result.is_none() && !self.disconnected {
            if let Some(receiver) = &self.receiver {
                match receiver.try_recv() {
                    Ok(value) => {
                        if let Err(e) = &value {
                            self.failures.push((self.submitted.unwrap(), *e));
                        }
                        self.result = Some(value);
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
impl Worker {
    /// The C3 operation owner must retain/poll this entire future. No spawned or
    /// detached task is created here. Cancellation during HTTP leaves the real
    /// DispatchPossible unresolved and the once-only holder cannot replay it.
    pub(in crate::drafts::store::worker) async fn execute_slash(
        &self,
        c: &SlashCompletion,
    ) -> Result<(), SlashAttemptError> {
        let transport = c.held.input.plan.transport().clone();
        self.execute_slash_with(c, |permit| async move {
            transport.execute_slash_command(permit).await
        })
        .await
    }
    async fn execute_slash_with<F, Fut>(
        &self,
        c: &SlashCompletion,
        send: F,
    ) -> Result<(), SlashAttemptError>
    where
        F: FnOnce(Permit) -> Fut,
        Fut: std::future::Future<Output = Report>,
    {
        let permit = self
            .arm_slash(c)
            .map_err(SlashAttemptError::BeforeDispatch)?;
        let report = send(permit).await;
        // No await between the sealed report returning and owner retention.
        self.bind_slash_report(c, report)
            .map_err(SlashAttemptError::Returned)?;
        self.submit_slash_completion(c)
            .map_err(SlashAttemptError::Storage)
    }
    #[cfg(any(test, feature = "protected-test-support"))]
    pub(in crate::drafts::store::worker) async fn fixture_execute_slash<F, Fut>(
        &self,
        c: &SlashCompletion,
        send: F,
    ) -> Result<(), SlashAttemptError>
    where
        F: FnOnce(Permit) -> Fut,
        Fut: std::future::Future<Output = Report>,
    {
        self.execute_slash_with(c, send).await
    }
    pub(in crate::drafts::store::worker) fn slash_failed_inputs(&self, pending: &Pending) -> bool {
        let Some(a) = &pending.admission else {
            return true;
        };
        a.slots.values().any(|entry| {
            if let Some(held) = entry
                .retained
                .downcast_ref::<StepHeld<CheckpointInput, p::WriteResult>>()
            {
                let mut s = held.status.lock().unwrap();
                return !matches!(s.poll(), StepState::Ready | StepState::Transferred);
            }
            if let Some(held) = entry
                .retained
                .downcast_ref::<StepHeld<PreserveInput, p::WriteResult>>()
            {
                let mut s = held.status.lock().unwrap();
                return !matches!(s.poll(), StepState::Ready | StepState::Transferred);
            }
            false
        })
    }

    pub(in crate::drafts::store::worker) fn hold_slash(
        &self,
        input: Arc<SlashInput>,
    ) -> Result<SlashReserved, Refused<Arc<SlashInput>>> {
        let scope = input.expected.scope().clone();
        self.family_request(
            &scope,
            input,
            &["slash-reserve", "slash-claim", "slash-complete"],
            |input, reply| {
                Request::SlashProtected(Box::new(SlashCommand::Reserve {
                    input: input.clone(),
                    reply,
                }))
            },
        )
    }
    pub(in crate::drafts::store::worker) fn claim_slash(
        &self,
        parent: &SlashReserved,
    ) -> Result<SlashClaimed, Reason> {
        let input = parent.input().as_ref().clone();
        let (key, held) = self.transfer(parent, "slash-claim", false, |(), mut reserved| {
            let (reply, receiver) = mpsc::channel();
            Arc::new(StepHeld {
                changed: Condvar::new(),
                input: Arc::new(input.clone()),
                status: Mutex::new(StepStatus {
                    request: Some(Request::SlashProtected(Box::new(SlashCommand::Claim {
                        input,
                        reply,
                    }))),
                    receiver: Some(receiver),
                    result: None,
                    failures: Vec::new(),
                    disconnected: false,
                    submitted: None,
                    next: None,
                    reserved: reserved.pop_front(),
                    tail: reserved,
                }),
            })
        })?;
        Ok(Step { key, held })
    }
    pub(in crate::drafts::store::worker) fn begin_slash_completion(
        &self,
        parent: &SlashClaimed,
    ) -> Result<SlashCompletion, Reason> {
        let input = parent.input().as_ref().clone();
        let (key, held) = self.transfer(parent, "slash-complete", true, |claim, _| {
            Arc::new(Held {
                archive: Mutex::new(None),
                changed: Condvar::new(),
                #[cfg(test)]
                wait_test: Mutex::new(None),
                input,
                status: Mutex::new(Status {
                    claim: Some(claim.claim),
                    activity: claim.activity,
                    target: None,
                    handed: false,
                    report: None,
                    outcome: None,
                    request: None,
                    receiver: None,
                    result: None,
                    submitted: None,
                    failures: Vec::new(),
                    disconnected: false,
                    building: false,
                    fresh_outstanding: false,
                }),
            })
        })?;
        Ok(SlashCompletion { key, held })
    }
    pub(in crate::drafts::store::worker) fn resume_slash_completion(
        &self,
        key: &Key,
    ) -> Result<SlashCompletion, Reason> {
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
        Ok(SlashCompletion {
            key: key.clone(),
            held,
        })
    }
    fn exact_slash<'a>(
        &self,
        pending: &'a Pending,
        c: &SlashCompletion,
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
    pub(in crate::drafts::store::worker) fn arm_slash(
        &self,
        c: &SlashCompletion,
    ) -> Result<Permit, Reason> {
        let pending = self.pending.lock().unwrap();
        let entry = self.exact_slash(&pending, c)?;
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
        if pending.upgrade_generation != s.activity
            || pending
                .accounting
                .as_ref()
                .is_none_or(|b| b.slash_inflight())
        {
            return Err(Reason::Incomplete);
        }
        if s.handed || s.report.is_some() {
            return Err(Reason::Stale);
        }
        let claim = s.claim.take().ok_or(Reason::Incomplete)?;
        // No await, I/O or CAS. The existing sealed Admission provides revocation
        // for native/auth input changes; no queue ID stands in for that authority.
        match Controller::arm_claimed(&c.held.input.plan, claim) {
            Ok((permit, target)) => {
                s.target = Some(Arc::new(target));
                s.handed = true;
                Ok(permit)
            }
            Err(refused) => {
                s.claim = Some(refused.claim);
                Err(Reason::Stale)
            }
        }
    }
    pub(in crate::drafts::store::worker) fn bind_slash_report(
        &self,
        c: &SlashCompletion,
        report: Report,
    ) -> Result<(), Refused<Report>> {
        let pending = self.pending.lock().unwrap();
        let refusal = |reason, input| Refused {
            reason,
            generation: pending.admission.as_ref().map(|a| a.generation),
            input,
        };
        if let Err(e) = self.exact_slash(&pending, c) {
            return Err(refusal(e, report));
        }
        let mut s = c.held.status.lock().unwrap();
        if !s.handed || s.report.is_some() || !c.held.input.plan.accepts(&report) {
            return Err(refusal(Reason::Stale, report));
        }
        // Retain actual sealed report first; no storage refusal can discard it.
        s.report = Some(Arc::new(report));
        Ok(())
    }
    pub(in crate::drafts::store::worker) fn submit_slash_completion(
        &self,
        c: &SlashCompletion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        self.exact_slash(&pending, c)?;
        let mut s = c.held.status.lock().unwrap();
        if s.submitted.is_some() {
            return Ok(());
        }
        if s.request.is_none() {
            if s.building {
                return Err(Reason::Incomplete);
            }
            let report = s.report.clone().ok_or(Reason::Incomplete)?;
            let target = s.target.clone().ok_or(Reason::Incomplete)?;
            s.building = true;
            drop(s);
            drop(pending);
            let outcome = c.held.input.plan.finish(&report).map(Arc::new);
            let mut s = c.held.status.lock().unwrap();
            s.building = false;
            let outcome = outcome.map_err(|_| Reason::Stale)?;
            let (reply, receiver) = mpsc::channel();
            s.outcome = Some(outcome.clone());
            s.receiver = Some(receiver);
            s.request = Some(Request::SlashProtected(Box::new(SlashCommand::Complete {
                target,
                outcome,
                reply,
            })));
        } else {
            drop(s);
            drop(pending)
        }
        let mut pending = self.pending.lock().unwrap();
        self.exact_slash(&pending, c)?;
        let mut s = c.held.status.lock().unwrap();
        if s.submitted.is_some() {
            return Ok(());
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
    pub(in crate::drafts::store::worker) fn retry_slash_completion(
        &self,
        c: &SlashCompletion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        self.exact_slash(&pending, c)?;
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
        s.result = None;
        s.submitted = None;
        s.request = None;
        s.receiver = None;
        drop(s);
        drop(pending);
        self.submit_slash_completion(c)
    }
}

#[path = "slash_wait.rs"]
mod wait;

#[path = "slash_family_retirement.rs"]
mod family_retirement;

impl Worker {
    pub(in crate::drafts::store::worker) fn hold_slash_archive(
        &self,
        completion: &SlashCompletion,
        input: Arc<p::slash_runtime::ArchiveInput>,
    ) -> Result<ArchiveStep, Reason> {
        // This dedicated holder mutex is never acquired under Pending/Status.
        // Reattached completion handles share it, so capture cannot issue twice.
        let mut retained = completion
            .held
            .archive
            .lock()
            .map_err(|_| Reason::Incomplete)?;
        if let Some(step) = retained.as_ref() {
            let prior = step.input();
            if prior.intent != input.intent
                || !prior.expected.same(&input.expected)
                || prior.writer.id() != input.writer.id()
                || prior.serial != input.serial
            {
                return Err(Reason::Stale);
            }
            return self.reattach_step(&step.key);
        }
        let scope = input.expected.scope().clone();
        let step = self
            .family_request(&scope, input, &["slash-archive"], |input, reply| {
                Request::SlashProtected(Box::new(SlashCommand::Archive {
                    input: input.clone(),
                    reply,
                }))
            })
            .map_err(|e| e.reason)?;
        *retained = Some(step.clone());
        Ok(step)
    }
}
