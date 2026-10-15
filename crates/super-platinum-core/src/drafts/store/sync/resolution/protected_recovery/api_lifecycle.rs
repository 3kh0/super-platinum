//! Process authority belongs to one FIFO route. Loaded epochs are never latches.
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Default)]
struct Registry(BTreeMap<(String, String), Vec<Weak<AtomicBool>>>);
impl Registry {
    fn sweep(&mut self) {
        self.0.retain(|_, entries| {
            entries.retain(|entry| {
                entry
                    .upgrade()
                    .is_some_and(|latch| !latch.load(Ordering::Acquire))
            });
            !entries.is_empty()
        });
    }
    fn revoke(&mut self, account: &str, team: Option<&str>) {
        self.0.retain(|(owner, name), entries| {
            if owner != account || team.is_some_and(|wanted| wanted != name) {
                return true;
            }
            for latch in entries.iter().filter_map(Weak::upgrade) {
                latch.store(true, Ordering::Release);
            }
            false
        });
        self.sweep();
    }
}
impl Drop for Registry {
    fn drop(&mut self) {
        for latch in self.0.values().flatten().filter_map(Weak::upgrade) {
            latch.store(true, Ordering::Release);
        }
    }
}
// The existing default FIFO has one worker thread; each explicit isolated route
// has its own thread. Identical fixture account IDs cannot revoke another route.
std::thread_local! { static LIVE: RefCell<Registry> = RefCell::new(Registry::default()); }
pub(super) fn revoke_team(account: &str, team: &str) {
    LIVE.with(|registry| registry.borrow_mut().revoke(account, Some(team)));
}
pub(super) fn revoke_account(account: &str) {
    LIVE.with(|registry| registry.borrow_mut().revoke(account, None));
}
pub(super) fn register(account: &str, team: &str, latch: &Arc<AtomicBool>) {
    LIVE.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.sweep();
        registry
            .0
            .entry((account.into(), team.into()))
            .or_default()
            .push(Arc::downgrade(latch));
    });
}
