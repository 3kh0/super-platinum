//! Explicit initialized Owned route. No default Client conversion or drain API.
use super::super::admission as a;
use super::*;
use crate::drafts::store::protected as p;

pub use a::{
    CheckpointInput, MutationInput, OpenInput, PreserveInput, ReleaseInput, SendInput, TeamInput,
    WriteOutcome,
};

/// Admission refusal is never a durable Error and must not trigger Preserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    Fencing,
    Unsupported,
    Stale,
    Capacity,
    Counter,
    Metadata,
    Incomplete,
    Dead,
    ColdRequired,
}
impl From<a::Reason> for AdmissionError {
    fn from(value: a::Reason) -> Self {
        match value {
            a::Reason::Fencing => Self::Fencing,
            a::Reason::Unsupported => Self::Unsupported,
            a::Reason::Stale => Self::Stale,
            a::Reason::Capacity => Self::Capacity,
            a::Reason::Counter => Self::Counter,
            a::Reason::Metadata => Self::Metadata,
            a::Reason::Incomplete => Self::Incomplete,
            a::Reason::Dead => Self::Dead,
            a::Reason::ColdRequired => Self::ColdRequired,
        }
    }
}
pub struct AdmissionRefused<I> {
    pub reason: AdmissionError,
    pub generation: Option<u64>,
    pub input: I,
}
impl<I> std::fmt::Debug for AdmissionRefused<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmissionRefused")
            .field("reason", &self.reason)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}
impl<I> From<a::Refused<I>> for AdmissionRefused<I> {
    fn from(v: a::Refused<I>) -> Self {
        Self {
            reason: v.reason.into(),
            generation: v.generation,
            input: v.input,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Waiting,
    Ready,
    Failed(p::Error),
    Transferred,
    Disconnected,
}
impl From<a::StepState> for JobState {
    fn from(v: a::StepState) -> Self {
        match v {
            a::StepState::Waiting => Self::Waiting,
            a::StepState::Ready => Self::Ready,
            a::StepState::Failed(e) => Self::Failed(e),
            a::StepState::Transferred => Self::Transferred,
            a::StepState::Disconnected => Self::Disconnected,
        }
    }
}
/// Opaque actual-worker key. No caller-supplied ID or deserialization constructor.
#[derive(Clone)]
pub struct OperationKey(a::Key);
#[derive(Clone)]
pub struct RetainedOperation {
    pub key: OperationKey,
    pub family: &'static str,
    pub generation: u64,
}
#[derive(Clone)]
pub struct Operations {
    queue: Arc<Queue>,
}
impl Operations {
    pub(in crate::drafts::store) fn from_initialized(queue: Arc<Queue>) -> Self {
        Self { queue }
    }
    pub fn account(&self) -> &str {
        self.queue.account()
    }
    pub fn retained(&self) -> Vec<RetainedOperation> {
        self.queue
            .worker
            .retained_keys()
            .into_iter()
            .map(|v| RetainedOperation {
                key: OperationKey(v.key),
                family: v.family,
                generation: v.generation,
            })
            .collect()
    }
    fn wrap<I, T>(&self, step: a::Step<I, T>) -> StorageJob<I, T> {
        StorageJob {
            queue: self.queue.clone(),
            step,
        }
    }
    fn resume<I: Send + Sync + 'static, T: Send + 'static>(
        &self,
        key: &OperationKey,
    ) -> Result<StorageJob<I, T>, AdmissionError> {
        self.queue
            .worker
            .reattach_step(&key.0)
            .map(|s| self.wrap(s))
            .map_err(Into::into)
    }
}
/// The constructor is private; generic arguments cannot create queue authority.
/// The queue owns the exact input/result independently of this UI handle.
pub struct StorageJob<I, T> {
    queue: Arc<Queue>,
    step: a::Step<I, T>,
}
impl<I, T> Clone for StorageJob<I, T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            step: self.step.clone(),
        }
    }
}
impl<I, T> StorageJob<I, T> {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.step.key())
    }
    pub fn input(&self) -> Arc<I> {
        self.step.input()
    }
    pub fn state(&self) -> JobState {
        self.step.state().into()
    }
}
impl<I: Send + Sync + 'static, T: Send + 'static> StorageJob<I, T> {
    /// Blocking: call only from a retained off-renderer task. This neither
    /// submits nor retries. A concurrent state()/input() never waits for recv.
    /// Waits for the actual matching accounting terminal, not just reply send.
    pub fn wait(&self) -> Result<JobState, AdmissionError> {
        self.queue
            .worker
            .wait_step(&self.step)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_step(&self.step)
            .map_err(Into::into)
    }
}
impl<I, T: Clone> StorageJob<I, T> {
    pub fn result(&self) -> Option<Result<T, p::Error>> {
        match self.step.state() {
            a::StepState::Ready | a::StepState::Failed(_) => Some(self.step.value()),
            _ => None,
        }
    }
}
macro_rules! root {
    ($hold:ident,$resume:ident,$internal:ident,$input:ty,$out:ty) => {
        impl Operations {
            pub fn $hold(
                &self,
                input: $input,
            ) -> Result<StorageJob<$input, $out>, AdmissionRefused<$input>> {
                self.queue
                    .worker
                    .$internal(input)
                    .map(|s| self.wrap(s))
                    .map_err(Into::into)
            }
            pub fn $resume(
                &self,
                key: &OperationKey,
            ) -> Result<StorageJob<$input, $out>, AdmissionError> {
                self.resume(key)
            }
        }
    };
}
root!(
    local_inventory,
    resume_local_inventory,
    hold_local_inventory,
    Scope,
    p::LocalInventory
);
root!(review, resume_review, hold_managed_review, Scope, p::Review);
root!(
    enroll,
    resume_enroll,
    hold_enroll,
    p::LocalInventory,
    p::Review
);
root!(
    stream_state,
    resume_stream_state,
    hold_stream_state,
    TeamInput,
    p::StreamState
);
root!(
    open_stream,
    resume_open_stream,
    hold_open_stream,
    OpenInput,
    p::Stream
);
root!(
    verify_local,
    resume_verify_local,
    hold_verify,
    MutationInput,
    p::Review
);
root!(
    register,
    resume_register,
    hold_register,
    p::Review,
    p::Writer
);
root!(
    checkpoint,
    resume_checkpoint,
    hold_checkpoint,
    CheckpointInput,
    p::WriteResult
);
root!(
    preserve,
    resume_preserve,
    hold_preserve,
    PreserveInput,
    p::WriteResult
);
root!(
    consume_send,
    resume_consume_send,
    hold_send,
    SendInput,
    p::Review
);
root!(
    release,
    resume_release,
    hold_release,
    ReleaseInput,
    p::Release
);

impl<I> StorageJob<I, p::WriteResult> {
    pub fn write_result(&self) -> Option<Result<WriteOutcome, p::Error>> {
        self.step.write_result()
    }
    pub fn failed_attempts(&self) -> Vec<(u64, p::Error)> {
        self.step.failed_attempts()
    }
}
impl StorageJob<CheckpointInput, p::WriteResult> {
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_checkpoint(&self.step)
            .map_err(Into::into)
    }
}
impl StorageJob<PreserveInput, p::WriteResult> {
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_preserve(&self.step)
            .map_err(Into::into)
    }
}
impl StorageJob<SendInput, p::Review> {
    /// Caller observes the actual consumed Review before applying native clear.
    pub fn prepare(&self) -> Result<StorageJob<Arc<SendInput>, p::DispatchTicket>, AdmissionError> {
        self.queue
            .worker
            .prepare_send(&self.step)
            .map(|step| StorageJob {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
}
impl StorageJob<Arc<SendInput>, p::DispatchTicket> {
    pub fn claim(&self) -> Result<StorageJob<Arc<SendInput>, p::ClaimedDispatch>, AdmissionError> {
        self.queue
            .worker
            .claim_send(&self.step)
            .map(|step| StorageJob {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
}
impl StorageJob<Arc<SendInput>, p::ClaimedDispatch> {
    pub fn completion(&self) -> Result<MutationCompletion, AdmissionError> {
        self.queue
            .worker
            .begin_send_record(&self.step)
            .map(|held| MutationCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl StorageJob<ReleaseInput, p::Release> {
    pub fn start_local(&self) -> Result<LocalCompletion, AdmissionError> {
        self.queue
            .worker
            .begin_start_local(&self.step)
            .map(|held| LocalCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl Operations {
    pub fn resume_send_prepared(
        &self,
        key: &OperationKey,
    ) -> Result<StorageJob<Arc<SendInput>, p::DispatchTicket>, AdmissionError> {
        self.resume(key)
    }
    pub fn resume_send_claimed(
        &self,
        key: &OperationKey,
    ) -> Result<StorageJob<Arc<SendInput>, p::ClaimedDispatch>, AdmissionError> {
        self.resume(key)
    }
}

#[path = "owned_completions.rs"]
mod completions;
pub use completions::*;

#[cfg(test)]
#[path = "owned_operations_tests.rs"]
mod tests;
