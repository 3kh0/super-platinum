//! Explicit cold configured ownership. No queue-idle sample can grant this.
//! Registration and first ordinary submission share one process-wide mutex.
use super::super::protected::{Error, upgrade::Owned};
use super::*;
use std::{path::PathBuf, sync::Arc};

enum Entry {
    Ordinary(OnceLock<Result<Arc<Owned>, Error>>),
    Protected(OnceLock<Result<Arc<owned_queue::Queue>, Error>>),
}
#[derive(Default)]
pub(in crate::drafts::store) struct Registry {
    entries: Mutex<BTreeMap<String, Arc<Entry>>>,
    #[cfg(test)]
    root: Option<PathBuf>,
}
pub(in crate::drafts::store) fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}
impl Registry {
    // Mark before enqueue, not at cycle/open. A detached batch is still activity.
    pub(super) fn ordinary(&self, account: &str) -> bool {
        let mut entries = self.entries.lock().unwrap();
        matches!(
            &**entries
                .entry(account.to_owned())
                .or_insert_with(|| { Arc::new(Entry::Ordinary(OnceLock::new())) }),
            Entry::Ordinary(_)
        )
    }
    fn directory(&self) -> Result<PathBuf, Error> {
        #[cfg(test)]
        if let Some(root) = &self.root {
            return Ok(root.clone());
        }
        crate::config::data_dir().map_err(|_| Error::Storage)
    }
    pub(in crate::drafts::store) fn acquire(
        &self,
        account: &str,
    ) -> Result<Arc<owned_queue::Queue>, Error> {
        if uuid::Uuid::parse_str(account)
            .ok()
            .map(|id| id.to_string())
            .as_deref()
            != Some(account)
        {
            return Err(Error::Stale);
        }
        let entry = {
            let mut entries = self.entries.lock().map_err(|_| Error::Storage)?;
            if !entries.contains_key(account)
                && entries
                    .values()
                    .filter(|entry| matches!(&***entry, Entry::Protected(_)))
                    .count()
                    >= 16
            {
                return Err(Error::Pending);
            }
            entries
                .entry(account.to_owned())
                .or_insert_with(|| Arc::new(Entry::Protected(OnceLock::new())))
                .clone()
        };
        let Entry::Protected(job) = &*entry else {
            return Err(Error::Stale);
        };
        // Registry retains both success and failure after dropped callers. No
        // second worker, fallback opener, automatic migration or retry is minted.
        job.get_or_init(|| {
            let owner =
                Owned::configured(account, self.directory()?).map_err(|_| Error::Storage)?;
            owned_queue::Queue::start(owner)
        })
        .clone()
    }
    pub(super) fn open_ordinary(&self, account: &str) -> Result<DraftStore, AppError> {
        self.ordinary(account);
        let entry = self.entries.lock().unwrap().get(account).unwrap().clone();
        let Entry::Ordinary(job) = &*entry else {
            return Err(sync::invalid(
                "configured account belongs to protected owner",
            ));
        };
        let owner = job
            .get_or_init(|| {
                Owned::configured(account, self.directory()?).map_err(|_| Error::Storage)
            })
            .clone()
            .map_err(|_| sync::invalid("configured account owner unavailable"))?;
        owner.check()?;
        let retained = owner.retain_db()?;
        let store = DraftStore::open(owner.path())?;
        owner.check()?;
        // Existing configured file identities remain held during ordinary open.
        drop(retained);
        Ok(store)
    }
}

#[cfg(test)]
#[path = "configured_tests.rs"]
mod tests;
