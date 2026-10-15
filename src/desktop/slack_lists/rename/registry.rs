//! Bounded process retention. No credentials, dispatch permits or replay actions.
use super::*;
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
pub(super) const MAX_ENTRIES: usize = 32;
pub(super) const MAX_DOCUMENT: usize = 2 * 1024 * 1024;
pub(super) type Owner = (Option<String>, String, String);
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Key {
    pub owner: Owner,
    pub list: String,
}
impl Key {
    pub fn current(shell: &ShellState) -> Option<Self> {
        if shell.overlay != Some(Overlay::SlackList) {
            return None;
        }
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
    Editing,
    Preparing,
    Saving,
    NotRequested,
    Uncertain,
    Checking,
    Conflict,
    Observed,
    Available,
}
#[derive(Clone)]
pub(super) struct Entry {
    pub serial: u64,
    pub job: Option<u64>,
    pub target: Target,
    pub view_selection: Option<u64>,
    pub selected: Option<String>,
    pub available: Option<String>,
    pub name: String,
    pub before: String,
    pub original: Document,
    pub intent: Option<Intent>,
    pub stage: Stage,
    pub message: String,
    pub review: Option<owner::Review>,
    pub raw_reply: Option<Value>,
}
#[derive(Default)]
pub(super) struct Registry {
    pub entries: BTreeMap<Key, Entry>,
    deadlines: BTreeMap<Owner, Option<Instant>>,
    overflow: bool,
}
pub(super) fn with_registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    f(&mut REGISTRY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner()))
}
pub(super) fn entry(shell: &ShellState) -> Option<Entry> {
    let key = Key::current(shell)?;
    with_registry(|r| r.entries.get(&key).cloned())
}
fn seconds(error: &Error) -> Option<u64> {
    match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => Some(retry_after_secs.unwrap_or(30).max(1)),
        Error::HttpStatus {
            retry_after_secs: Some(s),
            ..
        } => Some((*s).max(1)),
        _ => None,
    }
}
impl Registry {
    fn prune(&mut self, now: Instant) {
        self.deadlines.retain(|_, d| d.is_none_or(|d| d > now));
    }
    fn record(&mut self, owner: &Owner, seconds: u64, now: Instant) {
        self.prune(now);
        if !self.deadlines.contains_key(owner) && self.deadlines.len() >= 128 {
            self.overflow = true;
            return;
        }
        let incoming = now.checked_add(Duration::from_secs(seconds.max(1)));
        self.deadlines
            .entry(owner.clone())
            .and_modify(|old| {
                *old = match (*old, incoming) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    _ => None,
                }
            })
            .or_insert(incoming);
    }
    fn blocked(&mut self, owner: &Owner, now: Instant) -> bool {
        self.prune(now);
        self.overflow || self.deadlines.contains_key(owner)
    }
}
pub(super) fn blocked(owner: &Owner) -> bool {
    with_registry(|r| r.blocked(owner, Instant::now()))
}
pub(super) fn blocked_at(owner: &Owner, now: Instant) -> bool {
    with_registry(|r| r.blocked(owner, now))
}
pub(super) fn record(owner: &Owner, error: &Error) {
    if let Some(s) = seconds(error) {
        with_registry(|r| r.record(owner, s, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cooldown_extends_survives_owner_switch_and_overflow_fails_closed() {
        let mut r = Registry::default();
        let now = Instant::now();
        let a = (Some("A".into()), "T".into(), "U".into());
        let b = (Some("B".into()), "T".into(), "U".into());
        r.record(&a, 30, now);
        r.record(&a, 2, now);
        assert!(r.blocked(&a, now + Duration::from_secs(29)));
        assert!(!r.blocked(&b, now));
        assert!(!r.blocked(&a, now + Duration::from_secs(30)));
        r.record(&a, u64::MAX, now);
        assert!(r.blocked(&a, now + Duration::from_secs(999)));
    }
    #[test]
    fn known_non429_limit_and_headerless429_have_positive_floor() {
        assert_eq!(
            seconds(&Error::RateLimited {
                retry_after_secs: None
            }),
            Some(30)
        );
        assert_eq!(
            seconds(&Error::HttpStatus {
                status: 503,
                retry_after_secs: Some(8)
            }),
            Some(8)
        );
        assert_eq!(
            seconds(&Error::HttpStatus {
                status: 200,
                retry_after_secs: Some(0)
            }),
            Some(1)
        );
    }
}
