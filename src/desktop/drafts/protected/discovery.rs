//! Inactive account discovery/enrollment coordinator. No selector or UI caller.
//! Supplied Accounts are never reconstructed from legacy rows or Slack sessions.
use super::*;
use crate::state::ShellState;
use std::sync::atomic::{AtomicUsize, Ordering};

mod enrollment;
mod job;
pub(crate) use enrollment::{EnrollmentOutcome, EnrollmentRequest, PreparedEnrollment};
use job::Job;
pub(crate) use job::Wait;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Inactive,
    NeedsLoad,
    Loading,
    Ready,
    Unavailable(durable::Error),
}
struct Selection {
    account: Arc<Account>,
    epoch: u64,
    view: u64,
}
impl Selection {
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_deref() == Some(self.account.id())
            && shell.core.account_epoch == self.epoch
            && shell.draft_writes.view_id == self.view
    }
}
struct Load {
    selection: Arc<Selection>,
    ticket: session::LoadTicket,
    job: Arc<Job<Arc<durable::Loaded>>>,
}
/// This owner is retained by its caller across ShellState replacement. Its
/// selection is explicitly reconciled with each new shell; there is no timer.
pub(crate) struct Discovery {
    selected: Option<Arc<Selection>>,
    session: Session,
    registry: Arc<registry::Registry>,
    keys: Vec<DraftKey>,
    hosted: Option<Arc<durable::Loaded>>,
    status: Status,
    generation: u64,
    load: Option<Load>,
    reads: Arc<AtomicUsize>,
    enrollment: Option<enrollment::Pending>,
}
impl Default for Discovery {
    fn default() -> Self {
        Self {
            selected: None,
            session: Session::default(),
            registry: Arc::default(),
            keys: vec![],
            hosted: None,
            status: Status::Inactive,
            generation: 0,
            load: None,
            reads: Arc::default(),
            enrollment: None,
        }
    }
}
impl Discovery {
    /// Memory only. A factory/default-store/keyring lookup is deliberately not
    /// part of this API. A wrong supplied Account fails before any load.
    pub fn select(
        &mut self,
        shell: &ShellState,
        account: Arc<Account>,
    ) -> Result<(), durable::Error> {
        if shell.core.active_account.as_deref() != Some(account.id()) {
            return Err(durable::Error::Stale);
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|old| old.current(shell) && Arc::ptr_eq(&old.account, &account))
        {
            return Ok(());
        }
        self.invalidate();
        let selected = Arc::new(Selection {
            account,
            epoch: shell.core.account_epoch,
            view: shell.draft_writes.view_id,
        });
        self.session
            .select(selected.account.id().into(), selected.epoch);
        self.registry
            .select(selected.account.id().into(), selected.epoch);
        self.selected = Some(selected);
        self.status = Status::NeedsLoad;
        Ok(())
    }
    /// Sign-out/owner retirement clears only presentation and registry owners.
    /// Account retains failed input; an admitted enrollment keeps its receipt.
    pub fn retire(&mut self) {
        self.invalidate();
    }
    fn invalidate(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("discovery generation exhausted");
        self.selected = None;
        self.session = Session::default();
        // Registry::select has no view identity. A new registry is required
        // even when a replacement shell has the same account and epoch.
        self.registry = Arc::default();
        self.keys.clear();
        self.hosted = None;
        self.load = None;
        self.status = Status::Inactive;
    }
    fn selection(&self, shell: &ShellState) -> Result<&Arc<Selection>, durable::Error> {
        self.selected
            .as_ref()
            .filter(|s| s.current(shell))
            .ok_or(durable::Error::Stale)
    }
    pub fn status(&self, shell: &ShellState) -> Status {
        if self.selection(shell).is_ok() {
            self.status
        } else {
            Status::Inactive
        }
    }
    pub fn registry(&self, shell: &ShellState) -> Result<Arc<registry::Registry>, durable::Error> {
        self.selection(shell)?;
        Ok(self.registry.clone())
    }
    pub fn entry(&self, shell: &ShellState, key: &DraftKey) -> Option<&View> {
        let selected = self.selection(shell).ok()?;
        (self.status == Status::Ready)
            .then(|| {
                self.session
                    .entry(selected.account.id(), selected.epoch, key)
            })
            .flatten()
    }
    pub fn keys(&self, shell: &ShellState) -> Option<&[DraftKey]> {
        (self.selection(shell).is_ok() && self.status == Status::Ready)
            .then_some(self.keys.as_slice())
    }
    pub(crate) fn hosted(
        &self,
        shell: &ShellState,
    ) -> Option<&durable::owned_store::HostedInventory> {
        self.selection(shell).ok()?;
        (self.status == Status::Ready)
            .then(|| self.hosted.as_ref().map(|v| v.hosted()))
            .flatten()
    }
    pub fn recovery(&self, shell: &ShellState) -> Option<&[durable::RecoverySummary]> {
        let selected = self.selection(shell).ok()?;
        (self.status == Status::Ready)
            .then(|| self.session.recovery(selected.account.id(), selected.epoch))
            .flatten()
    }
    /// Exposes all actual retained identities, including multiple writers for
    /// one scope. No first-by-key choice, reconstruction, or automatic restore.
    pub fn retained(&self, shell: &ShellState) -> Result<Vec<Arc<editor::Editor>>, durable::Error> {
        self.selection(shell)?.account.retained_editors()
    }
    /// Recover readiness after a UI waiter was canceled. This does not issue
    /// another read or duplicate the one-use result.
    pub fn pending_load(&self, shell: &ShellState) -> Option<Wait<Arc<durable::Loaded>>> {
        let selected = self.selection(shell).ok()?;
        self.load
            .as_ref()
            .filter(|load| Arc::ptr_eq(selected, &load.selection))
            .map(|load| Wait::new(load.job.clone()))
    }
    pub fn begin_load(
        &mut self,
        shell: &ShellState,
    ) -> Result<Wait<Arc<durable::Loaded>>, durable::Error> {
        let selected = self.selection(shell)?.clone();
        if self.load.is_some() {
            return Err(durable::Error::Pending);
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?;
        // Old read tasks may finish after an account/view switch. Bound them
        // without canceling initialization or accepting their stale results.
        self.reads
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| durable::Error::Pending)?;
        let permit = ReadPermit(self.reads.clone());
        let ticket = self.session.begin_load().ok_or(durable::Error::Stale)?;
        let job = Job::new();
        self.load = Some(Load {
            selection: selected.clone(),
            ticket,
            job: job.clone(),
        });
        self.status = Status::Loading;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("discovery generation exhausted");
        let account = selected.account.clone();
        let completion = job.completion();
        runtime.spawn(Box::pin(async move {
            let _permit = permit;
            completion.finish(account.inventory().await);
        }));
        Ok(Wait::new(job))
    }
    /// Called after the returned wait handle becomes ready. Publication is a
    /// synchronous whole-inventory transaction on this coordinator only.
    pub fn finish_load(
        &mut self,
        shell: &ShellState,
        wait: &Wait<Arc<durable::Loaded>>,
    ) -> Result<(), durable::Error> {
        let selected = self.selection(shell)?.clone();
        let load = self.load.as_ref().ok_or(durable::Error::Stale)?;
        if !Arc::ptr_eq(&selected, &load.selection) || !wait.is(&load.job) {
            return Err(durable::Error::Stale);
        }
        let result = load.job.take()?;
        let load = self.load.take().expect("checked load");
        self.status = Status::Unavailable(durable::Error::Storage);
        let loaded = match result {
            Ok(loaded) => loaded,
            Err(error) => {
                self.status = Status::Unavailable(error);
                return Err(error);
            }
        };
        let views = model::views(selected.account.id(), &loaded)?;
        self.session.accept_load(load.ticket, &loaded)?;
        self.keys = views.into_keys().collect();
        self.hosted = Some(loaded);
        self.status = Status::Ready;
        Ok(())
    }
}
struct ReadPermit(Arc<AtomicUsize>);
impl Drop for ReadPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Discovery {
    pub(in super::super) fn publish_current(
        &mut self,
        shell: &ShellState,
        before: &View,
        after: View,
    ) -> bool {
        let Ok(selected) = self.selection(shell).cloned() else {
            return false;
        };
        self.status == Status::Ready
            && self
                .session
                .apply(selected.account.id(), selected.epoch, before, after)
    }
}
