//! Explicit configured cold ownership and gated fixture construction. No path, SQL,
//! initialization mode or drain certificate can be supplied by a caller.
//! Construction, handoff, reopening and waits perform disk I/O: use off renderer.
use super::*;
// Explicitly select the owned facade. The parent's legacy BaselineCompletion
// otherwise shadows the public operations glob through the private import above.
pub use crate::drafts::store::worker::owned_queue::operations::{
    BaselineAcknowledgment, BaselineClaimed, BaselineCompletion, BaselineFinishInput,
    BaselineInput, BaselineOutcome, BaselineRead, BaselineReserved, BaselineState,
};
use crate::drafts::store::worker::{
    owned_queue::{Job, Queue},
    protected_queue::Route,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct OwnedRoute {
    queue: Arc<Queue>,
}
#[derive(Clone)]
pub struct InspectJob {
    queue: Arc<Queue>,
    job: Arc<Job>,
}
#[derive(Clone)]
pub struct CommitJob {
    queue: Arc<Queue>,
    job: Arc<Job>,
}
#[derive(Clone)]
pub struct StoreReview {
    queue: Arc<Queue>,
    review: Arc<upgrade::Review>,
}
#[derive(Clone)]
pub struct OpenedStore {
    queue: Arc<Queue>,
    migration_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobId {
    worker: String,
    index: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    Missing,
    OrdinaryLocal,
    Legacy5,
    Protected7,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewMetadata {
    pub kind: StoreKind,
    pub rows: usize,
    pub migration_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReceipt {
    pub id: String,
    pub imported: usize,
    pub repeated: bool,
}

impl OwnedRoute {
    /// Explicit cold acquisition only. Call off the renderer before the first
    /// configured draft load/write. Failed acquisition is retained; a warm
    /// default-worker account cannot be switched or drained by this method.
    pub fn acquire_configured_cold(account: &str) -> Result<Self, Error> {
        Ok(Self {
            queue: crate::drafts::store::worker::configured::registry().acquire(account)?,
        })
    }

    /// Fresh temporary store only, seeded before inspection. Blocking/off-renderer.
    /// The returned context is structurally disconnected and has no live fallback.
    #[cfg(any(test, feature = "protected-test-support"))]
    pub fn fresh_slash_fixture(
        target: SlashFixtureTarget,
        outcome: SlashFixtureOutcome,
    ) -> Result<(Self, SlashFixture), Error> {
        let queue = Queue::fresh()?;
        let fixture = SlashFixture::create(queue.clone(), target, outcome)?;
        Ok((Self { queue }, fixture))
    }

    /// Closed temporary remote-baseline seed. No configured account/path input.
    /// Call off-renderer, then perform the normal inspect/commit/open/cold path.
    #[cfg(any(test, feature = "protected-test-support"))]
    pub fn fresh_baseline_fixture(
        target: BaselineFixtureTarget,
        outcome: BaselineFixtureOutcome,
    ) -> Result<(Self, BaselineFixture), Error> {
        let queue = Queue::fresh()?;
        let fixture = BaselineFixture::create(queue.clone(), target, outcome)?;
        Ok((Self { queue }, fixture))
    }

    #[cfg(any(test, feature = "protected-test-support"))]
    pub fn fresh_fixture() -> Result<Self, Error> {
        Ok(Self {
            queue: Queue::fresh()?,
        })
    }
    /// Locator parsing only selects the known temporary fixture. Real marker
    /// locking, file identity and read-only inspection establish fresh authority.
    #[cfg(any(test, feature = "protected-test-support"))]
    pub fn reopen_fixture(ticket: &str) -> Result<Self, Error> {
        Ok(Self {
            queue: Queue::reopen(ticket)?,
        })
    }
    pub fn account(&self) -> &str {
        self.queue.account()
    }
    pub fn worker_id(&self) -> &str {
        self.queue.worker_id()
    }
    /// Recover only the existing successful handoff, including after the last UI
    /// review/result handle was dropped. This is not a new inventory or readiness
    /// check; every Client command still validates the retained owner and file.
    pub fn opened(&self) -> Option<OpenedStore> {
        let proof = self.queue.opened()?;
        Some(OpenedStore {
            queue: self.queue.clone(),
            migration_id: proof.migration_id().map(str::to_owned),
        })
    }
    pub fn inspect(&self) -> Result<InspectJob, Error> {
        Ok(InspectJob {
            queue: self.queue.clone(),
            job: self.queue.submit_upgrade(upgrade::Action::Inspect)?,
        })
    }
    pub fn commit(&self, review: &StoreReview) -> Result<CommitJob, Error> {
        if !Arc::ptr_eq(&self.queue, &review.queue) {
            return Err(Error::Stale);
        }
        Ok(CommitJob {
            queue: self.queue.clone(),
            job: self
                .queue
                .submit_upgrade(upgrade::Action::Commit(review.review.clone()))?,
        })
    }
    pub fn resume_commit(&self, id: JobId) -> Result<CommitJob, Error> {
        if id.worker != self.worker_id() {
            return Err(Error::Stale);
        }
        let job = self.queue.retained_job(id.index, true)?;
        Ok(CommitJob {
            queue: self.queue.clone(),
            job,
        })
    }
}
impl InspectJob {
    pub fn wait(&self) -> Result<StoreReview, Error> {
        self.decode(self.job.wait()?)
    }
    pub fn poll(&self) -> Result<Option<StoreReview>, Error> {
        self.job.poll()?.map(|reply| self.decode(reply)).transpose()
    }
    fn decode(&self, reply: Arc<upgrade::Reply>) -> Result<StoreReview, Error> {
        match &*reply {
            upgrade::Reply::Inspected(review) => Ok(StoreReview {
                queue: self.queue.clone(),
                review: review.clone(),
            }),
            _ => Err(Error::Stale),
        }
    }
}
impl CommitJob {
    pub fn id(&self) -> JobId {
        JobId {
            worker: self.queue.worker_id().to_owned(),
            index: self.queue.job_id(&self.job),
        }
    }
    pub fn wait(&self) -> Result<CommitReceipt, Error> {
        Self::decode(self.job.wait()?)
    }
    pub fn poll(&self) -> Result<Option<CommitReceipt>, Error> {
        self.job.poll()?.map(Self::decode).transpose()
    }
    fn decode(reply: Arc<upgrade::Reply>) -> Result<CommitReceipt, Error> {
        match &*reply {
            upgrade::Reply::Committed(receipt) => Ok(CommitReceipt {
                id: receipt.id.clone(),
                imported: receipt.imported,
                repeated: receipt.repeated,
            }),
            _ => Err(Error::Stale),
        }
    }
}
impl StoreReview {
    pub fn metadata(&self) -> ReviewMetadata {
        ReviewMetadata {
            kind: match self.review.class() {
                upgrade::Class::Missing => StoreKind::Missing,
                upgrade::Class::OrdinaryLocal => StoreKind::OrdinaryLocal,
                upgrade::Class::Legacy5 => StoreKind::Legacy5,
                upgrade::Class::Protected7 => StoreKind::Protected7,
            },
            rows: self.review.row_count(),
            migration_id: self.review.migration_id().map(str::to_owned),
        }
    }
    pub fn open(self) -> Result<OpenedStore, Error> {
        self.queue.open(self.review.clone())?;
        Ok(OpenedStore {
            queue: self.queue,
            migration_id: self.review.migration_id().map(str::to_owned),
        })
    }
}
impl OpenedStore {
    pub fn account(&self) -> &str {
        self.queue.account()
    }
    pub fn worker_id(&self) -> &str {
        self.queue.worker_id()
    }
    pub fn migration_id(&self) -> Option<&str> {
        self.migration_id.as_deref()
    }
    #[cfg(any(test, feature = "protected-test-support"))]
    pub fn fixture_ticket(&self) -> Result<String, Error> {
        self.queue.ticket()
    }
    pub fn client(&self) -> Client {
        Client {
            account: self.account().to_owned(),
            route: Route::Owned(self.queue.clone()),
        }
    }
}

#[path = "api_owned_initialization.rs"]
mod initialization;
pub use initialization::*;

#[path = "api_owned_slash_schema.rs"]
mod slash_schema;
pub use slash_schema::*;
