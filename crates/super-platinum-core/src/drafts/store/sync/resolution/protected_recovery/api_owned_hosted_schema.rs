//! Explicit schema initialization before owned handoff; blocking waits stay off UI.
use super::*;

#[derive(Clone)]
pub struct HostedSchemaJob {
    queue: Arc<Queue>,
    job: Arc<Job>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostedSchemaJobId(JobId);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostedSchemaReceipt {
    pub review_id: String,
    pub changed: bool,
}
impl OwnedRoute {
    /// Requires an exact fresh Protected7 inspection. Missing/ordinary stores
    /// first use the existing reviewed commit, then a separate fresh inspection.
    /// This does not open, initialize Current state, or authorize any HTTP.
    pub fn initialize_hosted(&self, review: &StoreReview) -> Result<HostedSchemaJob, Error> {
        if !Arc::ptr_eq(&self.queue, &review.queue)
            || review.review.class() != upgrade::Class::Protected7
        {
            return Err(Error::Stale);
        }
        Ok(HostedSchemaJob {
            queue: self.queue.clone(),
            job: self
                .queue
                .submit_upgrade(upgrade::Action::InitializeHosted(review.review.clone()))?,
        })
    }
    /// Reattach only to the existing retained result, never resubmit migration.
    pub fn resume_hosted_initialization(
        &self,
        id: HostedSchemaJobId,
    ) -> Result<HostedSchemaJob, Error> {
        if id.0.worker != self.worker_id() {
            return Err(Error::Stale);
        }
        Ok(HostedSchemaJob {
            queue: self.queue.clone(),
            job: self.queue.retained_job(id.0.index, true)?,
        })
    }
}
impl HostedSchemaJob {
    pub fn id(&self) -> HostedSchemaJobId {
        HostedSchemaJobId(JobId {
            worker: self.queue.worker_id().to_owned(),
            index: self.queue.job_id(&self.job),
        })
    }
    pub fn wait(&self) -> Result<HostedSchemaReceipt, Error> {
        Self::decode(self.job.wait()?)
    }
    pub fn poll(&self) -> Result<Option<HostedSchemaReceipt>, Error> {
        self.job.poll()?.map(Self::decode).transpose()
    }
    fn decode(reply: Arc<upgrade::Reply>) -> Result<HostedSchemaReceipt, Error> {
        match &*reply {
            upgrade::Reply::HostedInitialized { review, changed } => Ok(HostedSchemaReceipt {
                review_id: review.clone(),
                changed: *changed,
            }),
            _ => Err(Error::Stale),
        }
    }
}

#[cfg(test)]
#[path = "api_owned_hosted_schema_tests.rs"]
mod tests;
