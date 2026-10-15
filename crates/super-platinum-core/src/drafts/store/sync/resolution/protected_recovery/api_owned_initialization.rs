//! Opaque cold initialization result. A migration receipt cannot construct it.
use super::*;
use crate::drafts::store::worker::owned_queue::initialization::ColdJob;

/// Root identity is memory-only. It grants no disk access or initialization by
/// itself; only the exact opened route can bind it, once, to its real load.
#[derive(Clone)]
pub struct FactoryIdentity(Arc<()>);
impl Default for FactoryIdentity {
    fn default() -> Self {
        Self(Arc::new(()))
    }
}
#[derive(Clone)]
pub struct InitializationJob {
    opened: OpenedStore,
    job: Arc<ColdJob>,
}
#[derive(Clone)]
pub struct Initialized {
    opened: OpenedStore,
    loaded: Arc<Loaded>,
}
impl OpenedStore {
    pub fn initialize_cold(&self, identity: &FactoryIdentity) -> Result<InitializationJob, Error> {
        Ok(InitializationJob {
            opened: self.clone(),
            job: self.queue.cold(&identity.0)?,
        })
    }
}
impl InitializationJob {
    pub fn wait(&self) -> Result<Initialized, Error> {
        Ok(Initialized {
            opened: self.opened.clone(),
            loaded: self.job.wait()?,
        })
    }
    pub fn poll(&self) -> Result<Option<Initialized>, Error> {
        Ok(self.job.poll()?.map(|loaded| Initialized {
            opened: self.opened.clone(),
            loaded,
        }))
    }
}
pub use crate::drafts::store::worker::owned_queue::operations::*;
impl Initialized {
    /// Inactive explicit Owned route only; never converts the ordinary Client.
    pub fn operations(&self) -> Operations {
        Operations::from_initialized(self.opened.queue.clone())
    }

    pub fn account(&self) -> &str {
        self.opened.account()
    }
    pub fn worker_id(&self) -> &str {
        self.opened.worker_id()
    }
    pub fn mode(&self) -> LoadMode {
        LoadMode::AfterRestart
    }
    /// Historical actual initialization inventory, not a fresh write permit.
    pub fn inventory(&self) -> &Loaded {
        &self.loaded
    }
    pub fn client(&self) -> Client {
        self.opened.client()
    }
}
