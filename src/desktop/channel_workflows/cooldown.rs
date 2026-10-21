//! Retained read throttles, not read authority. This std-only production module
//! can be tested independently of the desktop renderer/linker.
use std::time::{Duration, Instant};

const LIMIT: usize = 64;
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Scope {
    pub(crate) account: Option<String>,
    pub(crate) epoch: u64,
    pub(crate) team: String,
}
impl Scope {
    fn same_account(&self, other: &Self) -> bool {
        self.account == other.account && self.epoch == other.epoch
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Deadline {
    Until(Instant),
    Indefinite,
}
impl Deadline {
    fn active(self, now: Instant) -> bool {
        match self {
            Self::Until(t) => now < t,
            Self::Indefinite => true,
        }
    }
    fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::Until(a), Self::Until(b)) => Self::Until(a.max(b)),
            _ => Self::Indefinite,
        }
    }
}
struct Entry {
    scope: Scope,
    method: &'static str,
    deadline: Deadline,
    seconds: u64,
}
#[derive(Default)]
pub(crate) struct Root {
    entries: Vec<Entry>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Gate {
    pub(crate) blocked: bool,
    pub(crate) wake_at: Option<Instant>,
    pub(crate) retry_after_seconds: Option<u64>,
    pub(crate) indefinite: bool,
}
impl Gate {
    fn from_entry(entry: &Entry) -> Self {
        match entry.deadline {
            Deadline::Until(t) => Self {
                blocked: true,
                wake_at: Some(t),
                retry_after_seconds: Some(entry.seconds),
                indefinite: false,
            },
            Deadline::Indefinite => Self {
                blocked: true,
                indefinite: true,
                ..Self::default()
            },
        }
    }
}
impl Root {
    /// Call only after the original read owner has been rechecked. An observed
    /// server deadline survives a Transport/socket replacement in this scope.
    pub(crate) fn record(
        &mut self,
        scope: Scope,
        method: &'static str,
        retry_after: Option<u64>,
        now: Instant,
    ) -> bool {
        self.entries
            .retain(|e| e.scope.same_account(&scope) && e.deadline.active(now));
        let seconds = retry_after.unwrap_or(60);
        let deadline = now
            .checked_add(Duration::from_secs(seconds))
            .map(Deadline::Until)
            .unwrap_or(Deadline::Indefinite);
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.scope == scope && e.method == method)
        {
            let next = entry.deadline.max(deadline);
            if next != entry.deadline {
                entry.seconds = seconds;
            }
            entry.deadline = next;
            return true;
        }
        if self.entries.len() == LIMIT {
            return false;
        }
        if deadline.active(now) {
            self.entries.push(Entry {
                scope,
                method,
                deadline,
                seconds,
            });
        }
        true
    }
    pub(crate) fn gate(&self, scope: &Scope, now: Instant) -> Gate {
        let active: Vec<_> = self
            .entries
            .iter()
            .filter(|e| e.scope.same_account(scope) && e.deadline.active(now))
            .collect();
        let matching = active
            .iter()
            .copied()
            .filter(|e| e.scope == *scope)
            .max_by_key(|e| match e.deadline {
                Deadline::Until(t) => (false, Some(t)),
                Deadline::Indefinite => (true, None),
            });
        if let Some(entry) = matching {
            return Gate::from_entry(entry);
        }
        // Never evict an unexpired throttle merely to admit a new scope. An
        // exhausted store denies new reads until at least one slot expires.
        if active.len() == LIMIT {
            let next = active
                .iter()
                .filter_map(|e| match e.deadline {
                    Deadline::Until(t) => Some(t),
                    Deadline::Indefinite => None,
                })
                .min();
            return Gate {
                blocked: true,
                wake_at: next,
                retry_after_seconds: None,
                indefinite: next.is_none(),
            };
        }
        Gate::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope(team: &str) -> Scope {
        Scope {
            account: Some("original".into()),
            epoch: 7,
            team: team.into(),
        }
    }
    #[test]
    fn exact_deadline_and_reentry_do_not_shorten_retry_after() {
        let now = Instant::now();
        let mut root = Root::default();
        let key = scope("T1");
        assert!(root.record(key.clone(), "preview", Some(90), now));
        assert!(root.gate(&key, now + Duration::from_secs(89)).blocked);
        // A new panel/Transport has the same server scope; lookup needs no old
        // component or Transport identity to preserve the deadline.
        assert!(
            root.gate(&scope("T1"), now + Duration::from_secs(89))
                .blocked
        );
        assert!(!root.gate(&key, now + Duration::from_secs(90)).blocked);
    }
    #[test]
    fn extension_does_not_shorten_and_old_wake_cannot_clear_new_deadline() {
        let now = Instant::now();
        let mut root = Root::default();
        let key = scope("T1");
        root.record(key.clone(), "preview", Some(12), now);
        let old_wake = root.gate(&key, now).wake_at.unwrap();
        root.record(
            key.clone(),
            "preview",
            Some(1),
            now + Duration::from_secs(1),
        );
        assert_eq!(root.gate(&key, now).wake_at, Some(old_wake));
        root.record(
            key.clone(),
            "preview",
            Some(20),
            now + Duration::from_secs(2),
        );
        assert!(root.gate(&key, old_wake).blocked);
        assert!(!root.gate(&key, now + Duration::from_secs(22)).blocked);
    }
    #[test]
    fn missing_zero_and_unrepresentable_deadlines_are_explicit() {
        let now = Instant::now();
        let mut root = Root::default();
        let key = scope("T1");
        root.record(key.clone(), "preview", None, now);
        assert_eq!(root.gate(&key, now).retry_after_seconds, Some(60));
        assert!(!root.gate(&key, now + Duration::from_secs(60)).blocked);
        root.record(
            key.clone(),
            "preview",
            Some(0),
            now + Duration::from_secs(60),
        );
        assert!(!root.gate(&key, now + Duration::from_secs(60)).blocked);
        root.record(key.clone(), "preview", Some(u64::MAX), now);
        assert!(root.gate(&key, now).indefinite);
        root.record(key.clone(), "preview", Some(1), now);
        assert!(root.gate(&key, now).indefinite);
    }
    #[test]
    fn account_team_and_method_scopes_do_not_waive_other_active_methods() {
        let now = Instant::now();
        let mut root = Root::default();
        let key = scope("T1");
        root.record(key.clone(), "bookmarks", Some(10), now);
        root.record(key.clone(), "preview", Some(20), now);
        assert!(root.gate(&key, now + Duration::from_secs(10)).blocked);
        assert!(!root.gate(&scope("T2"), now).blocked);
        let mut successor = key.clone();
        successor.epoch += 1;
        assert!(!root.gate(&successor, now).blocked);
        successor = key;
        successor.account = Some("replacement".into());
        assert!(!root.gate(&successor, now).blocked);
    }
    #[test]
    fn capacity_never_evicts_live_deadlines_to_allow_another_read() {
        let now = Instant::now();
        let mut root = Root::default();
        for i in 0..LIMIT {
            assert!(root.record(scope(&format!("T{i}")), "preview", Some(12), now));
        }
        let extra = scope("extra");
        assert!(root.gate(&extra, now).blocked);
        assert!(!root.record(extra.clone(), "preview", Some(12), now));
        assert!(root.gate(&scope("T0"), now).blocked);
        assert!(!root.gate(&extra, now + Duration::from_secs(12)).blocked);
        assert!(root.record(extra, "preview", Some(1), now + Duration::from_secs(12)));
        assert_eq!(root.entries.len(), 1);
    }
}
