//! Explicit fixture route only. No default Client conversion or load mode.
use super::*;
pub(super) use durable::owned_store as o;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct Route {
    // Retain actual cold initialization and its physical owner, not only IDs.
    pub initialized: o::Initialized,
    pub operations: o::Operations,
    pub unsupported: AtomicBool,
    pub uploads: super::owned_upload::Book,
    #[cfg(test)]
    local_read_gate: std::sync::Mutex<
        Option<(
            tokio::sync::oneshot::Sender<()>,
            tokio::sync::oneshot::Receiver<()>,
        )>,
    >,
}
impl Route {
    pub fn new(initialized: o::Initialized) -> Arc<Self> {
        Arc::new(Self {
            operations: initialized.operations(),
            initialized,
            unsupported: AtomicBool::new(false),
            uploads: super::owned_upload::Book::default(),
            #[cfg(test)]
            local_read_gate: std::sync::Mutex::default(),
        })
    }
    pub fn unsupported(&self) -> durable::Error {
        self.unsupported.store(true, Ordering::Release);
        durable::Error::Unsupported
    }
    pub async fn review(&self, scope: Scope) -> Result<durable::Review, durable::Error> {
        // The actual worker retains admitted reads. Refused scope is still
        // owned by the existing Owner; it contains no displaced local input.
        let held = Held::new(self.operations.review(scope));
        held.observed_success().await
    }
    pub async fn local(&self, scope: Scope) -> Result<durable::LocalInventory, durable::Error> {
        let held = Held::new(self.operations.local_inventory(scope));
        let result = held.observed_success().await;
        #[cfg(test)]
        {
            let gate = self
                .local_read_gate
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .take();
            if let Some((entered, release)) = gate {
                let _ = entered.send(());
                release.await.map_err(|_| durable::Error::Storage)?;
            }
        }
        result
    }
    #[cfg(test)]
    pub fn hold_next_local_read(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (entered, observed) = tokio::sync::oneshot::channel();
        let (release, held) = tokio::sync::oneshot::channel();
        assert!(
            self.local_read_gate
                .lock()
                .unwrap()
                .replace((entered, held))
                .is_none()
        );
        (observed, release)
    }
    pub async fn inventory(&self) -> Result<Arc<durable::Loaded>, durable::Error> {
        // Cloning this result retains the actual worker's Arc, not a rebuilt
        // inventory or another initialization. Named read has no LoadMode input.
        let held = Held::new(self.operations.inventory());
        held.observed_success().await
    }
}

/// Queue admission is kept outside the durable result. This value is installed
/// in its existing parent BEFORE submit; refusal retains the exact returned input.
pub(super) struct Held<I, T> {
    pub admitted: Result<o::StorageJob<I, T>, o::AdmissionRefused<I>>,
}
impl<I, T> Held<I, T> {
    pub fn new(admitted: Result<o::StorageJob<I, T>, o::AdmissionRefused<I>>) -> Arc<Self> {
        Arc::new(Self { admitted })
    }
    pub fn job(&self) -> Result<&o::StorageJob<I, T>, o::AdmissionError> {
        self.admitted.as_ref().map_err(|e| e.reason)
    }
}
impl<I: Send + Sync + 'static, T: Send + 'static> Held<I, T> {
    pub fn submit(&self) -> Result<(), o::AdmissionError> {
        self.job()?.submit()
    }
    pub async fn wait(&self) -> Result<o::JobState, o::AdmissionError> {
        let job = self.job()?.clone();
        // Parent/root retains its actual future. Cancellation cannot discard
        // the moved receiver: blocking waiter owns it and caches its result.
        tokio::task::spawn_blocking(move || job.wait())
            .await
            .map_err(|_| o::AdmissionError::Incomplete)?
    }
}
impl<I: Send + Sync + 'static, T: Clone + Send + 'static> Held<I, T> {
    pub async fn observed_success(&self) -> Result<T, durable::Error> {
        self.submit().map_err(ui)?;
        self.wait().await.map_err(ui)?;
        let job = self.job().map_err(ui)?;
        if let Some(Err(error)) = job.result() {
            return Err(error);
        }
        let (value, acknowledgment) = job.observe_success().map_err(ui)?;
        // Actual consumer has observed and retains value before retirement.
        // Failed acknowledgment leaves the core slot and its result retained.
        acknowledgment
            .acknowledge()
            .map_err(|(error, _)| ui(error))?;
        Ok(value)
    }
    pub async fn value(&self) -> Result<Result<T, durable::Error>, o::AdmissionError> {
        self.submit()?;
        self.wait().await?;
        self.job()?.result().ok_or(o::AdmissionError::Incomplete)
    }
}
// UI compatibility only. Never feed this projection into Editor's Preserve
// fallback; owned_editor keeps the distinct outer AdmissionError until stopped.
pub(super) fn ui(_: o::AdmissionError) -> durable::Error {
    durable::Error::Unsupported
}

#[derive(Default)]
pub(super) struct Opening {
    pub read: Option<Arc<Held<Scope, durable::Review>>>,
    pub register: Option<Arc<Held<durable::Review, durable::Writer>>>,
}
