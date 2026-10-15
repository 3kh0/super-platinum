//! Actual isolated test worker or opaque owned-store handoff.
//! No path/account/format booleans constitute production ownership.
use super::*;
pub(super) struct Source {
    id: String,
    kind: Kind,
    hold: tokio::sync::Semaphore,
    held: AtomicBool,
    starts: std::sync::atomic::AtomicUsize,
    root: Mutex<Option<Arc<()>>>,
    path: std::path::PathBuf,
}
enum Kind {
    Isolated(durable::test_support::Isolated),
    Owned(durable::owned_store::OpenedStore),
}
impl Source {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn bind(&self, identity: &Arc<()>) -> Result<(), Error> {
        let mut root = self
            .root
            .lock()
            .map_err(|_| Error::Storage(durable::Error::Storage))?;
        match &*root {
            Some(old) if !Arc::ptr_eq(old, identity) => Err(Error::Identity),
            Some(_) => Ok(()),
            None => {
                *root = Some(identity.clone());
                Ok(())
            }
        }
    }
    pub async fn initialize(
        &self,
        factory: &durable::owned_store::FactoryIdentity,
    ) -> Result<ReadyAccount, Error> {
        match &self.kind {
            Kind::Isolated(isolated) => {
                let account = Account::isolated(self.id.clone(), isolated.client());
                account.initialize().await?;
                Ok(ReadyAccount {
                    account,
                    initialized: None,
                })
            }
            Kind::Owned(opened) => {
                let job = opened.initialize_cold(factory)?;
                let initialized = tokio::task::spawn_blocking(move || job.wait())
                    .await
                    .map_err(|_| Error::Storage(durable::Error::Storage))??;
                let account = Account::from_initialized(initialized.clone());
                Ok(ReadyAccount {
                    account,
                    initialized: Some(initialized),
                })
            }
        }
    }
    pub(super) fn owned(opened: durable::owned_store::OpenedStore) -> Arc<Self> {
        Arc::new(Self {
            id: opened.account().to_owned(),
            kind: Kind::Owned(opened),
            hold: tokio::sync::Semaphore::new(0),
            held: AtomicBool::new(false),
            starts: Default::default(),
            root: Mutex::new(None),
            path: Default::default(),
        })
    }
    pub async fn before_initialize(&self) {
        self.starts.fetch_add(1, Ordering::AcqRel);
        if self.held.load(Ordering::Acquire) {
            let permit = self
                .hold
                .acquire()
                .await
                .expect("test gate is never closed");
            permit.forget();
        }
    }
    // Temporary-only constructor, never reads config or opens an existing path.
    // Test helpers run outside the renderer; core creates the fresh isolated DB.
    #[cfg(test)]
    pub(super) fn fresh(id: String, held: bool) -> Arc<Self> {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "protected-root-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let isolated = durable::test_support::Isolated::create(id.clone(), path.clone()).unwrap();
        Arc::new(Self {
            id,
            kind: Kind::Isolated(isolated),
            hold: tokio::sync::Semaphore::new(0),
            held: AtomicBool::new(held),
            starts: Default::default(),
            root: Mutex::new(None),
            path,
        })
    }
    #[cfg(test)]
    pub(super) fn release(&self) {
        self.hold.add_permits(1);
    }
    #[cfg(test)]
    pub(super) fn starts(&self) -> usize {
        self.starts.load(Ordering::Acquire)
    }
    #[cfg(test)]
    pub(super) fn marker(&self) -> std::path::PathBuf {
        self.path.join("fixture.json")
    }
    #[cfg(test)]
    pub(super) fn fail_next(&self, point: durable::test_support::FaultPoint) -> durable::Reply<()> {
        let Kind::Isolated(isolated) = &self.kind else {
            panic!("isolated test helper")
        };
        isolated.fail_next(point)
    }
}
