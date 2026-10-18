//! Inactive root-retainable local coordinator. No factory, renderer or network.
use super::*;
use crate::state::ShellState;
use std::sync::atomic::{AtomicBool, Ordering};
mod adopt;
mod install;
mod job;
mod rebind;
mod route;
pub(crate) use rebind::{ReadyRebind, RebindRequest};
mod consent;
pub(crate) use install::{InstallRequest, Installed, ReadyInstall};
pub(crate) use route::RouteSnapshot;
mod local;
mod managed;
pub(crate) use managed::{ManagedInstallRequest, ManagedOpen, ReadyManagedInstall};
#[cfg(test)]
mod tests;
pub(crate) use adopt::{Adoption, AdoptionResult};
use job::{Job, Wait};
pub(crate) use local::{Lane, LocalOpen};
const MAX_LOCAL_LANES: usize = 64;
const MAX_ADOPTIONS: usize = 32;

struct Selected {
    id: String,
    account: Arc<Account>,
    epoch: u64,
    view: u64,
    live: AtomicBool,
}
/// Identity, not a reconstructed account/epoch tuple. Old handles never revive.
#[derive(Clone)]
pub(crate) struct Selection(Arc<Selected>);
impl Selection {
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn epoch(&self) -> u64 {
        self.0.epoch
    }
    pub fn view(&self) -> u64 {
        self.0.view
    }
    pub fn account_id(&self) -> &str {
        self.0.account.id()
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.0.live.load(Ordering::Acquire)
            && shell.core.active_account.as_deref() == Some(self.0.account.id())
            && shell.core.account_epoch == self.0.epoch
            && shell.draft_writes.view_id == self.0.view
    }
}
/// The caller retains this outside replaceable ShellState. There is deliberately
/// no global/static default Host and no call from ordinary boot or routing.
#[derive(Default)]
pub(crate) struct Host {
    selected: Option<Selection>,
    discovery: discovery::Discovery,
    locals: BTreeMap<String, local::Slot>,
    managed: BTreeMap<String, managed::Slot>,
    adoptions: BTreeMap<String, adopt::Slot>,
    routes: BTreeMap<(String, DraftKey), route::Route>,
}
impl Host {
    pub fn select(
        &mut self,
        shell: &ShellState,
        account: Arc<Account>,
    ) -> Result<Selection, durable::Error> {
        if shell.core.active_account.as_deref() != Some(account.id()) {
            return Err(durable::Error::Stale);
        }
        if let Some(old) = &self.selected {
            if old.current(shell) && Arc::ptr_eq(&old.0.account, &account) {
                return Ok(old.clone());
            }
        }
        self.retire_selection();
        self.discovery.select(shell, account.clone())?;
        let selected = Selection(Arc::new(Selected {
            id: barrier_id(),
            account,
            epoch: shell.core.account_epoch,
            view: shell.draft_writes.view_id,
            live: AtomicBool::new(true),
        }));
        self.selected = Some(selected.clone());
        Ok(selected)
    }
    pub fn retire_selection(&mut self) {
        if let Some(old) = self.selected.take() {
            old.0.live.store(false, Ordering::Release);
        }
        self.discovery.retire();
    }
    fn check(&self, shell: &ShellState, selected: &Selection) -> Result<(), durable::Error> {
        if !selected.current(shell)
            || !self
                .selected
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(&s.0, &selected.0))
        {
            return Err(durable::Error::Stale);
        }
        Ok(())
    }
    pub fn begin_load(
        &mut self,
        shell: &ShellState,
    ) -> Result<discovery::Wait<Arc<durable::Loaded>>, durable::Error> {
        self.discovery.begin_load(shell)
    }
    pub fn finish_load(
        &mut self,
        shell: &ShellState,
        ready: &discovery::Wait<Arc<durable::Loaded>>,
    ) -> Result<(), durable::Error> {
        self.discovery.finish_load(shell, ready)
    }
    pub fn discovery(&self) -> &discovery::Discovery {
        &self.discovery
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.retire_selection();
    }
}

impl Host {
    pub(super) fn publish_current(
        &mut self,
        shell: &ShellState,
        before: &View,
        after: View,
    ) -> bool {
        self.discovery.publish_current(shell, before, after)
    }
}
