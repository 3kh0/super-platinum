//! Inactive bounded root lifetime. There is deliberately no default-store source.
//!
//! Sources are isolated test adapters or actual fixture-owned Slice 2a routes.
//! Ordinary ownership/Current cutover remains unavailable until the real drain
//! bridge exists. Neither this factory nor its errors authorize legacy load.
use super::*;
use crate::state::ShellState;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

mod configured;
#[cfg(test)]
mod configured_tests;
mod job;
#[cfg(test)]
mod owned_tests;
#[cfg(test)]
mod reservation;
mod source;
#[cfg(test)]
use reservation::{Reservation, Slot};
#[cfg(test)]
mod tests;
use job::Job;
use source::Source;

const MAX_ACCOUNTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Capacity,
    Closed,
    Identity,
    Stale,
    Pending,
    Storage(durable::Error),
}
impl From<durable::Error> for Error {
    fn from(value: durable::Error) -> Self {
        Self::Storage(value)
    }
}

struct Entry {
    source: Arc<Source>,
    initialization: Arc<Job<ReadyAccount>>,
}

struct ReadyAccount {
    account: Arc<Account>,
    // Historical full actual load retained alongside the exact Account lifetime.
    initialized: Option<durable::owned_store::Initialized>,
}

#[derive(Clone)]
pub(super) struct Acquisition(Arc<Entry>);
impl Acquisition {
    pub async fn ready(&self) {
        self.0.initialization.wait().ready().await;
    }
    pub fn account(&self) -> Result<Arc<Account>, Error> {
        Ok(self.0.initialization.result()?.account.clone())
    }
}

struct Selected {
    entry: Arc<Entry>,
    epoch: u64,
    view: u64,
    live: AtomicBool,
}
#[derive(Clone)]
pub(super) struct Selection(Arc<Selected>);

/// Constructed outside replaceable ShellState. No eviction, secret/session load,
/// migration, ordinary queue admission, or explicit flush in Drop. Successful and failed
/// entries both occupy their slot until root shutdown.
pub(super) struct Root {
    accounts: BTreeMap<String, Arc<Entry>>,
    selected: Option<Selection>,
    host: lifecycle::Host,
    closed: bool,
    identity: Arc<()>,
    factory: durable::owned_store::FactoryIdentity,
    #[cfg(test)]
    reservations: Vec<Arc<Slot>>,
}
impl Root {
    pub(super) fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
            selected: None,
            host: Default::default(),
            closed: false,
            identity: Arc::new(()),
            factory: Default::default(),
            #[cfg(test)]
            reservations: Vec::new(),
        }
    }

    #[cfg(not(test))]
    fn occupied(&self) -> usize {
        self.accounts.len()
    }

    /// Only the disconnected native fixture supplies this temporary opaque route.
    /// Initialization, selection, load, Host and all subsequent producer methods
    /// are the same configured path. No configured account/path is accepted here.
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub(super) fn acquire_configured_fixture(
        &mut self,
        route: durable::owned_store::OwnedRoute,
    ) -> Result<Acquisition, Error> {
        self.acquire(Source::configured_fixture(route))
    }

    pub(super) fn acquire_configured(&mut self, id: &str) -> Result<Acquisition, Error> {
        if let Some(entry) = self.accounts.get(id) {
            if self.closed || !entry.source.is_configured() {
                return Err(Error::Identity);
            }
            return Ok(Acquisition(entry.clone()));
        }
        self.acquire(Source::configured(id)?)
    }
    pub(super) fn configured_account(&self, id: &str) -> Result<Arc<Account>, Error> {
        let entry = self.accounts.get(id).ok_or(Error::Pending)?;
        if !entry.source.is_configured() {
            return Err(Error::Identity);
        }
        Ok(entry.initialization.result()?.account.clone())
    }
    pub(super) fn host(&self) -> &lifecycle::Host {
        &self.host
    }
    pub(super) fn host_mut(&mut self) -> &mut lifecycle::Host {
        &mut self.host
    }

    pub(super) fn submit_current(
        &mut self,
        shell: &ShellState,
        key: &DraftKey,
        local: &LocalDraft,
    ) -> Result<u64, Error> {
        let selected = self.selected.clone().ok_or(Error::Stale)?;
        self.check(shell, &selected)?;
        let account = selected.0.entry.initialization.result()?.account.clone();
        let selected = self.host.select(shell, account)?;
        if self.host.route_snapshot(shell, key).is_err() {
            let inventory = self
                .host
                .discovery()
                .entry(shell, key)
                .map(|view| view.inventory().clone());
            match inventory {
                Some(Inventory::Managed(expected)) => {
                    self.host.begin_managed(shell, &selected, expected)?;
                }
                Some(Inventory::Local(_)) | None => {
                    self.host
                        .open_local_current(shell, &selected, key.clone())?;
                }
            }
        }
        Ok(self.host.submit(shell, &selected, key, local)?)
    }

    /// Source is opaque and retained. A UUID collision with a different source
    /// refuses instead of returning the first account or opening another worker.
    fn acquire(&mut self, source: Arc<Source>) -> Result<Acquisition, Error> {
        self.acquire_inner(source, false)
    }
    fn acquire_inner(&mut self, source: Arc<Source>, reserved: bool) -> Result<Acquisition, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if let Some(entry) = self.accounts.get(source.id()) {
            return if Arc::ptr_eq(&entry.source, &source) {
                Ok(Acquisition(entry.clone()))
            } else {
                Err(Error::Identity)
            };
        }
        if !reserved && self.occupied() >= MAX_ACCOUNTS {
            return Err(Error::Capacity);
        }
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| Error::Storage(durable::Error::Storage))?;
        source.bind(&self.identity)?;
        let initialization = Job::new();
        let entry = Arc::new(Entry {
            source,
            initialization,
        });
        self.accounts
            .insert(entry.source.id().to_owned(), entry.clone());
        let completion = entry.initialization.completion();
        let owned = entry.clone();
        let factory = self.factory.clone();
        runtime.spawn(Box::pin(async move {
            // The task, not the first renderer waiter, owns initialization.
            owned.source.before_initialize().await;
            completion.finish(owned.source.initialize(&factory).await);
        }));
        Ok(Acquisition(entry))
    }

    /// Must be called before publishing a replacement shell. Retirement is
    /// immediate and irreversible; already admitted work remains account-owned.
    pub fn retire_selection(&mut self) {
        if let Some(old) = self.selected.take() {
            old.0.live.store(false, Ordering::Release);
        }
        self.host.retire_selection();
    }

    pub fn select(
        &mut self,
        shell: &ShellState,
        acquisition: &Acquisition,
    ) -> Result<Selection, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        self.retire_selection();
        let entry = self
            .accounts
            .get(acquisition.0.source.id())
            .ok_or(Error::Identity)?;
        if !Arc::ptr_eq(entry, &acquisition.0)
            || shell.core.active_account.as_deref() != Some(entry.source.id())
        {
            return Err(Error::Identity);
        }
        let selected = Selection(Arc::new(Selected {
            entry: entry.clone(),
            epoch: shell.core.account_epoch,
            view: shell.draft_writes.view_id,
            live: AtomicBool::new(true),
        }));
        self.selected = Some(selected.clone());
        Ok(selected)
    }

    fn check(&self, shell: &ShellState, selection: &Selection) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if !selection.0.live.load(Ordering::Acquire)
            || !self
                .selected
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(&current.0, &selection.0))
            || shell.core.active_account.as_deref() != Some(selection.0.entry.source.id())
            || shell.core.account_epoch != selection.0.epoch
            || shell.draft_writes.view_id != selection.0.view
        {
            return Err(Error::Stale);
        }
        Ok(())
    }

    /// Actual protected Discovery loader only. There is no Legacy variant or
    /// fallback callback. WrongFormat/failed initialization stay unavailable.
    pub fn begin_load(&mut self, shell: &ShellState, selected: &Selection) -> Result<Load, Error> {
        self.check(shell, selected)?;
        let initialized = selected.0.entry.initialization.result()?;
        self.host.select(shell, initialized.account.clone())?;
        let wait = self.host.begin_load(shell)?;
        Ok(Load {
            selection: selected.clone(),
            wait,
        })
    }
    pub fn finish_load(&mut self, shell: &ShellState, load: &Load) -> Result<(), Error> {
        self.check(shell, &load.selection)?;
        self.host.finish_load(shell, &load.wait)?;
        Ok(())
    }
    /// Caller cancellation retains Discovery's actual load. Reattachment does
    /// not enqueue another inventory or normalize a live account.
    pub fn pending_load(&self, shell: &ShellState, selected: &Selection) -> Result<Load, Error> {
        self.check(shell, selected)?;
        let wait = self
            .host
            .discovery()
            .pending_load(shell)
            .ok_or(Error::Stale)?;
        Ok(Load {
            selection: selected.clone(),
            wait,
        })
    }

    /// This slice retires only Root/Host selection admission and exposes retained
    /// initialization waits. Already returned Account/Editor capabilities remain
    /// usable; this is NOT an account-wide admission fence.
    /// It does NOT claim ordinary queues, Editors, or D jobs have drained. A
    /// production close hook must join their actual ownership barriers first.
    pub fn stop_admission(&mut self) -> Vec<Acquisition> {
        self.retire_selection();
        self.closed = true;
        self.accounts.values().cloned().map(Acquisition).collect()
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        self.retire_selection();
    }
}

pub(super) struct Load {
    selection: Selection,
    wait: discovery::Wait<Arc<durable::Loaded>>,
}
impl Load {
    pub async fn ready(&self) {
        self.wait.ready().await;
    }
}
