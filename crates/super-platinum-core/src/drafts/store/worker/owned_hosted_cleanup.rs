//! No clear on drop or historical receipt; the consumer observes exact completion.
use super::*;
pub use p::hosted_files::cleanup::{
    Effect as HostedEffect, FreshClear as HostedFreshClear, Receipt as HostedReceipt,
};
type Completed = p::hosted_files::cleanup::Completed;
#[derive(Clone)]
pub struct HostedCleanup {
    completion: HostedCompletion,
    step: a::Step<Arc<HostedPrepared>, Arc<std::sync::Mutex<Completed>>>,
}
pub struct HostedAcknowledgment {
    cleanup: HostedCleanup,
}
impl HostedCompletion {
    pub fn cleanup(&self) -> Result<HostedCleanup, AdmissionError> {
        let step = self.queue.worker.begin_hosted_cleanup(&self.held)?;
        Ok(HostedCleanup {
            completion: self.clone(),
            step,
        })
    }
}
impl HostedCleanup {
    pub fn state(&self) -> JobState {
        self.step.state().into()
    }
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.completion
            .queue
            .worker
            .retry_hosted_cleanup(&self.completion.held, &self.step)
            .map_err(Into::into)
    }
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.completion
            .queue
            .worker
            .submit_step(&self.step)
            .map_err(Into::into)
    }
    pub fn wait(&self) -> Result<JobState, AdmissionError> {
        self.completion
            .queue
            .worker
            .wait_step(&self.step)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn observe(
        &self,
    ) -> Result<
        (
            HostedReceipt,
            Option<HostedFreshClear>,
            HostedAcknowledgment,
        ),
        AdmissionError,
    > {
        let receipt = self.completion.queue.worker.observe_hosted_cleanup(
            &self.completion.held,
            &self.step,
            false,
        )?;
        let clear = self
            .completion
            .queue
            .worker
            .take_hosted_clear(&self.completion.held, &self.step)?;
        Ok((
            receipt,
            clear,
            HostedAcknowledgment {
                cleanup: self.clone(),
            },
        ))
    }
}
impl HostedAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.cleanup.completion.queue.worker.observe_hosted_cleanup(
            &self.cleanup.completion.held,
            &self.cleanup.step,
            true,
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
