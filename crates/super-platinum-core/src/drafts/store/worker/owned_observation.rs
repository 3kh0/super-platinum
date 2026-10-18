//! Ordered observation storage only. Tickets never leave the retained worker.
use super::*;

/// A caller may supply observed content, but only actual Stream/Review values
/// can admit it. The existing executor validates ordering, identity and content.
#[derive(Clone)]
pub struct ObservationInput {
    inner: a::ObservationInput,
}
impl ObservationInput {
    pub fn new(
        stream: p::Stream,
        expected: p::Review,
        event: crate::drafts::sync::DraftObservation,
    ) -> Self {
        Self {
            inner: a::ObservationInput {
                stream,
                expected,
                event,
            },
        }
    }
    pub fn stream(&self) -> &p::Stream {
        &self.inner.stream
    }
    pub fn expected(&self) -> &p::Review {
        &self.inner.expected
    }
    pub fn event(&self) -> &crate::drafts::sync::DraftObservation {
        &self.inner.event
    }
}

#[derive(Clone)]
pub struct ObservationReserved {
    queue: Arc<Queue>,
    step: a::ReservedObservation,
}
#[derive(Clone)]
pub struct ObservationCompletion {
    queue: Arc<Queue>,
    held: a::Completion,
}
/// Read-only view of the exact retained executor result. Cloning this wrapper
/// shares evidence; it creates no new ticket, result or completion authority.
#[derive(Clone)]
pub struct ObservationOutcome {
    value: Arc<a::Finished>,
}
impl ObservationOutcome {
    fn project(value: Arc<a::Finished>) -> Result<Result<Self, p::Error>, AdmissionError> {
        match &*value {
            a::Finished::Observation(Ok(_)) => Ok(Ok(Self { value })),
            a::Finished::Observation(Err(error)) => Ok(Err(*error)),
            _ => Err(AdmissionError::Stale),
        }
    }
    fn actual(&self) -> &p::ObservationResult {
        match &*self.value {
            a::Finished::Observation(Ok(result)) => result,
            _ => unreachable!("closed successful observation result"),
        }
    }
    pub fn before(&self) -> &p::Review {
        self.actual().before()
    }
    pub fn after(&self) -> &p::Review {
        self.actual().after()
    }
}
/// Dropping this receipt preserves the complete family. Acknowledge only after
/// handling the exact before/after projection, including a truthful no-op.
pub struct ObservationAcknowledgment {
    completion: ObservationCompletion,
    outcome: ObservationOutcome,
}
impl Operations {
    pub fn observe(
        &self,
        input: ObservationInput,
    ) -> Result<ObservationReserved, AdmissionRefused<ObservationInput>> {
        self.queue
            .worker
            .hold_observation(input.inner)
            .map(|step| ObservationReserved {
                queue: self.queue.clone(),
                step,
            })
            .map_err(|r| AdmissionRefused {
                reason: r.reason.into(),
                generation: r.generation,
                input: ObservationInput { inner: r.input },
            })
    }
    pub fn resume_observation_reserved(
        &self,
        key: &OperationKey,
    ) -> Result<ObservationReserved, AdmissionError> {
        self.queue
            .worker
            .reattach_step(&key.0)
            .map(|step| ObservationReserved {
                queue: self.queue.clone(),
                step,
            })
            .map_err(Into::into)
    }
    pub fn resume_observation_completion(
        &self,
        key: &OperationKey,
    ) -> Result<ObservationCompletion, AdmissionError> {
        let held = self
            .queue
            .worker
            .reattach_completion(&key.0)
            .map_err(AdmissionError::from)?;
        if !held.is_observation() {
            return Err(AdmissionError::Stale);
        }
        Ok(ObservationCompletion {
            queue: self.queue.clone(),
            held,
        })
    }
}
impl ObservationReserved {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.step.key())
    }
    pub fn input(&self) -> ObservationInput {
        ObservationInput {
            inner: self.step.input().as_ref().clone(),
        }
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
    /// Off-renderer actual reply plus accounting wait; never submits a job.
    pub fn wait(&self) -> Result<JobState, AdmissionError> {
        self.queue
            .worker
            .wait_step(&self.step)
            .map(Into::into)
            .map_err(Into::into)
    }
    /// Transfers the actual issued ticket to its pre-reserved child once.
    /// Repeated calls reattach the same child; no caller-supplied ticket exists.
    pub fn finish(&self) -> Result<ObservationCompletion, AdmissionError> {
        self.queue
            .worker
            .begin_observation_completion(&self.step)
            .map(|held| ObservationCompletion {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl ObservationCompletion {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.held.key())
    }
    /// Bind the closed storage-only operation and submit its retained request.
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.queue.worker.bind_observation_completion(&self.held)?;
        self.queue
            .worker
            .submit_completion(&self.held)
            .map_err(Into::into)
    }
    /// Only an actual Storage error may retry the exact already-issued ticket.
    /// Retry history remains retained and is excluded from success retirement.
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_completion(&self.held)
            .map_err(Into::into)
    }
    pub fn result(&self) -> Result<Option<Result<ObservationOutcome, p::Error>>, AdmissionError> {
        match self.held.result().map_err(AdmissionError::from)? {
            None => Ok(None),
            Some(v) => ObservationOutcome::project(v).map(Some),
        }
    }
    /// Off-renderer wait for the actual retained reply and its accounting record.
    pub fn wait(&self) -> Result<Result<ObservationOutcome, p::Error>, AdmissionError> {
        let v = self
            .queue
            .worker
            .wait_completion(&self.held)
            .map_err(AdmissionError::from)?;
        ObservationOutcome::project(v)
    }
    /// Observation does not apply a UI projection. The host must retain this
    /// receipt until that exact result is handled; taking a result is not ack.
    pub fn observe_success(
        &self,
    ) -> Result<(ObservationOutcome, ObservationAcknowledgment), AdmissionError> {
        let result = self
            .queue
            .worker
            .observe_observation_family(&self.held, None, false)?;
        Ok((
            ObservationOutcome {
                value: result.clone(),
            },
            ObservationAcknowledgment {
                completion: self.clone(),
                outcome: ObservationOutcome { value: result },
            },
        ))
    }
}
impl ObservationAcknowledgment {
    /// Bind host projection handling to this exact executor-result instance.
    pub fn matches(&self, outcome: &ObservationOutcome) -> bool {
        Arc::ptr_eq(&self.outcome.value, &outcome.value)
    }
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.completion.queue.worker.observe_observation_family(
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
#[path = "owned_observation_tests.rs"]
mod tests;
