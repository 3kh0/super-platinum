//! Only the real worker can construct a cutover fence. Explicit test-support substrate.
use super::super::protected::upgrade::{self, Command};
#[cfg(test)]
use super::super::protected::upgrade::{Action, Owned, Reply as UpgradeReply};
use super::*;
#[cfg(test)]
use std::sync::Arc;

pub(in crate::drafts::store) struct UpgradeFence<'a> {
    worker: &'a Worker,
    account: String,
    generation: u64,
    sole: bool,
    sealed: bool,
}
impl UpgradeFence<'_> {
    pub(in crate::drafts::store) fn generation(&self) -> u64 {
        self.generation
    }
    pub(in crate::drafts::store) fn worker_id(&self) -> &str {
        self.worker
            .upgrade_id
            .get_or_init(|| uuid::Uuid::new_v4().to_string())
    }
    pub(in crate::drafts::store) fn check(&self, account: &str) -> Result<(), AppError> {
        let pending = self.worker.pending.lock().unwrap();
        if self.sealed
            || !self.sole
            || account != self.account
            || self.generation == u64::MAX
            || pending.upgrade_generation != self.generation
            || !pending.requests.is_empty()
            || pending.failed.contains(account)
            || pending.blocked.contains(account)
            || pending.updates.keys().any(|(a, _)| a == account)
        {
            return Err(sync::invalid(
                "upgrade account queue is not exclusively quiescent",
            ));
        }
        Ok(())
    }
    pub(in crate::drafts::store) fn seal(&mut self, account: &str) -> Result<(), AppError> {
        // This lock is the ordering point, not a lock held across SQLite I/O.
        // Later enqueue remains in Pending for the following worker cycle.
        self.check(account)?;
        let pending = self.worker.pending.lock().unwrap();
        if pending.upgrade_generation != self.generation {
            return Err(sync::invalid("input raced the upgrade seal"));
        }
        self.sealed = true;
        Ok(())
    }
}
pub(super) fn execute(
    worker: &Worker,
    command: Command,
    generation: u64,
    sole: bool,
) -> super::super::terminal::Report {
    let account = command.account().to_owned();
    command.execute(UpgradeFence {
        worker,
        account,
        generation,
        sole,
        sealed: false,
    })
}

// Actual cycle, isolated Worker, and never the default opener. This is the sole
// route used by sibling migration tests; no arbitrary bool can mint the fence.
#[cfg(test)]
pub(in crate::drafts::store) struct TestWorker(Arc<Worker>);
#[cfg(test)]
impl TestWorker {
    pub(in crate::drafts::store) fn new() -> Self {
        Self(Arc::new(Worker::default()))
    }
    pub(in crate::drafts::store) fn run(&self, owner: &Arc<Owned>, command: Command) {
        self.0.submit(Request::StoreUpgrade(Box::new(command)));
        self.0.cycle(|account| {
            if account != owner.account() {
                return Err(sync::invalid("wrong test account"));
            }
            DraftStore::open(owner.path())
        });
    }
}

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;
