//! Account-owned process retention with capacity reserved before detached work.
use super::*;
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};
pub(super) type Owner = (Option<String>, String, String);
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Key {
    pub owner: Owner,
    pub list: String,
}
impl Key {
    pub fn current(shell: &ShellState) -> Option<Self> {
        let scope = shell
            .slack_lists
            .scope
            .as_ref()
            .filter(|s| s.current(shell))?;
        Some(Self {
            owner: scope.stable_owner(),
            list: shell.slack_lists.view.as_ref()?.id.clone(),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Stage {
    Preparing,
    Sending,
    NotRequested,
    Uncertain,
    AwaitingRead,
    Checking,
    Available,
    Reviewed,
}
impl Stage {
    pub fn busy(self) -> bool {
        matches!(self, Self::Preparing | Self::Sending | Self::Checking)
    }
}
#[derive(Clone)]
pub(super) struct Entry {
    pub serial: u64,
    pub job: u64,
    pub stage: Stage,
    pub in_flight: bool,
    pub intent: Arc<SubitemCreationIntent>,
    pub timestamp: ListTimestamp,
    pub failure: Option<Failure>,
    pub reply: Option<Arc<Value>>,
    pub read: Option<Arc<Value>>,
    pub child: Option<Record>,
    pub review: Option<owner::Proof>,
    pub retirement: Option<owner::Retirement>,
    pub message: &'static str,
    pub charge: usize,
}
pub(super) const MAX_ENTRIES: usize = 64;
pub(super) const TOTAL_BYTES: usize = 64 * 1024 * 1024;
// Four original-payload copies plus a fixed allowance for later maximum-sized
// check/review snapshots and bounded ACK/read evidence. The fixed allowance is
// independent of the original size: a tiny List may grow before explicit Check.
// At most two operations fit the 64 MiB aggregate. Transient allocator overhead
// and worker-local merge allocations are not byte-identical to JSON; this is an
// encoded-payload reservation, not an allocator/RSS claim.
pub(super) fn charge(input: usize) -> Option<usize> {
    input.checked_mul(4)?.checked_add(24 * 1024 * 1024)
}
#[derive(Default)]
pub(super) struct Registry {
    pub entries: BTreeMap<Key, Entry>,
}
impl Registry {
    pub fn room(&self, key: &Key, charge: usize) -> bool {
        !self.entries.contains_key(key)
            && self.entries.len() < MAX_ENTRIES
            && self
                .entries
                .values()
                .try_fold(charge, |sum, e| sum.checked_add(e.charge))
                .is_some_and(|n| n <= TOTAL_BYTES)
    }
}
#[cfg(not(test))]
pub(super) fn with<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    f(&mut REGISTRY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner()))
}
// Isolated serial UI instance per unit-test thread; replacement inside a test
// retains receipts exactly as the production process-wide registry does.
#[cfg(test)]
thread_local! {static TEST_REGISTRY:std::cell::RefCell<Registry>=Default::default();}
#[cfg(test)]
pub(super) fn with<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    TEST_REGISTRY.with(|r| f(&mut r.borrow_mut()))
}
pub(super) fn entry(shell: &ShellState) -> Option<Entry> {
    let key = Key::current(shell)?;
    with(|r| r.entries.get(&key).cloned())
}

#[derive(Clone, serde::Serialize)]
pub(super) struct Failure {
    kind: &'static str,
    status: Option<u16>,
    retry_after_secs: Option<u64>,
}
impl Failure {
    pub fn from_error(kind: &'static str, error: &Error) -> Self {
        let (status, retry_after_secs) = match error {
            Error::RateLimited { retry_after_secs } => (Some(429), *retry_after_secs),
            Error::HttpStatus {
                status,
                retry_after_secs,
            } => (Some(*status), *retry_after_secs),
            _ => (None, None),
        };
        Self {
            kind,
            status,
            retry_after_secs,
        }
    }
}
