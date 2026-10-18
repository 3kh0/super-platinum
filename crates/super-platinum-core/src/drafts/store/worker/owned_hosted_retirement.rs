use super::*;
pub struct HostedRetirementAcknowledgment {
    job: StorageJob<HostedRetirement, HostedRetired>,
    reports: Vec<(u64, crate::drafts::store::terminal::Report)>,
}
impl StorageJob<HostedRetirement, HostedRetired> {
    pub fn retry_retirement_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_hosted_retirement(&self.step)
            .map_err(Into::into)
    }
    pub fn observe_retirement(
        &self,
    ) -> Result<(HostedRetired, HostedRetirementAcknowledgment), AdmissionError> {
        let value = self
            .result()
            .ok_or(AdmissionError::Incomplete)?
            .map_err(|_| AdmissionError::Incomplete)?;
        let reports = self
            .queue
            .worker
            .observe_hosted_retirement(&self.step, None, false)?;
        Ok((
            value,
            HostedRetirementAcknowledgment {
                job: self.clone(),
                reports,
            },
        ))
    }
}
impl HostedRetirementAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.job.queue.worker.observe_hosted_retirement(
            &self.job.step,
            Some(&self.reports),
            true,
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}

impl Operations {
    pub fn retire_hosted(
        &self,
        input: HostedRetirement,
    ) -> Result<StorageJob<HostedRetirement, HostedRetired>, AdmissionRefused<HostedRetirement>>
    {
        self.queue
            .worker
            .hold_hosted_retirement(input)
            .map(|s| self.wrap(s))
            .map_err(Into::into)
    }
    pub fn resume_hosted_retirement(
        &self,
        key: &OperationKey,
    ) -> Result<StorageJob<HostedRetirement, HostedRetired>, AdmissionError> {
        self.resume(key)
    }
}
