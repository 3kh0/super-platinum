//! Inactive adapter for an already registered exact storage completion.
//! A renderer-root operation must retain this handle AND its actual future before
//! polling. Dropped UI waiters cannot retry HTTP; raw evidence survives storage
//! refusal. This is not a C3 operation registration or account drain certificate.
use super::*;
use super_platinum_core::drafts::store::protected::{
    self as durable,
    owned_store::{AdmissionError, AdmissionRefused, MutationCompletion, MutationOutcome},
};
use super_platinum_core::slack::transport::ProtectedDraftFailure;

#[derive(Default)]
struct State {
    started: bool,
    finished: bool,
    start_error: Option<AdmissionError>,
    attempt: Option<Arc<Attempt>>,
    refusal: Option<AdmissionRefused<MutationOutcome>>,
}
#[derive(Clone)]
pub(crate) struct RegisteredMutation {
    completion: MutationCompletion,
    state: Arc<Mutex<State>>,
}
impl RegisteredMutation {
    pub(crate) fn hold(completion: MutationCompletion) -> Self {
        Self {
            completion,
            state: Arc::new(Mutex::new(State::default())),
        }
    }
    fn take(&self) -> Result<durable::ClaimedDispatch, AdmissionError> {
        let mut state = self.state.lock().map_err(|_| AdmissionError::Incomplete)?;
        if state.started {
            return Err(AdmissionError::Stale);
        }
        state.started = true;
        drop(state);
        self.completion.take_claimed().inspect_err(|error| {
            if let Ok(mut state) = self.state.lock() {
                state.start_error = Some(*error);
            }
        })
    }
    pub(crate) async fn run<F: Fn() -> bool>(
        &self,
        binding: &Binding,
        now: u64,
        current: F,
    ) -> Result<(), AdmissionError> {
        let claim = self.take()?;
        let attempt = binding.execute_claimed(claim, now, current).await;
        self.finish(attempt)
    }
    #[cfg(test)]
    pub(crate) async fn run_with<F, Fut>(
        &self,
        binding: &Binding,
        now: u64,
        send: F,
    ) -> Result<(), AdmissionError>
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, ProtectedDraftFailure>>,
    {
        let claim = self.take()?;
        let attempt = binding.execute_claimed_with(claim, now, send).await;
        self.finish(attempt)
    }
    fn finish(&self, attempt: Attempt) -> Result<(), AdmissionError> {
        let attempt = Arc::new(attempt);
        // Retain the complete decoder/transport evidence before trying storage
        // admission, including unsupported raw fields and real dispatch status.
        self.state
            .lock()
            .map_err(|_| AdmissionError::Incomplete)?
            .attempt = Some(attempt.clone());
        let input = MutationOutcome {
            outcome: outcome(&attempt.outcome),
            known_until: attempt.cooldown_until_ms.unwrap_or(0),
        };
        let refusal = self.completion.bind(input).err();
        let mut state = self.state.lock().map_err(|_| AdmissionError::Incomplete)?;
        state.finished = true;
        state.refusal = refusal;
        match &state.refusal {
            Some(e) => Err(e.reason),
            None => Ok(()),
        }
    }
    pub(crate) fn attempt(&self) -> Option<Arc<Attempt>> {
        self.state.lock().ok()?.attempt.clone()
    }
    pub(crate) fn completion(&self) -> &MutationCompletion {
        &self.completion
    }
    pub(crate) fn refused_outcome(&self) -> Option<MutationOutcome> {
        self.state
            .lock()
            .ok()?
            .refusal
            .as_ref()
            .map(|r| r.input.clone())
    }
    pub(crate) fn commit(&self) -> Result<(), AdmissionError> {
        let state = self.state.lock().map_err(|_| AdmissionError::Incomplete)?;
        if !state.finished {
            return Err(AdmissionError::Incomplete);
        }
        if let Some(e) = &state.refusal {
            return Err(e.reason);
        }
        drop(state);
        self.completion.submit()
    }
    /// Only storage is retried. Neither the claim nor HTTP closure can be taken
    /// again, including after an interrupted future or a lost successful result.
    pub(crate) fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.completion.retry_storage()
    }
}
fn outcome(value: &Outcome) -> durable::Outcome {
    match value {
        Outcome::Draft(v) => durable::Outcome::Active(v.clone()),
        Outcome::Deleted { draft_id } => durable::Outcome::Deleted {
            draft_id: draft_id.clone(),
        },
        Outcome::Sent {
            client_msg_id,
            draft_id,
            ts,
        } => durable::Outcome::Sent {
            client_msg_id: client_msg_id.clone(),
            draft_id: draft_id.clone(),
            ts: ts.clone(),
        },
        Outcome::Scheduled { draft, post_at } => durable::Outcome::Scheduled {
            remote: draft.clone(),
            post_at: *post_at,
        },
        Outcome::Rejected => durable::Outcome::Rejected,
        Outcome::Uncertain => durable::Outcome::Uncertain,
    }
}
#[cfg(test)]
#[path = "owned_attempt_tests.rs"]
mod tests;
