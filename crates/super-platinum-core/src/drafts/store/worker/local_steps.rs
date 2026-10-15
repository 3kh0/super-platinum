//! Closed retained writer, Send consumption and local-handoff prerequisites.
//! No default-route admission, transport capability or drain certificate.
use super::*;
use crate::drafts::sync::ConsumeIntent;

#[derive(Clone)]
pub struct CheckpointInput {
    pub writer: p::Writer,
    pub serial: u64,
    pub expected: p::Review,
    pub local: LocalDraft,
    pub direct: Option<ConsumeIntent>,
}
#[derive(Clone)]
pub struct PreserveInput {
    pub writer: p::Writer,
    pub serial: u64,
    pub local: LocalDraft,
}
#[derive(Clone)]
pub struct SendInput {
    pub stream: p::Stream,
    pub expected: p::Review,
}
#[derive(Clone)]
pub struct ReleaseInput {
    pub expected: p::Review,
    pub writer: p::Writer,
}
pub(in crate::drafts::store::worker) type Registered = Step<p::Review, p::Writer>;
pub(in crate::drafts::store::worker) type Checkpointed = Step<CheckpointInput, p::WriteResult>;
pub(in crate::drafts::store::worker) type Preserved = Step<PreserveInput, p::WriteResult>;
pub(in crate::drafts::store::worker) type SendConsumed = Step<SendInput, p::Review>;
pub(in crate::drafts::store::worker) type SendPrepared = Step<Arc<SendInput>, p::DispatchTicket>;
pub(in crate::drafts::store::worker) type SendClaimed = Step<Arc<SendInput>, p::ClaimedDispatch>;
pub(in crate::drafts::store::worker) type Released = Step<ReleaseInput, p::Release>;

#[derive(Clone)]
pub enum WriteOutcome {
    Applied(p::Review),
    Preserved(p::LocalInventory),
    Retired,
}
impl<I> Step<I, p::WriteResult> {
    pub(in crate::drafts::store::worker) fn write_result(
        &self,
    ) -> Option<Result<WriteOutcome, p::Error>> {
        let mut s = self.held.status.lock().unwrap();
        s.poll();
        s.result.as_ref().map(|r| match r {
            Ok(p::WriteResult::Applied(v)) => Ok(WriteOutcome::Applied(v.clone())),
            Ok(p::WriteResult::Preserved(v)) => Ok(WriteOutcome::Preserved(v.clone())),
            Ok(p::WriteResult::Retired) => Ok(WriteOutcome::Retired),
            Err(e) => Err(*e),
        })
    }
    pub(in crate::drafts::store::worker) fn failed_attempts(&self) -> Vec<(u64, p::Error)> {
        self.held.status.lock().unwrap().failures.clone()
    }
}

fn checkpoint(
    i: &CheckpointInput,
    reply: mpsc::Sender<Result<p::WriteResult, p::Error>>,
) -> Action {
    Action::Checkpoint {
        writer: i.writer.clone(),
        serial: i.serial,
        expected: i.expected.clone(),
        local: i.local.clone(),
        direct: i.direct.clone(),
        reply,
    }
}
fn preserve(i: &PreserveInput, reply: mpsc::Sender<Result<p::WriteResult, p::Error>>) -> Action {
    Action::PreserveVersion {
        writer: i.writer.clone(),
        serial: i.serial,
        local: i.local.clone(),
        reply,
    }
}
impl Worker {
    /// Test-only replay probe of the actual immutable consumed-intent guard.
    /// No public/facade constructor or generic command visibility is widened.
    #[cfg(test)]
    pub(in crate::drafts::store::worker) fn hold_consumed_replay(
        &self,
        consumed: p::Review,
        stream: p::Stream,
    ) -> Result<Step<p::Review, p::DispatchTicket>, Refused<p::Review>> {
        let scope = consumed.scope().clone();
        self.step(&scope, consumed, "test-replay-consumed", |proof, reply| {
            Action::PrepareConsumedSend {
                stream,
                consumed: proof.clone(),
                reply,
            }
        })
    }

    #[cfg(test)]
    pub(in crate::drafts::store::worker) fn hold_local_fault(
        &self,
        scope: Scope,
        point: p::test_support::FaultPoint,
    ) -> Result<Step<p::test_support::FaultPoint, ()>, Refused<p::test_support::FaultPoint>> {
        self.step(
            &scope,
            point,
            "protected-local-commit-fault",
            |point, reply| Action::ArmFault {
                point: *point,
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn hold_register(
        &self,
        input: p::Review,
    ) -> Result<Registered, Refused<p::Review>> {
        if !bounded(&input.scope().account_id, Some(&input.scope().key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.scope().clone();
        self.step(&scope, input, "protected-register-writer", |i, reply| {
            Action::RegisterWriter {
                expected: i.clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn hold_checkpoint(
        &self,
        input: CheckpointInput,
    ) -> Result<Checkpointed, Refused<CheckpointInput>> {
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
        if input.writer.scope() != input.expected.scope() {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Stale,
                input,
            ));
        }
        let scope = input.expected.scope().clone();
        self.step(&scope, input, "protected-checkpoint", checkpoint)
    }
    pub(in crate::drafts::store::worker) fn hold_preserve(
        &self,
        input: PreserveInput,
    ) -> Result<Preserved, Refused<PreserveInput>> {
        if !bounded(
            &input.writer.scope().account_id,
            Some(&input.writer.scope().key),
        ) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.writer.scope().clone();
        self.step(&scope, input, "protected-preserve-version", preserve)
    }
    pub(in crate::drafts::store::worker) fn hold_send(
        &self,
        input: SendInput,
    ) -> Result<SendConsumed, Refused<SendInput>> {
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
                "protected-consume-send",
                "protected-prepare",
                "protected-claim-dispatch",
                "protected-record",
            ],
            |i, reply| Action::Consume {
                stream: i.stream.clone(),
                expected: i.expected.clone(),
                kind: crate::drafts::sync::ConsumeKind::Send,
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn prepare_send(
        &self,
        parent: &SendConsumed,
    ) -> Result<SendPrepared, Reason> {
        let input = parent.input();
        let account = input.expected.scope().account_id.clone();
        let stream = input.stream.clone();
        let (key, held) = self.transfer(
            parent,
            "protected-prepare",
            false,
            |expected, mut reserved| {
                let (reply, receiver) = mpsc::channel();
                Arc::new(StepHeld {
                    changed: Condvar::new(),
                    input: Arc::new(input),
                    status: Mutex::new(StepStatus {
                        request: Some(wrapped(
                            &account,
                            Action::PrepareConsumedSend {
                                stream,
                                consumed: expected,
                                reply,
                            },
                        )),
                        receiver: Some(receiver),
                        result: None,
                        disconnected: false,
                        submitted: None,
                        next: None,
                        reserved: reserved.pop_front(),
                        tail: reserved,
                        failures: Vec::new(),
                    }),
                })
            },
        )?;
        Ok(Step { key, held })
    }
    pub(in crate::drafts::store::worker) fn claim_send(
        &self,
        parent: &SendPrepared,
    ) -> Result<SendClaimed, Reason> {
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
                    input: input.clone(),
                    status: Mutex::new(StepStatus {
                        request: Some(wrapped(&account, Action::ClaimDispatch { ticket, reply })),
                        receiver: Some(receiver),
                        result: None,
                        disconnected: false,
                        submitted: None,
                        next: None,
                        reserved: reserved.pop_front(),
                        tail: reserved,
                        failures: Vec::new(),
                    }),
                })
            },
        )?;
        Ok(Step { key, held })
    }
    pub(in crate::drafts::store::worker) fn hold_release(
        &self,
        input: ReleaseInput,
    ) -> Result<Released, Refused<ReleaseInput>> {
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
        if input.writer.scope() != input.expected.scope()
            || input.writer.enrollment() != input.expected.row().lineage().enrollment()
        {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Stale,
                input,
            ));
        }
        let scope = input.expected.scope().clone();
        self.family(
            &scope,
            input,
            &["protected-release", "protected-start-local"],
            |i, reply| Action::Release {
                expected: i.expected.clone(),
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn retry_checkpoint(
        &self,
        step: &Checkpointed,
    ) -> Result<(), Reason> {
        self.retry_local_storage(step, checkpoint)
    }
    pub(in crate::drafts::store::worker) fn retry_preserve(
        &self,
        step: &Preserved,
    ) -> Result<(), Reason> {
        self.retry_local_storage(step, preserve)
    }
    // Named local retries only. No new-generation or fenced retry authority.
    // The original payload remains held even if submission races a new fence.
    fn retry_local_storage<I: Send + Sync + 'static>(
        &self,
        step: &Step<I, p::WriteResult>,
        build: impl FnOnce(&I, mpsc::Sender<Result<p::WriteResult, p::Error>>) -> Action,
    ) -> Result<(), Reason> {
        let input = step.input();
        let (reply, receiver) = mpsc::channel();
        let request = wrapped(step.key.route.owner.account(), build(&input, reply));
        let mut pending = self.pending.lock().unwrap();
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let entry = a.entry(&step.key)?;
        let actual = entry
            .retained
            .clone()
            .downcast::<StepHeld<I, p::WriteResult>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &step.held) || entry.generation != a.generation {
            return Err(Reason::Stale);
        }
        a.check(step.key.route.owner.account(), false)?;
        let mut s = actual.status.lock().unwrap();
        if !matches!(s.result, Some(Err(p::Error::Storage))) {
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
        s.failures.push((id, p::Error::Storage));
        s.result = None;
        s.submitted = None;
        s.receiver = Some(receiver);
        s.request = Some(request);
        drop(s);
        drop(pending);
        self.submit_step(step)
    }
}
