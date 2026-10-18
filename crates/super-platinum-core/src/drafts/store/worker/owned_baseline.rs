//! Actual remote baseline custody. Read retention and explicit choice are separate.
use super::*;

#[derive(Clone)]
pub struct BaselineInput {
    inner: a::BaselineInput,
}
impl BaselineInput {
    pub fn new(stream: p::Stream, expected: p::Review, now_ms: u64) -> Self {
        Self {
            inner: a::BaselineInput {
                stream,
                expected,
                now_ms,
            },
        }
    }
    pub fn stream(&self) -> &p::Stream {
        &self.inner.stream
    }
    pub fn expected(&self) -> &p::Review {
        &self.inner.expected
    }
    pub fn now_ms(&self) -> u64 {
        self.inner.now_ms
    }
}
#[derive(Clone)]
pub struct BaselineFinishInput {
    evidence: p::BaselineEvidence,
    choice: crate::drafts::store::sync::RemoteChoice,
}
impl BaselineFinishInput {
    pub fn new(
        evidence: p::BaselineEvidence,
        choice: crate::drafts::store::sync::RemoteChoice,
    ) -> Self {
        Self { evidence, choice }
    }
    pub fn evidence(&self) -> &p::BaselineEvidence {
        &self.evidence
    }
    pub fn choice(&self) -> &crate::drafts::store::sync::RemoteChoice {
        &self.choice
    }
}
#[derive(Clone)]
pub struct BaselineReserved {
    queue: Arc<Queue>,
    step: a::ReservedBaseline,
}
#[derive(Clone)]
pub struct BaselineClaimed {
    queue: Arc<Queue>,
    step: a::ClaimedBaseline,
}
#[derive(Clone)]
pub struct BaselineCompletion {
    queue: Arc<Queue>,
    held: a::Completion,
}
#[derive(Clone)]
pub struct BaselineRead {
    pub response: p::BaselineResponse,
    pub known_until: u64,
}
/// Observation only; obtaining this state never takes or creates a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaselineState {
    Ready,
    Deferred(u64),
    ClaimPending { now_ms: u64, submitted: bool },
    Requested,
    ResponseRetained,
    Failed(p::Error),
    Disconnected,
}
/// Shares the exact executor result, never a caller-created positive receipt.
#[derive(Clone)]
pub struct BaselineOutcome {
    value: Arc<a::Finished>,
}
impl BaselineOutcome {
    fn project(value: Arc<a::Finished>) -> Result<Result<Self, p::Error>, AdmissionError> {
        match &*value {
            a::Finished::Baseline(Ok(_)) => Ok(Ok(Self { value })),
            a::Finished::Baseline(Err(e)) => Ok(Err(*e)),
            _ => Err(AdmissionError::Stale),
        }
    }
    /// None is the actual nonpositive read, not an absent/unfinished result.
    /// Evidence remains usable after memory acknowledgment; finish revalidates
    /// its durable lineage, current stream and exact full review independently.
    pub fn evidence(&self) -> Option<p::BaselineEvidence> {
        match &*self.value {
            a::Finished::Baseline(Ok(p::BaselineResult::Retained(e))) => Some(e.clone()),
            a::Finished::Baseline(Ok(p::BaselineResult::Nonpositive)) => None,
            _ => unreachable!("closed baseline result"),
        }
    }
}
pub struct BaselineAcknowledgment {
    completion: BaselineCompletion,
    outcome: BaselineOutcome,
}
impl Operations {
    pub fn baseline(
        &self,
        input: BaselineInput,
    ) -> Result<BaselineReserved, AdmissionRefused<BaselineInput>> {
        self.queue
            .worker
            .hold_baseline(input.inner)
            .map(|step| BaselineReserved {
                queue: self.queue.clone(),
                step,
            })
            .map_err(|r| AdmissionRefused {
                reason: r.reason.into(),
                generation: r.generation,
                input: BaselineInput { inner: r.input },
            })
    }
    pub fn resume_baseline_reserved(
        &self,
        key: &OperationKey,
    ) -> Result<BaselineReserved, AdmissionError> {
        self.queue
            .worker
            .reattach_step(&key.0)
            .map(|step| BaselineReserved {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
    pub fn resume_baseline_claimed(
        &self,
        key: &OperationKey,
    ) -> Result<BaselineClaimed, AdmissionError> {
        self.queue
            .worker
            .reattach_step(&key.0)
            .map(|step| BaselineClaimed {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
    pub fn resume_baseline_completion(
        &self,
        key: &OperationKey,
    ) -> Result<BaselineCompletion, AdmissionError> {
        let held = self.queue.worker.reattach_completion(&key.0)?;
        if !held.is_baseline() {
            return Err(AdmissionError::Stale);
        }
        Ok(BaselineCompletion {
            queue: self.queue.clone(),
            held,
        })
    }
}
root!(
    finish_baseline,
    resume_finish_baseline,
    hold_finish_baseline,
    BaselineFinishInput,
    p::Review
);
impl StorageJob<BaselineFinishInput, p::Review> {
    /// Only a classified Storage failure retries this exact evidence/choice.
    /// Prior failures remain held; only the distinct repaired observation can
    /// acknowledge them after an exact successful retry.
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_finish_baseline(&self.step)
            .map_err(Into::into)
    }
}
macro_rules! issuance {
    ($kind:ident) => {
        impl $kind {
            pub fn key(&self) -> OperationKey {
                OperationKey(self.step.key())
            }
            pub fn state(&self) -> JobState {
                self.step.state().into()
            }
            pub fn submit(&self) -> Result<(), AdmissionError> {
                self.queue
                    .worker
                    .submit_step(&self.step)
                    .map_err(Into::into)
            }
            pub fn wait(&self) -> Result<JobState, AdmissionError> {
                self.queue
                    .worker
                    .wait_step(&self.step)
                    .map(Into::into)
                    .map_err(Into::into)
            }
        }
    };
}
issuance!(BaselineReserved);
issuance!(BaselineClaimed);
impl BaselineReserved {
    pub fn input(&self) -> BaselineInput {
        BaselineInput {
            inner: self.step.input().as_ref().clone(),
        }
    }
    pub fn claim(&self) -> Result<BaselineClaimed, AdmissionError> {
        self.queue
            .worker
            .claim_baseline(&self.step)
            .map(|step| BaselineClaimed {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
}
impl BaselineClaimed {
    pub fn completion(&self) -> Result<BaselineCompletion, AdmissionError> {
        self.queue
            .worker
            .begin_baseline_completion(&self.step)
            .map(|held| BaselineCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl BaselineCompletion {
    pub fn state(&self) -> Result<BaselineState, AdmissionError> {
        self.held.baseline_state().map_err(Into::into)
    }
    pub fn key(&self) -> OperationKey {
        OperationKey(self.held.key())
    }
    pub fn deferred_until(&self) -> Option<u64> {
        self.held.deferred_until()
    }
    /// Explicitly submit ClaimBaseline for the same retained ticket. No timer,
    /// new reservation or request is created before its stored deadline.
    pub fn resume_deferred(&self, now_ms: u64) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .resume_baseline_deferred(&self.held, now_ms)
            .map_err(Into::into)
    }
    /// Off-renderer wait for that explicit claim, including terminal accounting.
    /// Ready means a request can be taken; Deferred carries the next deadline.
    pub fn wait_deferred(&self) -> Result<Result<Option<u64>, p::Error>, AdmissionError> {
        self.queue
            .worker
            .wait_baseline_deferred(&self.held)
            .map_err(Into::into)
    }
    pub fn take_request(&self) -> Result<p::BaselineRequest, AdmissionError> {
        match self.queue.worker.take_dispatch(&self.held)? {
            a::Dispatch::Baseline(request) => Ok(request),
            _ => unreachable!("typed baseline binding"),
        }
    }
    pub fn cancel_before_dispatch(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .cancel_before_dispatch(&self.held)
            .map_err(Into::into)
    }
    pub fn bind(&self, input: BaselineRead) -> Result<(), AdmissionRefused<BaselineRead>> {
        self.queue
            .worker
            .bind_raw(
                &self.held,
                a::Raw::Baseline {
                    response: input.response,
                    known_until: input.known_until,
                },
            )
            .map_err(|r| {
                let a::Raw::Baseline {
                    response,
                    known_until,
                } = r.input
                else {
                    unreachable!()
                };
                AdmissionRefused {
                    reason: r.reason.into(),
                    generation: r.generation,
                    input: BaselineRead {
                        response,
                        known_until,
                    },
                }
            })
    }
    pub fn retained_read(&self) -> Option<BaselineRead> {
        match self.held.raw().as_deref() {
            Some(a::Raw::Baseline {
                response,
                known_until,
            }) => Some(BaselineRead {
                response: response.clone(),
                known_until: *known_until,
            }),
            _ => None,
        }
    }
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_completion(&self.held)
            .map_err(Into::into)
    }
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_completion(&self.held)
            .map_err(Into::into)
    }
    pub fn result(&self) -> Result<Option<Result<BaselineOutcome, p::Error>>, AdmissionError> {
        match self.held.result()? {
            None => Ok(None),
            Some(v) => BaselineOutcome::project(v).map(Some),
        }
    }
    pub fn wait(&self) -> Result<Result<BaselineOutcome, p::Error>, AdmissionError> {
        BaselineOutcome::project(self.queue.worker.wait_completion(&self.held)?)
    }
    /// Taking a result never acknowledges it. Storage-retry/error history is
    /// conservatively retained; only a fully accounted successful family clears.
    pub fn observe_success(
        &self,
    ) -> Result<(BaselineOutcome, BaselineAcknowledgment), AdmissionError> {
        let value = self
            .queue
            .worker
            .observe_baseline_family(&self.held, None, false)?;
        let outcome = BaselineOutcome { value };
        Ok((
            outcome.clone(),
            BaselineAcknowledgment {
                completion: self.clone(),
                outcome,
            },
        ))
    }
}
impl BaselineAcknowledgment {
    pub fn matches(&self, outcome: &BaselineOutcome) -> bool {
        Arc::ptr_eq(&self.outcome.value, &outcome.value)
    }
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.completion.queue.worker.observe_baseline_family(
            &self.completion.held,
            Some(&self.outcome.value),
            true,
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
#[cfg(test)]
#[path = "owned_baseline_tests.rs"]
mod tests;

#[cfg(any(test, feature = "protected-test-support"))]
#[path = "owned_baseline_fixture.rs"]
mod fixture;
#[cfg(any(test, feature = "protected-test-support"))]
pub use fixture::*;

#[path = "owned_baseline_retirement.rs"]
mod retirement;
pub use retirement::*;
