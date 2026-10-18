use super::*;
pub struct HostedDraftAcknowledgment {
    job: StorageJob<HostedDraftInput, Option<HostedDraft>>,
    reports: Vec<(u64, crate::drafts::store::terminal::Report)>,
}
impl StorageJob<HostedDraftInput, Option<HostedDraft>> {
    pub fn retry_hosted_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_hosted_draft(&self.step)
            .map_err(Into::into)
    }
    pub fn observe_hosted_saved(
        &self,
    ) -> Result<(Option<HostedDraft>, HostedDraftAcknowledgment), AdmissionError> {
        let value = self
            .result()
            .ok_or(AdmissionError::Incomplete)?
            .map_err(|_| AdmissionError::Incomplete)?;
        let reports = self
            .queue
            .worker
            .observe_hosted_draft(&self.step, None, false)?;
        Ok((
            value,
            HostedDraftAcknowledgment {
                job: self.clone(),
                reports,
            },
        ))
    }
}
impl HostedDraftAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self
            .job
            .queue
            .worker
            .observe_hosted_draft(&self.job.step, Some(&self.reports), true)
        {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
