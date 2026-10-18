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
    #[cfg(test)]
    Isolated(durable::test_support::Isolated),
    #[cfg(test)]
    Owned(durable::owned_store::OpenedStore),
    Configured,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    ConfiguredFixture(durable::owned_store::OwnedRoute),
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
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Kind::ConfiguredFixture(route) => {
                let route = route.clone();
                let factory = factory.clone();
                let initialized = tokio::task::spawn_blocking(move || {
                    super::configured::initialize_route(route, &factory)
                })
                .await
                .map_err(|_| Error::Storage(durable::Error::Storage))??;
                Ok(ReadyAccount {
                    account: Account::from_retained(initialized.clone()),
                    initialized: Some(initialized),
                })
            }
            Kind::Configured => {
                let id = self.id.clone();
                let factory = factory.clone();
                let initialized = tokio::task::spawn_blocking(move || {
                    super::configured::initialize(&id, &factory)
                })
                .await
                .map_err(|_| Error::Storage(durable::Error::Storage))??;
                Ok(ReadyAccount {
                    account: Account::from_retained(initialized.clone()),
                    initialized: Some(initialized),
                })
            }
            #[cfg(test)]
            Kind::Isolated(isolated) => {
                let account = Account::isolated(self.id.clone(), isolated.client());
                account.initialize().await?;
                Ok(ReadyAccount {
                    account,
                    initialized: None,
                })
            }
            #[cfg(test)]
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
    #[cfg(test)]
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
    pub(super) fn configured(id: &str) -> Result<Arc<Self>, Error> {
        if !super_platinum_core::drafts::sync::valid_account_id(id) {
            return Err(Error::Identity);
        }
        Ok(Arc::new(Self {
            id: id.to_owned(),
            kind: Kind::Configured,
            hold: tokio::sync::Semaphore::new(0),
            held: AtomicBool::new(false),
            starts: Default::default(),
            root: Mutex::new(None),
            path: Default::default(),
        }))
    }
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub(super) fn configured_fixture(route: durable::owned_store::OwnedRoute) -> Arc<Self> {
        Arc::new(Self {
            id: route.account().into(),
            kind: Kind::ConfiguredFixture(route),
            hold: tokio::sync::Semaphore::new(0),
            held: AtomicBool::new(false),
            starts: Default::default(),
            root: Mutex::new(None),
            path: Default::default(),
        })
    }
    pub(super) fn is_configured(&self) -> bool {
        match &self.kind {
            Kind::Configured => true,
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Kind::ConfiguredFixture(_) => true,
            #[cfg(test)]
            _ => false,
        }
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
