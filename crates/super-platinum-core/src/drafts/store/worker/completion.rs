//! Fixture-private exact storage completions. No HTTP, Current or drain authority.
use super::*;
use crate::drafts::store::protected as p;
use crate::drafts::sync::Operation as DraftOperation;
#[path = "completion_steps.rs"]
mod steps;
pub(in crate::drafts::store::worker) use steps::{
    BaselineInput, CheckInput, Checkpointed, Claimed, ClaimedBaseline, ClaimedCheck,
    ObservationInput, Prepared, Preserved, Registered, Released, ReservedBaseline, ReservedCheck,
    ReservedObservation, SendClaimed, SendConsumed, SendPrepared, Step, StepState,
};
pub use steps::{
    CheckpointInput, LocalOutcome, LocalWriteInput, MutationInput, PreserveInput, ReleaseInput,
    SendInput, WriteOutcome,
};
pub(in crate::drafts::store::worker) use steps::{HostedClaimed, HostedCompletion};
#[path = "completion_prerequisites.rs"]
mod prerequisites;
pub(in crate::drafts::store::worker) use prerequisites::TakeoverInput;
pub use prerequisites::{GapInput, OpenInput, TeamInput};
#[path = "completion_requests.rs"]
mod requests;

// Values come only from actual typed executor replies. There is no ID/JSON
// constructor, no Clone for linear tokens, and no generic continuation builder.
enum Issued {
    Mutation(p::ClaimedDispatch),
    Check(p::CheckClaim),
    Baseline(p::BaselineClaim),
    Observation(p::ObservationTicket),
    Local(p::Release, p::Writer),
}
enum Binding {
    Mutation(p::Receipt),
    Check(p::CheckCompletion),
    Baseline(p::BaselineCompletion),
    Observation(Arc<p::ObservationTicket>),
    Local(Arc<p::Release>, p::Writer),
    DeferredCheck(p::CheckTicket, u64),
    DeferredBaseline(p::BaselineTicket, u64),
}
pub(in crate::drafts::store::worker) enum Dispatch {
    Mutation(p::ClaimedDispatch),
    Check(DraftOperation),
    Baseline(p::BaselineRequest),
}
#[derive(Clone, PartialEq)]
pub(in crate::drafts::store::worker) enum Raw {
    Mutation {
        outcome: p::Outcome,
        known_until: u64,
    },
    Check {
        result: p::CheckResult,
        known_until: u64,
    },
    Baseline {
        response: p::BaselineResponse,
        known_until: u64,
    },
    Observation,
    Local,
}
pub(in crate::drafts::store::worker) enum Finished {
    Mutation(Result<p::Review, p::Error>),
    Check(Result<p::CheckReceipt, p::Error>),
    Baseline(Result<p::BaselineResult, p::Error>),
    Observation(Result<p::ObservationResult, p::Error>),
    Local(Result<p::LocalWriterLease, p::Error>),
}
impl Finished {
    fn storage_error(&self) -> bool {
        match self {
            Self::Mutation(r) => matches!(r, Err(p::Error::Storage)),
            Self::Check(r) => matches!(r, Err(p::Error::Storage)),
            Self::Baseline(r) => matches!(r, Err(p::Error::Storage)),
            Self::Observation(r) => matches!(r, Err(p::Error::Storage)),
            Self::Local(r) => matches!(r, Err(p::Error::Storage)),
        }
    }
}
struct CompletionHeld {
    status: Mutex<CompletionStatus>,
    changed: Condvar,
}
struct CompletionStatus {
    baseline: bool,
    baseline_deferred_initial: bool,
    deferred: baseline_deferred::Deferred,
    issued: Option<Issued>,
    binding: Option<Arc<Binding>>,
    dispatch: Option<Dispatch>,
    handed: bool,
    raw: Option<Arc<Raw>>,
    request: Option<Request>,
    receiver: Option<requests::Receiver>,
    result: Option<Arc<Finished>>,
    disconnected: bool,
    submitted: Option<u64>,
    attempts: Vec<(u64, Arc<Finished>)>,
    building: bool,
}
impl CompletionHeld {
    fn new(issued: Issued) -> Arc<Self> {
        Arc::new(Self {
            changed: Condvar::new(),
            status: Mutex::new(CompletionStatus {
                baseline: matches!(&issued, Issued::Baseline(_)),
                baseline_deferred_initial: matches!(
                    &issued,
                    Issued::Baseline(p::BaselineClaim::Deferred { .. })
                ),
                deferred: Default::default(),
                issued: Some(issued),
                binding: None,
                dispatch: None,
                handed: false,
                raw: None,
                request: None,
                receiver: None,
                result: None,
                disconnected: false,
                submitted: None,
                attempts: Vec::new(),
                building: false,
            }),
        })
    }
    fn initialize(&self) -> Result<(), Reason> {
        let mut s = self.status.lock().unwrap();
        if s.binding.is_some() {
            return Ok(());
        }
        let issued = s.issued.take().ok_or(Reason::Incomplete)?;
        drop(s);
        // Preserve the original issued claim through handoff. Receipt cloning
        // for storage completion happens outside Pending and cannot mint a claim.
        let (binding, dispatch) = match issued {
            Issued::Mutation(c) => {
                let receipt = c.receipt().clone();
                (Binding::Mutation(receipt), Some(Dispatch::Mutation(c)))
            }
            Issued::Check(p::CheckClaim::Ready {
                operation,
                completion,
            }) => (Binding::Check(completion), Some(Dispatch::Check(operation))),
            Issued::Check(p::CheckClaim::Deferred { ticket, until }) => {
                (Binding::DeferredCheck(ticket, until), None)
            }
            Issued::Baseline(p::BaselineClaim::Ready {
                request,
                completion,
            }) => (
                Binding::Baseline(completion),
                Some(Dispatch::Baseline(request)),
            ),
            Issued::Baseline(p::BaselineClaim::Deferred { ticket, until }) => {
                (Binding::DeferredBaseline(ticket, until), None)
            }
            Issued::Observation(ticket) => (Binding::Observation(Arc::new(ticket)), None),
            Issued::Local(release, writer) => (Binding::Local(Arc::new(release), writer), None),
        };
        let mut s = self.status.lock().unwrap();
        s.binding = Some(Arc::new(binding));
        s.dispatch = dispatch;
        Ok(())
    }
}
#[derive(Clone)]
pub(in crate::drafts::store::worker) struct Completion {
    key: Key,
    held: Arc<CompletionHeld>,
}
impl Completion {
    pub(in crate::drafts::store::worker) fn is_baseline(&self) -> bool {
        self.held.status.lock().unwrap().baseline
    }
    // Observation of the exact already-issued completion binding. This clone
    // cannot mint/recover ClaimedDispatch or authorize another network attempt.
    pub(in crate::drafts::store::worker) fn mutation_receipt(&self) -> Result<p::Receipt, Reason> {
        let s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
        match s.binding.as_deref() {
            Some(Binding::Mutation(receipt)) => Ok(receipt.clone()),
            _ => Err(Reason::Stale),
        }
    }

    pub(in crate::drafts::store::worker) fn is_mutation(&self) -> bool {
        matches!(
            self.held.status.lock().unwrap().binding.as_deref(),
            Some(Binding::Mutation(_))
        )
    }
    pub(in crate::drafts::store::worker) fn is_observation(&self) -> bool {
        matches!(
            self.held.status.lock().unwrap().binding.as_deref(),
            Some(Binding::Observation(_))
        )
    }
    pub(in crate::drafts::store::worker) fn is_local(&self) -> bool {
        matches!(
            self.held.status.lock().unwrap().binding.as_deref(),
            Some(Binding::Local(_, _))
        )
    }
    pub(in crate::drafts::store::worker) fn key(&self) -> Key {
        self.key.clone()
    }
    pub(in crate::drafts::store::worker) fn raw(&self) -> Option<Arc<Raw>> {
        self.held.status.lock().unwrap().raw.clone()
    }
    pub(in crate::drafts::store::worker) fn deferred_until(&self) -> Option<u64> {
        let s = self.held.status.lock().unwrap();
        match s.binding.as_deref() {
            Some(Binding::DeferredCheck(_, until) | Binding::DeferredBaseline(_, until)) => {
                Some(*until)
            }
            _ => None,
        }
    }
    pub(in crate::drafts::store::worker) fn result(&self) -> Result<Option<Arc<Finished>>, Reason> {
        let mut s = self.held.status.lock().unwrap();
        if s.result.is_some() {
            return Ok(s.result.clone());
        }
        if s.disconnected {
            return Err(Reason::Incomplete);
        }
        if let Some(receiver) = &s.receiver {
            match receiver.poll() {
                Ok(Some(result)) => {
                    let result = Arc::new(result);
                    let id = s.submitted.ok_or(Reason::Incomplete)?;
                    s.attempts.push((id, result.clone()));
                    s.result = Some(result);
                    s.receiver = None;
                }
                Ok(None) => (),
                Err(_) => {
                    s.disconnected = true;
                    return Err(Reason::Incomplete);
                }
            }
        }
        Ok(s.result.clone())
    }
}
impl Worker {
    pub(in crate::drafts::store::worker) fn begin_send_record(
        &self,
        parent: &SendClaimed,
    ) -> Result<Completion, Reason> {
        let (key, held) = self.transfer(parent, "protected-record", true, |v, _| {
            CompletionHeld::new(Issued::Mutation(v))
        })?;
        held.initialize()?;
        Ok(Completion { key, held })
    }
    pub(in crate::drafts::store::worker) fn begin_start_local(
        &self,
        parent: &Released,
    ) -> Result<Completion, Reason> {
        let writer = parent.input().writer.clone();
        let (key, held) = self.transfer(parent, "protected-start-local", true, |release, _| {
            CompletionHeld::new(Issued::Local(release, writer))
        })?;
        held.initialize()?;
        let result = Completion { key, held };
        // This closed storage completion has no HTTP outcome to accept.
        let mut status = result.held.status.lock().unwrap();
        if status.raw.is_none() {
            status.raw = Some(Arc::new(Raw::Local));
        }
        drop(status);
        Ok(result)
    }
    pub(in crate::drafts::store::worker) fn begin_record(
        &self,
        parent: &Claimed,
    ) -> Result<Completion, Reason> {
        let (key, held) = self.transfer(parent, "protected-record", true, |v, _| {
            CompletionHeld::new(Issued::Mutation(v))
        })?;
        held.initialize()?;
        Ok(Completion { key, held })
    }
    pub(in crate::drafts::store::worker) fn begin_check_completion(
        &self,
        parent: &ClaimedCheck,
    ) -> Result<Completion, Reason> {
        let (key, held) = self.transfer(parent, "protected-complete-check", true, |v, _| {
            CompletionHeld::new(Issued::Check(v))
        })?;
        held.initialize()?;
        Ok(Completion { key, held })
    }
    pub(in crate::drafts::store::worker) fn begin_baseline_completion(
        &self,
        parent: &ClaimedBaseline,
    ) -> Result<Completion, Reason> {
        let (key, held) = self.transfer(parent, "protected-retain-baseline", true, |v, _| {
            CompletionHeld::new(Issued::Baseline(v))
        })?;
        held.initialize()?;
        Ok(Completion { key, held })
    }
    pub(in crate::drafts::store::worker) fn begin_observation_completion(
        &self,
        parent: &ReservedObservation,
    ) -> Result<Completion, Reason> {
        let (key, held) = self.transfer(parent, "protected-finish-observation", true, |v, _| {
            CompletionHeld::new(Issued::Observation(v))
        })?;
        held.initialize()?;
        Ok(Completion { key, held })
    }
    pub(in crate::drafts::store::worker) fn bind_observation_completion(
        &self,
        c: &Completion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        self.exact_completion(&pending, c)?;
        let mut s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if !matches!(s.binding.as_deref(), Some(Binding::Observation(_))) {
            return Err(Reason::Stale);
        }
        match s.raw.as_deref() {
            None => s.raw = Some(Arc::new(Raw::Observation)),
            Some(Raw::Observation) => (),
            _ => return Err(Reason::Stale),
        }
        Ok(())
    }
    pub(in crate::drafts::store::worker) fn reattach_completion(
        &self,
        key: &Key,
    ) -> Result<Completion, Reason> {
        let pending = self.pending.lock().unwrap();
        let held = pending
            .admission
            .as_ref()
            .ok_or(Reason::Unsupported)?
            .entry(key)?
            .retained
            .clone()
            .downcast::<CompletionHeld>()
            .map_err(|_| Reason::Stale)?;
        Ok(Completion {
            key: key.clone(),
            held,
        })
    }
    fn exact_completion<'a>(
        &self,
        pending: &'a Pending,
        c: &Completion,
    ) -> Result<&'a Entry, Reason> {
        let entry = pending
            .admission
            .as_ref()
            .ok_or(Reason::Unsupported)?
            .entry(&c.key)?;
        let actual = entry
            .retained
            .clone()
            .downcast::<CompletionHeld>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &c.held) {
            return Err(Reason::Stale);
        }
        Ok(entry)
    }
    pub(in crate::drafts::store::worker) fn take_dispatch(
        &self,
        c: &Completion,
    ) -> Result<Dispatch, Reason> {
        let pending = self.pending.lock().unwrap();
        let entry = self.exact_completion(&pending, c)?;
        let a = pending.admission.as_ref().unwrap();
        if entry.generation != a.generation {
            return Err(Reason::Stale);
        }
        a.check(a.identity.owner.account(), false)?;
        let mut s = c.held.status.lock().unwrap();
        if s.handed || s.raw.is_some() {
            return Err(Reason::Stale);
        }
        let dispatch = s.dispatch.take().ok_or(Reason::Incomplete)?;
        s.handed = true;
        Ok(dispatch)
    }
    // A real local pre-dispatch cancellation. The holder proves no instruction
    // was handed out. It cannot synthesize positive remote evidence.
    pub(in crate::drafts::store::worker) fn cancel_before_dispatch(
        &self,
        c: &Completion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        self.exact_completion(&pending, c)?;
        let mut s = c.held.status.lock().unwrap();
        if s.handed || s.raw.is_some() {
            return Err(Reason::Stale);
        }
        let raw = match s.binding.as_deref() {
            Some(Binding::Mutation(_)) => Raw::Mutation {
                outcome: p::Outcome::Uncertain,
                known_until: 0,
            },
            Some(Binding::Check(_)) => Raw::Check {
                result: p::CheckResult::Nonpositive(p::KnownReadFailure::NotRequested),
                known_until: 0,
            },
            Some(Binding::Baseline(_)) => Raw::Baseline {
                response: p::BaselineResponse::Nonpositive(p::KnownReadFailure::NotRequested),
                known_until: 0,
            },
            _ => return Err(Reason::Stale),
        };
        s.dispatch = None;
        s.raw = Some(Arc::new(raw));
        Ok(())
    }
    // Bind raw BEFORE attempting storage admission. Even dead/unsupported/full
    // queue cannot discard an already returned HTTP observation. A wrong family
    // or second raw response is returned unchanged; the first remains retained.
    pub(in crate::drafts::store::worker) fn bind_raw(
        &self,
        c: &Completion,
        raw: Raw,
    ) -> Result<(), Refused<Raw>> {
        let pending = self.pending.lock().unwrap();
        let fail = |reason, input| Refused {
            reason,
            generation: pending.admission.as_ref().map(|a| a.generation),
            input,
        };
        if let Err(e) = self.exact_completion(&pending, c) {
            return Err(fail(e, raw));
        }
        let mut s = c.held.status.lock().unwrap();
        let matches = matches!(
            (s.binding.as_deref(), &raw),
            (Some(Binding::Mutation(_)), Raw::Mutation { .. })
                | (Some(Binding::Check(_)), Raw::Check { .. })
                | (Some(Binding::Baseline(_)), Raw::Baseline { .. })
                | (Some(Binding::Observation(_)), Raw::Observation)
        );
        if !matches || s.raw.is_some() || (!s.handed && !matches!(raw, Raw::Observation)) {
            return Err(fail(Reason::Stale, raw));
        }
        s.raw = Some(Arc::new(raw));
        Ok(())
    }
    // Explicit retry is limited to the exact prior Storage result and unchanged
    // binding/raw. No re-Prepare, re-Claim, new read, or replacement observation.
    pub(in crate::drafts::store::worker) fn retry_completion(
        &self,
        c: &Completion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        self.exact_completion(&pending, c)?;
        let mut s = c.held.status.lock().unwrap();
        if s.result.as_deref().is_none_or(|v| !v.storage_error()) {
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
        s.submitted = None;
        s.result = None;
        s.receiver = None;
        s.request = None;
        drop(s);
        drop(pending);
        self.submit_completion(c)
    }
    pub(in crate::drafts::store::worker) fn submit_completion(
        &self,
        c: &Completion,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        self.exact_completion(&pending, c)?;
        let mut s = c.held.status.lock().unwrap();
        if s.submitted.is_some() {
            return Ok(());
        }
        if s.request.is_none() {
            if s.building {
                return Err(Reason::Incomplete);
            }
            let binding = s.binding.clone().ok_or(Reason::Incomplete)?;
            let raw = s.raw.clone().ok_or(Reason::Incomplete)?;
            s.building = true;
            drop(s);
            drop(pending);
            let (request, receiver) =
                requests::build(&c.key.route.owner.account(), &binding, &raw)?;
            let mut s = c.held.status.lock().unwrap();
            s.request = Some(request);
            s.receiver = Some(receiver);
            s.building = false;
        } else {
            drop(s);
            drop(pending)
        }
        let mut pending = self.pending.lock().unwrap();
        self.exact_completion(&pending, c)?;
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
}

#[path = "completion_wait.rs"]
mod wait;

#[path = "baseline_deferred.rs"]
mod baseline_deferred;

pub(in crate::drafts::store::worker) use steps::{
    SlashAttemptError, SlashClaimed, SlashCompletion, SlashReserved,
};
