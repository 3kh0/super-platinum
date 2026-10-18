//! Closed Slash facade over the existing Owned worker. No admission constructor,
//! production route, ordinary Send conversion, or HTTP replay capability.
use super::*;
use crate::slash_commands::controller::{ClaimPlan, Record, Report};

/// Exact retained prepared input. Fields and construction stay inside core;
/// content, a catalog choice or an OperationKey cannot mint native admission.
#[derive(Clone)]
pub struct SlashInput {
    inner: Arc<p::slash_runtime::Input>,
}
impl SlashInput {
    pub(crate) fn from_prepared(
        stream: p::Stream,
        expected: p::Review,
        writer: p::Writer,
        serial: u64,
        plan: Arc<ClaimPlan>,
    ) -> Self {
        Self {
            inner: Arc::new(p::slash_runtime::Input {
                stream,
                expected,
                writer,
                serial,
                plan,
            }),
        }
    }
    pub fn review(&self) -> &p::Review {
        &self.inner.expected
    }
    pub fn serial(&self) -> u64 {
        self.inner.serial
    }
    pub fn prior(&self) -> &Record {
        self.inner.plan.prior()
    }
}

#[derive(Clone)]
pub struct SlashReserved {
    queue: Arc<Queue>,
    step: a::SlashReserved,
}
#[derive(Clone)]
pub struct SlashClaimed {
    queue: Arc<Queue>,
    step: a::SlashClaimed,
}
#[derive(Clone)]
pub struct SlashCompletion {
    queue: Arc<Queue>,
    held: a::SlashCompletion,
}

/// Returned evidence remains available even if its owner rejected attachment;
/// Storage means the real report is already retained. Neither authorizes replay.
#[derive(Debug)]
pub enum SlashExecuteError {
    BeforeDispatch(AdmissionError),
    Returned(AdmissionRefused<Report>),
    Storage(AdmissionError),
}
impl From<a::SlashAttemptError> for SlashExecuteError {
    fn from(value: a::SlashAttemptError) -> Self {
        match value {
            a::SlashAttemptError::BeforeDispatch(reason) => Self::BeforeDispatch(reason.into()),
            a::SlashAttemptError::Returned(refused) => Self::Returned(refused.into()),
            a::SlashAttemptError::Storage(reason) => Self::Storage(reason.into()),
        }
    }
}

/// Moves the original committed witness; never Clone, Deserialize or constructible.
/// This is not a native-input proof or an accounting barrier: await completion first.
pub struct SlashFreshClear {
    witness: p::FreshClear,
    completion: SlashCompletion,
}
impl SlashFreshClear {
    pub fn matches(&self, prepared: &Record, terminal: &Record) -> bool {
        self.witness.matches(prepared, terminal)
    }
    pub fn belongs_to(&self, completion: &SlashCompletion) -> bool {
        Arc::ptr_eq(&self.completion.queue, &completion.queue)
            && self.completion.held.same_holder(&completion.held)
    }
    /// Consume exactly once after the host has proved its current native/editor
    /// relation. Refusal returns the original witness for retained evidence.
    pub fn consume(
        self,
        completion: &SlashCompletion,
        prepared: &Record,
        terminal: &Record,
    ) -> Result<(), Self> {
        if self.belongs_to(completion)
            && self.matches(prepared, terminal)
            && completion.held.consume_fresh_clear()
        {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl Operations {
    pub fn slash(&self, input: SlashInput) -> Result<SlashReserved, AdmissionRefused<SlashInput>> {
        self.queue
            .worker
            .hold_slash(input.inner)
            .map(|step| SlashReserved {
                queue: self.queue.clone(),
                step,
            })
            .map_err(|refused| AdmissionRefused {
                reason: refused.reason.into(),
                generation: refused.generation,
                input: SlashInput {
                    inner: refused.input,
                },
            })
    }
    pub fn resume_slash_reserved(
        &self,
        key: &OperationKey,
    ) -> Result<SlashReserved, AdmissionError> {
        self.queue
            .worker
            .reattach_step(&key.0)
            .map(|step| SlashReserved {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
    pub fn resume_slash_claimed(&self, key: &OperationKey) -> Result<SlashClaimed, AdmissionError> {
        self.queue
            .worker
            .reattach_step(&key.0)
            .map(|step| SlashClaimed {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
    pub fn resume_slash_completion(
        &self,
        key: &OperationKey,
    ) -> Result<SlashCompletion, AdmissionError> {
        self.queue
            .worker
            .resume_slash_completion(&key.0)
            .map(|held| SlashCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}

macro_rules! storage {
    ($name:ident) => {
        impl $name {
            pub fn key(&self) -> OperationKey {
                OperationKey(self.step.key())
            }
            pub fn input(&self) -> SlashInput {
                SlashInput {
                    inner: self.step.input().as_ref().clone(),
                }
            }
            pub fn state(&self) -> JobState {
                self.step.state().into()
            }
            /// Memory-only submission of this exact retained storage step.
            pub fn submit(&self) -> Result<(), AdmissionError> {
                self.queue
                    .worker
                    .submit_step(&self.step)
                    .map_err(Into::into)
            }
            /// Blocking off-renderer wait for this step's actual accounting
            /// terminal. Does not submit or establish account-wide quiescence.
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
storage!(SlashReserved);
storage!(SlashClaimed);

impl SlashReserved {
    pub fn claim(&self) -> Result<SlashClaimed, AdmissionError> {
        self.queue
            .worker
            .claim_slash(&self.step)
            .map(|step| SlashClaimed {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
}
impl SlashClaimed {
    pub fn completion(&self) -> Result<SlashCompletion, AdmissionError> {
        self.queue
            .worker
            .begin_slash_completion(&self.step)
            .map(|held| SlashCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl SlashCompletion {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.held.key())
    }
    pub fn input(&self) -> SlashInput {
        SlashInput {
            inner: self.held.input(),
        }
    }
    /// Result observation only; not an accounting barrier or drain certificate.
    pub fn state(&self) -> JobState {
        self.held.state().into()
    }
    /// Blocking off-renderer wait for this retained storage attempt and its
    /// actual accounting terminal. Never submits, dispatches or certifies drain.
    /// An unsubmitted holder returns Waiting; lost delivery returns Disconnected.
    pub fn wait(&self) -> Result<JobState, AdmissionError> {
        self.queue
            .worker
            .wait_slash_completion(&self.held)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn report(&self) -> Option<Arc<Report>> {
        self.held.report()
    }
    pub fn terminal(&self) -> Option<Record> {
        self.held.terminal()
    }
    /// Moves the actual newly committed witness once across all cloned/resumed
    /// handles. The caller must still prove its exact live native composer.
    pub fn take_fresh_clear(&self) -> Option<SlashFreshClear> {
        self.held.take_fresh_clear().map(|witness| SlashFreshClear {
            witness,
            completion: self.clone(),
        })
    }
    /// Retain/poll this entire future in the desktop operation owner. Cancellation
    /// after handoff leaves DispatchPossible, never authority to issue again.
    /// A successful return means completion submitted, not settlement committed.
    pub async fn execute(&self) -> Result<(), SlashExecuteError> {
        self.queue
            .worker
            .execute_slash(&self.held)
            .await
            .map_err(Into::into)
    }
    /// Retry submission of already retained evidence only; no HTTP or new claim.
    pub fn submit_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_slash_completion(&self.held)
            .map_err(Into::into)
    }
    /// Only a recorded Storage failure can retry its same retained settlement.
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_slash_completion(&self.held)
            .map_err(Into::into)
    }
    #[cfg(test)]
    async fn execute_fixture<F, Fut>(&self, send: F) -> Result<(), SlashExecuteError>
    where
        F: FnOnce(crate::slash_commands::controller::Permit) -> Fut,
        Fut: std::future::Future<Output = Report>,
    {
        self.queue
            .worker
            .fixture_execute_slash(&self.held, send)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "owned_slash_tests.rs"]
mod tests;

#[cfg(any(test, feature = "protected-test-support"))]
#[path = "owned_slash_fixture.rs"]
mod fixture;
#[cfg(any(test, feature = "protected-test-support"))]
pub use fixture::*;

/// Non-Clone receipt for an observed acknowledged command. Drop retains custody.
pub struct SlashAcknowledgment {
    completion: SlashCompletion,
}
impl SlashCompletion {
    pub fn observe_acknowledged(&self) -> Result<(Record, SlashAcknowledgment), AdmissionError> {
        let record = self.queue.worker.observe_slash_family(&self.held, false)?;
        Ok((
            record,
            SlashAcknowledgment {
                completion: self.clone(),
            },
        ))
    }
}
impl SlashAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self
            .completion
            .queue
            .worker
            .observe_slash_family(&self.completion.held, true)
        {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}

#[path = "owned_slash_archive.rs"]
mod archive;
pub use archive::*;

#[path = "owned_slash_context.rs"]
mod context;
pub use context::{SlashContext, SlashDiscoveryError};
