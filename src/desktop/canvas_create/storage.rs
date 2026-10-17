use super::domain::{self, Owner, Store};
use std::path::PathBuf;

// Private account-bound authority. Production resolves the established config
// directory only on the worker; explicit paths exist only in isolated tests.
#[derive(Clone)]
pub(in crate::canvas_create) struct Storage {
    path: Option<PathBuf>,
    owner: Owner,
    #[cfg(test)]
    fault: std::sync::Arc<std::sync::atomic::AtomicU8>,
}
impl Storage {
    // A stable owner is local authority only; no network credential is retained.
    pub(in crate::canvas_create) fn application(owner: Owner) -> domain::Result<Self> {
        if cfg!(test) || crate::fixture::is_fixture() {
            return Err(domain::Fault::Invalid);
        }
        Ok(Self {
            path: None,
            owner,
            #[cfg(test)]
            fault: Default::default(),
        })
    }

    // Only the exact private native fixture capability can supply this path.
    // No public/environment path constructor exists on Storage.
    pub(in crate::canvas_create) fn fixture(
        binding: &crate::canvas_create::fixture::Binding,
    ) -> Self {
        Self {
            path: Some(binding.path().to_owned()),
            owner: binding.owner().clone(),
            #[cfg(test)]
            fault: Default::default(),
        }
    }

    #[cfg(test)]
    pub(in crate::canvas_create) fn isolated(path: PathBuf, owner: Owner) -> domain::Result<Self> {
        if !path.is_absolute() {
            return Err(domain::Fault::Invalid);
        }
        Ok(Self {
            path: Some(path),
            owner,
            fault: Default::default(),
        })
    }
    #[cfg(test)]
    pub(in crate::canvas_create) fn fail_next(&self, after_commit: bool) {
        self.fault.store(
            if after_commit { 2 } else { 1 },
            std::sync::atomic::Ordering::SeqCst,
        );
    }

    pub(in crate::canvas_create) fn owner(&self) -> &Owner {
        &self.owner
    }

    pub(in crate::canvas_create) async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Store) -> domain::Result<T> + Send + 'static,
    ) -> domain::Result<T> {
        let storage = self.clone();
        tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            let fault = storage.fault.swap(0, std::sync::atomic::Ordering::SeqCst);
            #[cfg(test)]
            if fault == 1 {
                return Err(domain::Fault::Invalid);
            }
            let mut store = match &storage.path {
                Some(path) => Store::open(path, storage.owner.account())?,
                None => {
                    if cfg!(test) {
                        return Err(domain::Fault::Invalid);
                    }
                    Store::open_default(storage.owner.account())?
                }
            };
            let result = action(&mut store);
            #[cfg(test)]
            if fault == 2 {
                return Err(domain::Fault::Invalid);
            }
            result
        })
        .await
        // No claim of rollback/known-unsent on a worker panic or lost reply.
        .map_err(|_| domain::Fault::Invalid)?
    }
}
