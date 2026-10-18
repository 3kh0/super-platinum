//! Explicit schema initialization before owned handoff; blocking waits stay off UI.
use super::*;

#[derive(Clone)]
pub struct SlashSchemaJob {
    queue: Arc<Queue>,
    job: Arc<Job>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashSchemaJobId(JobId);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashSchemaReceipt {
    pub review_id: String,
    pub changed: bool,
}
impl OwnedRoute {
    /// Requires an exact fresh Protected7 inspection. Missing/ordinary stores
    /// first use the existing reviewed commit, then a separate fresh inspection.
    /// This does not open, initialize Current state, or authorize any HTTP.
    pub fn initialize_slash(&self, review: &StoreReview) -> Result<SlashSchemaJob, Error> {
        if !Arc::ptr_eq(&self.queue, &review.queue)
            || review.review.class() != upgrade::Class::Protected7
        {
            return Err(Error::Stale);
        }
        Ok(SlashSchemaJob {
            queue: self.queue.clone(),
            job: self
                .queue
                .submit_upgrade(upgrade::Action::InitializeSlash(review.review.clone()))?,
        })
    }
    /// Reattach only to the existing retained result, never resubmit migration.
    pub fn resume_slash_initialization(
        &self,
        id: SlashSchemaJobId,
    ) -> Result<SlashSchemaJob, Error> {
        if id.0.worker != self.worker_id() {
            return Err(Error::Stale);
        }
        Ok(SlashSchemaJob {
            queue: self.queue.clone(),
            job: self.queue.retained_job(id.0.index, true)?,
        })
    }
}
impl SlashSchemaJob {
    pub fn id(&self) -> SlashSchemaJobId {
        SlashSchemaJobId(JobId {
            worker: self.queue.worker_id().to_owned(),
            index: self.queue.job_id(&self.job),
        })
    }
    pub fn wait(&self) -> Result<SlashSchemaReceipt, Error> {
        Self::decode(self.job.wait()?)
    }
    pub fn poll(&self) -> Result<Option<SlashSchemaReceipt>, Error> {
        self.job.poll()?.map(Self::decode).transpose()
    }
    fn decode(reply: Arc<upgrade::Reply>) -> Result<SlashSchemaReceipt, Error> {
        match &*reply {
            upgrade::Reply::SlashInitialized { review, changed } => Ok(SlashSchemaReceipt {
                review_id: review.clone(),
                changed: *changed,
            }),
            _ => Err(Error::Stale),
        }
    }
}
#[cfg(test)]
#[path = "api_owned_slash_schema_tests.rs"]
mod tests;
