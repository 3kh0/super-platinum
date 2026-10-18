//! Consumer acknowledgment of an exact completed Send family. Drop retains it.
use super::*;
pub struct SendAcknowledgment {
    completion: MutationCompletion,
}
impl MutationCompletion {
    pub fn observe_sent(&self) -> Result<(p::Review, SendAcknowledgment), AdmissionError> {
        let review = self.queue.worker.observe_send_family(&self.held, false)?;
        Ok((
            review,
            SendAcknowledgment {
                completion: self.clone(),
            },
        ))
    }
}
impl SendAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self
            .completion
            .queue
            .worker
            .observe_send_family(&self.completion.held, true)
        {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}

/// Observed actual StartLocal lease; no fresh lease or replay can be minted here.
pub struct LocalHandoffAcknowledgment {
    completion: LocalCompletion,
}
impl LocalCompletion {
    pub fn observe_handoff(
        &self,
    ) -> Result<(p::LocalWriterLease, LocalHandoffAcknowledgment), AdmissionError> {
        let lease = self
            .queue
            .worker
            .observe_local_handoff_family(&self.held, false)?;
        Ok((
            lease,
            LocalHandoffAcknowledgment {
                completion: self.clone(),
            },
        ))
    }
}
impl LocalHandoffAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self
            .completion
            .queue
            .worker
            .observe_local_handoff_family(&self.completion.held, true)
        {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
