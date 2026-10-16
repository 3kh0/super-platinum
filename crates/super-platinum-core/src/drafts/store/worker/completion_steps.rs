//! Retained issuance. Linear API tickets never leave their actual result/request.
use super::*;
use p::commands::{Action, Command};

pub(in crate::drafts::store::worker) struct Step<I, T> {
    key: Key,
    held: Arc<StepHeld<I, T>>,
}
impl<I, T> Clone for Step<I, T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            held: self.held.clone(),
        }
    }
}
struct ReservedSlot;
struct StepHeld<I, T> {
    input: Arc<I>,
    status: Mutex<StepStatus<T>>,
    changed: Condvar,
}
struct StepStatus<T> {
    request: Option<Request>,
    receiver: Option<mpsc::Receiver<Result<T, p::Error>>>,
    result: Option<Result<T, p::Error>>,
    failures: Vec<(u64, p::Error)>,
    disconnected: bool,
    submitted: Option<u64>,
    next: Option<Key>,
    reserved: Option<Key>,
    tail: std::collections::VecDeque<Key>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::drafts::store::worker) enum StepState {
    Waiting,
    Ready,
    Failed(p::Error),
    Transferred,
    Disconnected,
}
impl<T> StepStatus<T> {
    fn poll(&mut self) -> StepState {
        if self.next.is_some() {
            return StepState::Transferred;
        }
        if self.result.is_none()
            && !self.disconnected
            && let Some(receiver) = &self.receiver
        {
            match receiver.try_recv() {
                Ok(value) => self.result = Some(value),
                Err(mpsc::TryRecvError::Empty) => (),
                Err(mpsc::TryRecvError::Disconnected) => self.disconnected = true,
            }
        }
        match &self.result {
            Some(Ok(_)) => StepState::Ready,
            Some(Err(error)) => StepState::Failed(*error),
            None if self.disconnected => StepState::Disconnected,
            None => StepState::Waiting,
        }
    }
}
impl<I, T> Step<I, T> {
    pub(in crate::drafts::store::worker) fn key(&self) -> Key {
        self.key.clone()
    }
    pub(in crate::drafts::store::worker) fn state(&self) -> StepState {
        self.held.status.lock().unwrap().poll()
    }
    pub(in crate::drafts::store::worker) fn input(&self) -> Arc<I> {
        self.held.input.clone()
    }
}
// Cloneable inventories/capabilities may be inspected; non-Clone issuance tickets
// have no extraction API. They move only in the closed family methods below.
impl<I, T: Clone> Step<I, T> {
    pub(in crate::drafts::store::worker) fn value(&self) -> Result<T, p::Error> {
        let mut s = self.held.status.lock().unwrap();
        s.poll();
        s.result.as_ref().ok_or(p::Error::Pending)?.clone()
    }
}
fn wrapped(account: &str, action: Action) -> Request {
    Request::Protected(Box::new(Command {
        account: account.into(),
        action,
    }))
}
impl Worker {
    pub(super) fn step<I: Send + Sync + 'static, T: Send + 'static>(
        &self,
        scope: &Scope,
        input: I,
        family: &'static str,
        build: impl FnOnce(&I, mpsc::Sender<Result<T, p::Error>>) -> Action,
    ) -> Result<Step<I, T>, Refused<I>> {
        self.family(scope, input, &[family], build)
    }
    pub(super) fn family<I: Send + Sync + 'static, T: Send + 'static>(
        &self,
        scope: &Scope,
        input: I,
        families: &[&'static str],
        build: impl FnOnce(&I, mpsc::Sender<Result<T, p::Error>>) -> Action,
    ) -> Result<Step<I, T>, Refused<I>> {
        self.family_request(scope, input, families, |i, send| {
            wrapped(&scope.account_id, build(i, send))
        })
    }
    fn family_request<I: Send + Sync + 'static, T: Send + 'static>(
        &self,
        scope: &Scope,
        input: I,
        families: &[&'static str],
        build: impl FnOnce(&I, mpsc::Sender<Result<T, p::Error>>) -> Request,
    ) -> Result<Step<I, T>, Refused<I>> {
        self.family_request_at(&scope.account_id, Some(&scope.key), input, families, build)
    }
    // The account inventory has no invented destination; all scoped families
    // retain their original account/key bounds and exact admission path.
    fn family_request_at<I: Send + Sync + 'static, T: Send + 'static>(
        &self,
        account: &str,
        key: Option<&DraftKey>,
        input: I,
        families: &[&'static str],
        build: impl FnOnce(&I, mpsc::Sender<Result<T, p::Error>>) -> Request,
    ) -> Result<Step<I, T>, Refused<I>> {
        let (send, receive) = mpsc::channel();
        let mut pending = self.pending.lock().unwrap();
        if !bounded(account, key) {
            return Err(refusal(&mut pending, Reason::Metadata, input));
        }
        // All finite family slots are reserved atomically BEFORE any Prepare or
        // Reserve command. This reserves holder capacity, not C1 record capacity.
        let reserved = (|| {
            let a = pending.admission.as_mut().ok_or(Reason::Unsupported)?;
            a.check(account, false)?;
            if families.is_empty() || families.len() > 4 {
                return Err(Reason::Stale);
            }
            if a.slots.len() + families.len() > MAX_SLOTS {
                return Err(Reason::Capacity);
            }
            let end = a
                .next
                .checked_add(families.len() as u64)
                .ok_or(Reason::Counter)?;
            let first = a.next + 1;
            a.next = end;
            Ok(first)
        })();
        let first = match reserved {
            Ok(id) => id,
            Err(e) => return Err(refusal(&mut pending, e, input)),
        };
        let held = Arc::new(StepHeld {
            changed: Condvar::new(),
            input: Arc::new(input),
            status: Mutex::new(StepStatus {
                request: None,
                receiver: Some(receive),
                result: None,
                failures: Vec::new(),
                disconnected: false,
                submitted: None,
                next: None,
                reserved: None,
                tail: Default::default(),
            }),
        });
        let a = pending.admission.as_mut().unwrap();
        let key = a.register(first, None, families[0], held.clone());
        for (index, family) in families.iter().enumerate().skip(1) {
            let child = a.register(
                first + index as u64,
                Some(first),
                family,
                Arc::new(ReservedSlot),
            );
            let mut s = held.status.lock().unwrap();
            if index == 1 {
                s.reserved = Some(child)
            } else {
                s.tail.push_back(child)
            }
        }
        drop(pending);
        held.status.lock().unwrap().request = Some(build(&held.input, send));
        Ok(Step { key, held })
    }
    pub(in crate::drafts::store::worker) fn hold_inventory(
        &self,
        account: String,
    ) -> Result<Step<String, Arc<p::Loaded>>, Refused<String>> {
        if !bounded(&account, None) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                account,
            ));
        }
        self.family_request_at(
            &account.clone(),
            None,
            account,
            &["protected-inventory"],
            |account, reply| wrapped(account, Action::Inventory { reply }),
        )
    }
    pub(in crate::drafts::store::worker) fn reattach_step<
        I: Send + Sync + 'static,
        T: Send + 'static,
    >(
        &self,
        key: &Key,
    ) -> Result<Step<I, T>, Reason> {
        let pending = self.pending.lock().unwrap();
        let held = pending
            .admission
            .as_ref()
            .ok_or(Reason::Unsupported)?
            .entry(key)?
            .retained
            .clone()
            .downcast::<StepHeld<I, T>>()
            .map_err(|_| Reason::Stale)?;
        Ok(Step {
            key: key.clone(),
            held,
        })
    }
    pub(in crate::drafts::store::worker) fn submit_step<
        I: Send + Sync + 'static,
        T: Send + 'static,
    >(
        &self,
        step: &Step<I, T>,
    ) -> Result<(), Reason> {
        let mut pending = self.pending.lock().unwrap();
        let actual = pending
            .admission
            .as_ref()
            .ok_or(Reason::Unsupported)?
            .entry(&step.key)?
            .retained
            .clone()
            .downcast::<StepHeld<I, T>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &step.held) {
            return Err(Reason::Stale);
        }
        let mut s = actual.status.lock().unwrap();
        if s.submitted.is_some() {
            return Ok(());
        }
        let request = s.request.take().ok_or(Reason::Incomplete)?;
        match Self::try_submit_locked(&mut pending, &step.key, request) {
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
    // Transfer only under Pending -> exact holder and pre-reserved child.
    // There is no ticket constructor or generic public
    // child factory. A transferred result is never dispatched a second time.
    pub(super) fn transfer<
        I: Send + Sync + 'static,
        T: Send + 'static,
        U: Send + Sync + 'static,
    >(
        &self,
        parent: &Step<I, T>,
        family: &'static str,
        completion: bool,
        build: impl FnOnce(T, std::collections::VecDeque<Key>) -> Arc<U>,
    ) -> Result<(Key, Arc<U>), Reason> {
        let mut pending = self.pending.lock().unwrap();
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let entry = a.entry(&parent.key)?;
        let actual = entry
            .retained
            .clone()
            .downcast::<StepHeld<I, T>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &parent.held) {
            return Err(Reason::Stale);
        }
        let mut s = actual.status.lock().unwrap();
        if let Some(key) = &s.next {
            let held = a
                .entry(key)?
                .retained
                .clone()
                .downcast::<U>()
                .map_err(|_| Reason::Stale)?;
            return Ok((key.clone(), held));
        }
        // Claim is storage-only and already reserved before issuance. Binding it
        // across a fence does not expose dispatch; take_dispatch owns that gate.
        let root = entry.parent.unwrap_or(parent.key.id);
        let reserved = s.reserved.clone().ok_or(Reason::Stale)?;
        let slot = a.entry(&reserved)?;
        if slot.family != family
            || slot.parent != Some(root)
            || completion != s.tail.is_empty()
            || !slot.retained.is::<ReservedSlot>()
        {
            return Err(Reason::Stale);
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
        let a = pending.admission.as_mut().unwrap();
        let value = s.result.take().unwrap().unwrap_or_else(|_| unreachable!());
        // build only moves tokens and makes bounded holders/channels, never
        // serializes/clones rich payload under Pending.
        let held = build(value, std::mem::take(&mut s.tail));
        let key = reserved;
        a.slots.get_mut(&key.id).unwrap().retained = held.clone();
        s.next = Some(key.clone());
        Ok((key, held))
    }
}

#[derive(Clone)]
pub struct MutationInput {
    pub stream: p::Stream,
    pub expected: p::Review,
}
#[derive(Clone)]
pub(in crate::drafts::store::worker) struct CheckInput {
    pub recovery: p::Recovery,
    pub expected: p::Review,
    pub receipt: p::Receipt,
    pub now_ms: u64,
}
#[derive(Clone)]
pub(in crate::drafts::store::worker) struct BaselineInput {
    pub stream: p::Stream,
    pub expected: p::Review,
    pub now_ms: u64,
}
#[derive(Clone)]
pub(in crate::drafts::store::worker) struct ObservationInput {
    pub stream: p::Stream,
    pub expected: p::Review,
    pub event: crate::drafts::sync::DraftObservation,
}
pub(in crate::drafts::store::worker) type Prepared = Step<MutationInput, p::DispatchTicket>;
pub(in crate::drafts::store::worker) type Claimed = Step<Arc<MutationInput>, p::ClaimedDispatch>;
pub(in crate::drafts::store::worker) type ReservedCheck = Step<CheckInput, p::CheckTicket>;
pub(in crate::drafts::store::worker) type ClaimedCheck = Step<Arc<CheckInput>, p::CheckClaim>;
pub(in crate::drafts::store::worker) type ReservedBaseline = Step<BaselineInput, p::BaselineTicket>;
pub(in crate::drafts::store::worker) type ClaimedBaseline =
    Step<Arc<BaselineInput>, p::BaselineClaim>;
pub(in crate::drafts::store::worker) type ReservedObservation =
    Step<ObservationInput, p::ObservationTicket>;

impl Worker {
    pub(in crate::drafts::store::worker) fn hold_prepare(
        &self,
        input: MutationInput,
    ) -> Result<Prepared, Refused<MutationInput>> {
        if !bounded(
            &input.expected.scope().account_id,
            Some(&input.expected.scope().key),
        ) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.expected.scope().clone();
        self.family(
            &scope,
            input,
            &[
                "protected-prepare",
                "protected-claim-dispatch",
                "protected-record",
            ],
            |i, reply| Action::Prepare {
                stream: i.stream.clone(),
                expected: i.expected.clone(),
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn hold_check(
        &self,
        input: CheckInput,
    ) -> Result<ReservedCheck, Refused<CheckInput>> {
        if !bounded(
            &input.expected.scope().account_id,
            Some(&input.expected.scope().key),
        ) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.expected.scope().clone();
        self.family(
            &scope,
            input,
            &[
                "protected-reserve-check",
                "protected-claim-check",
                "protected-complete-check",
            ],
            |i, reply| Action::ReserveCheck {
                recovery: i.recovery.clone(),
                expected: i.expected.clone(),
                receipt: i.receipt.clone(),
                now_ms: i.now_ms,
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn hold_baseline(
        &self,
        input: BaselineInput,
    ) -> Result<ReservedBaseline, Refused<BaselineInput>> {
        if !bounded(
            &input.expected.scope().account_id,
            Some(&input.expected.scope().key),
        ) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.expected.scope().clone();
        self.family(
            &scope,
            input,
            &[
                "protected-reserve-baseline",
                "protected-claim-baseline",
                "protected-retain-baseline",
            ],
            |i, reply| Action::ReserveBaseline {
                stream: i.stream.clone(),
                expected: i.expected.clone(),
                now_ms: i.now_ms,
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn hold_observation(
        &self,
        input: ObservationInput,
    ) -> Result<ReservedObservation, Refused<ObservationInput>> {
        if !bounded(
            &input.expected.scope().account_id,
            Some(&input.expected.scope().key),
        ) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.expected.scope().clone();
        self.family(
            &scope,
            input,
            &[
                "protected-reserve-observation",
                "protected-finish-observation",
            ],
            |i, reply| Action::ReserveObservation {
                stream: i.stream.clone(),
                expected: i.expected.clone(),
                event: i.event.clone(),
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn claim_mutation(
        &self,
        parent: &Prepared,
    ) -> Result<Claimed, Reason> {
        let input = parent.input();
        let account = input.expected.scope().account_id.clone();
        let (key, held) = self.transfer(
            parent,
            "protected-claim-dispatch",
            false,
            |ticket, mut reserved| {
                let (reply, receiver) = mpsc::channel();
                Arc::new(StepHeld {
                    changed: Condvar::new(),
                    input: Arc::new(input),
                    status: Mutex::new(StepStatus {
                        request: Some(wrapped(&account, Action::ClaimDispatch { ticket, reply })),
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
            },
        )?;
        Ok(Step { key, held })
    }
    pub(in crate::drafts::store::worker) fn claim_check(
        &self,
        parent: &ReservedCheck,
    ) -> Result<ClaimedCheck, Reason> {
        let input = parent.input();
        let account = input.expected.scope().account_id.clone();
        let recovery = input.recovery.clone();
        let now_ms = input.now_ms;
        let (key, held) = self.transfer(
            parent,
            "protected-claim-check",
            false,
            |ticket, mut reserved| {
                let (reply, receiver) = mpsc::channel();
                Arc::new(StepHeld {
                    changed: Condvar::new(),
                    input: Arc::new(input),
                    status: Mutex::new(StepStatus {
                        request: Some(wrapped(
                            &account,
                            Action::ClaimCheck {
                                recovery,
                                ticket,
                                now_ms,
                                reply,
                            },
                        )),
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
            },
        )?;
        Ok(Step { key, held })
    }
    pub(in crate::drafts::store::worker) fn claim_baseline(
        &self,
        parent: &ReservedBaseline,
    ) -> Result<ClaimedBaseline, Reason> {
        let input = parent.input();
        let account = input.expected.scope().account_id.clone();
        let now_ms = input.now_ms;
        let (key, held) = self.transfer(
            parent,
            "protected-claim-baseline",
            false,
            |ticket, mut reserved| {
                let (reply, receiver) = mpsc::channel();
                Arc::new(StepHeld {
                    changed: Condvar::new(),
                    input: Arc::new(input),
                    status: Mutex::new(StepStatus {
                        request: Some(wrapped(
                            &account,
                            Action::ClaimBaseline {
                                ticket,
                                now_ms,
                                reply,
                            },
                        )),
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
            },
        )?;
        Ok(Step { key, held })
    }
}

#[path = "local_steps.rs"]
mod local;
pub use local::{
    CheckpointInput, LocalOutcome, LocalWriteInput, PreserveInput, ReleaseInput, SendInput,
    WriteOutcome,
};
pub(in crate::drafts::store::worker) use local::{
    Checkpointed, Preserved, Registered, Released, SendClaimed, SendConsumed, SendPrepared,
};

#[path = "step_wait.rs"]
mod wait;

#[path = "slash_steps.rs"]
mod slash;
pub(in crate::drafts::store::worker) use slash::{SlashClaimed, SlashCompletion, SlashReserved};
