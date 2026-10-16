//! Typed result projection; wrong-family reattachment cannot consume a claim.
use super::*;
#[derive(Clone)]
pub struct MutationCompletion {
    pub(super) queue: Arc<Queue>,
    pub(super) held: a::Completion,
}
#[derive(Clone)]
pub struct LocalCompletion {
    pub(super) queue: Arc<Queue>,
    pub(super) held: a::Completion,
}
#[derive(Clone)]
pub struct MutationOutcome {
    pub outcome: p::Outcome,
    pub known_until: u64,
}
macro_rules! common {
    ($name:ident,$variant:ident,$output:ty) => {
        impl $name {
            /// Blocking retained result wait; no submission, retry or HTTP.
            /// The returned value belongs to the exact completed attempt even
            /// if another handle explicitly starts a later storage retry.
            pub fn wait(&self) -> Result<Result<$output, p::Error>, AdmissionError> {
                let value = self
                    .queue
                    .worker
                    .wait_completion(&self.held)
                    .map_err(AdmissionError::from)?;
                match &*value {
                    a::Finished::$variant(result) => Ok(result.clone()),
                    _ => Err(AdmissionError::Stale),
                }
            }
            pub fn key(&self) -> OperationKey {
                OperationKey(self.held.key())
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
            pub fn result(&self) -> Result<Option<Result<$output, p::Error>>, AdmissionError> {
                match self.held.result().map_err(AdmissionError::from)? {
                    None => Ok(None),
                    Some(v) => match &*v {
                        a::Finished::$variant(r) => Ok(Some(r.clone())),
                        _ => Err(AdmissionError::Stale),
                    },
                }
            }
        }
    };
}
common!(MutationCompletion, Mutation, p::Review);
common!(LocalCompletion, Local, p::LocalWriterLease);
impl MutationCompletion {
    /// Historical exact issued receipt, not transport or current-row authority.
    pub fn receipt(&self) -> Result<p::Receipt, AdmissionError> {
        self.held.mutation_receipt().map_err(Into::into)
    }

    /// The actual non-Clone claim is transferred once. A receipt/Operation cannot
    /// construct this value. The slot and immutable completion binding remain.
    pub fn take_claimed(&self) -> Result<p::ClaimedDispatch, AdmissionError> {
        match self
            .queue
            .worker
            .take_dispatch(&self.held)
            .map_err(AdmissionError::from)?
        {
            a::Dispatch::Mutation(claim) => Ok(claim),
            _ => unreachable!("typed mutation binding"),
        }
    }
    pub fn cancel_before_dispatch(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .cancel_before_dispatch(&self.held)
            .map_err(Into::into)
    }
    pub fn bind(&self, input: MutationOutcome) -> Result<(), AdmissionRefused<MutationOutcome>> {
        self.queue
            .worker
            .bind_raw(
                &self.held,
                a::Raw::Mutation {
                    outcome: input.outcome,
                    known_until: input.known_until,
                },
            )
            .map_err(|r| {
                let a::Raw::Mutation {
                    outcome,
                    known_until,
                } = r.input
                else {
                    unreachable!()
                };
                AdmissionRefused {
                    reason: r.reason.into(),
                    generation: r.generation,
                    input: MutationOutcome {
                        outcome,
                        known_until,
                    },
                }
            })
    }
    pub fn retained_outcome(&self) -> Option<MutationOutcome> {
        match self.held.raw().as_deref() {
            Some(a::Raw::Mutation {
                outcome,
                known_until,
            }) => Some(MutationOutcome {
                outcome: outcome.clone(),
                known_until: *known_until,
            }),
            _ => None,
        }
    }
}
impl Operations {
    pub fn resume_mutation(
        &self,
        key: &OperationKey,
    ) -> Result<MutationCompletion, AdmissionError> {
        let held = self
            .queue
            .worker
            .reattach_completion(&key.0)
            .map_err(AdmissionError::from)?;
        if !held.is_mutation() {
            return Err(AdmissionError::Stale);
        }
        Ok(MutationCompletion {
            queue: self.queue.clone(),
            held,
        })
    }
    pub fn resume_local(&self, key: &OperationKey) -> Result<LocalCompletion, AdmissionError> {
        let held = self
            .queue
            .worker
            .reattach_completion(&key.0)
            .map_err(AdmissionError::from)?;
        if !held.is_local() {
            return Err(AdmissionError::Stale);
        }
        Ok(LocalCompletion {
            queue: self.queue.clone(),
            held,
        })
    }
}
