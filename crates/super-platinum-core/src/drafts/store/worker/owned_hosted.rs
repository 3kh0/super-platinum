//! Typed hosted-file route over the actual Owned worker and retained completion.
use super::*;
use crate::config::WorkspaceSession;
use crate::slack::file_share_policy::{self as policy, PostingFacts};
use crate::slack::{api::file_share::batch, client::SlackClient, transport::Transport};
pub use p::hosted_files::{
    Input as HostedInput, Record as HostedRecord, Returned as HostedReturned,
    editor::Draft as HostedDraft,
    runtime::{Prepared as HostedPrepared, SaveDraft as HostedDraftInput},
};
use serde_json::Value;
#[path = "owned_hosted_retirement.rs"]
mod retirement;
pub use p::hosted_files::cleanup::retirement::{
    Disposition as HostedRetirementDisposition, Retired as HostedRetired,
    Retirement as HostedRetirement,
};
pub use retirement::HostedRetirementAcknowledgment;

/// Fresh canonical observations for this exact set. This is deliberately not
/// serializable/cloneable. Host must recheck its native/account/source owner
/// immediately before awaiting execute; no source is fetched by this value.
pub struct HostedFresh {
    input: Arc<HostedPrepared>,
}
impl HostedFresh {
    pub fn new(
        input: Arc<HostedPrepared>,
        current: &HostedInput,
        destination: &Value,
        facts: &PostingFacts,
        peer: Option<&Value>,
    ) -> Result<Self, AdmissionError> {
        if current != input.input()
            || destination.get("id").and_then(Value::as_str)
                != Some(input.review().scope().key.channel.as_str())
            || policy::destination(destination, facts, peer) != policy::Eligibility::Allowed
        {
            return Err(AdmissionError::Stale);
        }
        Ok(Self { input })
    }
}
#[derive(Clone)]
pub struct HostedClaimed {
    queue: Arc<Queue>,
    step: a::HostedClaimed,
}
#[derive(Clone)]
pub struct HostedCompletion {
    queue: Arc<Queue>,
    held: a::HostedCompletion,
}
pub enum HostedExecuteError {
    BeforeDispatch(AdmissionError),
    Returned(AdmissionRefused<HostedReturned>),
    Storage(AdmissionError),
}
impl std::fmt::Debug for HostedExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostedExecuteError(..)")
    }
}
impl Operations {
    pub fn save_hosted_draft(
        &self,
        input: HostedDraftInput,
    ) -> Result<StorageJob<HostedDraftInput, Option<HostedDraft>>, AdmissionRefused<HostedDraftInput>>
    {
        self.queue
            .worker
            .hold_hosted_draft(input)
            .map(|s| self.wrap(s))
            .map_err(Into::into)
    }
    pub fn load_hosted_draft(
        &self,
        scope: Scope,
    ) -> Result<StorageJob<Scope, Option<HostedDraft>>, AdmissionRefused<Scope>> {
        self.queue
            .worker
            .hold_hosted_draft_load(scope)
            .map(|s| self.wrap(s))
            .map_err(Into::into)
    }
    pub fn resume_hosted_draft_save(
        &self,
        key: &OperationKey,
    ) -> Result<StorageJob<HostedDraftInput, Option<HostedDraft>>, AdmissionError> {
        self.resume(key)
    }
    pub fn resume_hosted_draft_load(
        &self,
        key: &OperationKey,
    ) -> Result<StorageJob<Scope, Option<HostedDraft>>, AdmissionError> {
        self.resume(key)
    }
    pub fn hosted(
        &self,
        input: Arc<HostedPrepared>,
    ) -> Result<HostedClaimed, AdmissionRefused<Arc<HostedPrepared>>> {
        self.queue
            .worker
            .hold_hosted(input)
            .map(|step| HostedClaimed {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
    pub fn resume_hosted_claimed(
        &self,
        key: &OperationKey,
    ) -> Result<HostedClaimed, AdmissionError> {
        self.queue
            .worker
            .reattach_step(&key.0)
            .map(|step| HostedClaimed {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
    pub fn resume_hosted_completion(
        &self,
        key: &OperationKey,
    ) -> Result<HostedCompletion, AdmissionError> {
        self.queue
            .worker
            .resume_hosted_completion(&key.0)
            .map(|held| HostedCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl HostedClaimed {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.step.key())
    }
    pub fn input(&self) -> Arc<HostedPrepared> {
        self.step.input().as_ref().clone()
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
    pub fn state(&self) -> JobState {
        self.step.state().into()
    }
    pub fn completion(&self) -> Result<HostedCompletion, AdmissionError> {
        self.queue
            .worker
            .begin_hosted_completion(&self.step)
            .map(|held| HostedCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl HostedCompletion {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.held.key())
    }
    pub fn input(&self) -> Arc<HostedPrepared> {
        self.held.input()
    }
    pub fn state(&self) -> JobState {
        self.held.state().into()
    }
    pub fn result(&self) -> Option<Result<HostedRecord, p::Error>> {
        self.held.result()
    }
    pub fn returned(&self) -> Option<Arc<HostedReturned>> {
        self.held.returned()
    }
    pub fn wait(&self) -> Result<JobState, AdmissionError> {
        self.queue
            .worker
            .wait_hosted_completion(&self.held)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn submit_returned(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_hosted_completion(&self.held)
            .map_err(Into::into)
    }
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_hosted_completion(&self.held)
            .map_err(Into::into)
    }
    /// Synchronous final admission. The caller releases its native read-only
    /// lease immediately after this returns, before awaiting the transport.
    pub fn arm(
        &self,
        fresh: HostedFresh,
        session: &WorkspaceSession,
    ) -> Result<HostedAttempt, HostedExecuteError> {
        let input = self.held.input();
        if !Arc::ptr_eq(&input, &fresh.input) || session.team_id != input.review().scope().key.team
        {
            return Err(HostedExecuteError::BeforeDispatch(AdmissionError::Stale));
        }
        let intent = input
            .input()
            .request()
            .map_err(|_| HostedExecuteError::BeforeDispatch(AdmissionError::Unsupported))?;
        let request = batch::existing_files_share(&SlackClient::default(), session, &intent);
        self.queue
            .worker
            .arm_hosted(&self.held)
            .map_err(|e| HostedExecuteError::BeforeDispatch(e.into()))?;
        Ok(HostedAttempt {
            completion: self.clone(),
            request,
        })
    }
    pub async fn execute(
        &self,
        fresh: HostedFresh,
        transport: &Transport,
        session: &WorkspaceSession,
    ) -> Result<(), HostedExecuteError> {
        self.arm(fresh, session)?.execute(transport).await
    }
    #[cfg(test)]
    fn fixture_return(&self, result: HostedReturned) -> Result<(), HostedExecuteError> {
        self.queue
            .worker
            .arm_hosted(&self.held)
            .map_err(|e| HostedExecuteError::BeforeDispatch(e.into()))?;
        self.queue
            .worker
            .bind_hosted_returned(&self.held, result)
            .map_err(|r| HostedExecuteError::Returned(r.into()))?;
        self.submit_returned().map_err(HostedExecuteError::Storage)
    }
}

#[cfg(test)]
#[path = "owned_hosted_tests.rs"]
mod tests;

#[path = "owned_hosted_cleanup.rs"]
mod cleanup;
pub use cleanup::*;
#[path = "owned_hosted_draft_retry.rs"]
mod draft_retry;
pub use draft_retry::HostedDraftAcknowledgment;

/// One consumed actual request. Neither Clone nor serialization can recover it.
/// Dropping/canceling it after arm leaves the original durable claim uncertain.
pub struct HostedAttempt {
    completion: HostedCompletion,
    request: crate::slack::PreparedRequest,
}
impl HostedAttempt {
    pub async fn execute(self, transport: &Transport) -> Result<(), HostedExecuteError> {
        let returned = match transport.execute_draft_mutation(self.request).await {
            Ok(value) => HostedReturned::Response(value),
            Err(crate::slack::Error::RateLimited {
                retry_after_secs: Some(n),
            }) if n > 0 => HostedReturned::RateLimited {
                retry_after_secs: n,
            },
            Err(_) => HostedReturned::Unknown,
        };
        self.completion
            .queue
            .worker
            .bind_hosted_returned(&self.completion.held, returned)
            .map_err(|r| HostedExecuteError::Returned(r.into()))?;
        self.completion
            .submit_returned()
            .map_err(HostedExecuteError::Storage)
    }
}

#[cfg(any(test, feature = "protected-test-support"))]
#[path = "owned_hosted_fixture.rs"]
mod fixture;
#[cfg(any(test, feature = "protected-test-support"))]
pub use fixture::*;
