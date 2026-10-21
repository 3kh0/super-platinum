//! files.info debt outlives the component which received it. This endpoint's
//! fence never blocks comment reads or imports another endpoint's cooldown.
use super::*;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, PartialEq, Eq)]
struct Key {
    account: Option<String>,
    team: String,
    user: String,
}
#[derive(Clone, Copy)]
pub(super) enum Wait {
    Until(Instant),
    Blocked,
}
impl Wait {
    fn active(self, now: Instant) -> bool {
        match self {
            Self::Until(at) => at > now,
            Self::Blocked => true,
        }
    }
    fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::Until(a), Self::Until(b)) => Self::Until(a.max(b)),
            _ => Self::Blocked,
        }
    }
    pub(super) fn deadline(self) -> Option<Instant> {
        match self {
            Self::Until(at) => Some(at),
            Self::Blocked => None,
        }
    }
}
#[derive(Default)]
struct Store {
    entries: Vec<(Key, Wait)>,
    overflow: Option<Wait>,
}
impl Store {
    fn remember(&mut self, key: Key, wait: Wait, now: Instant) {
        self.entries.retain(|(_, v)| v.active(now));
        if let Some((_, old)) = self.entries.iter_mut().find(|(k, _)| k == &key) {
            *old = old.max(wait);
        } else if self.entries.len() < 16 {
            self.entries.push((key, wait));
        } else {
            self.overflow = Some(self.overflow.map_or(wait, |old| old.max(wait)));
        }
    }
    fn get(&self, key: &Key, now: Instant) -> Option<Wait> {
        let own = self
            .entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| *v)
            .filter(|v| v.active(now));
        match (own, self.overflow.filter(|v| v.active(now))) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }
}
static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
fn key(shell: &ShellState) -> Option<Key> {
    Some(Key {
        account: shell.core.active_account.clone(),
        team: shell.core.active_team.clone()?,
        user: shell.self_account.user_id.clone(),
    })
}
pub(super) fn current(shell: &ShellState) -> Option<Wait> {
    STORE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .get(&key(shell)?, Instant::now())
}
pub(super) fn remember(shell: &ShellState, deadline: Option<Instant>) {
    let Some(key) = key(shell) else { return };
    STORE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .remember(
            key,
            deadline.map(Wait::Until).unwrap_or(Wait::Blocked),
            Instant::now(),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(account: &str, team: &str, user: &str) -> Key {
        Key {
            account: Some(account.into()),
            team: team.into(),
            user: user.into(),
        }
    }
    #[test]
    fn endpoint_debt_outlives_view_and_credentials_but_not_identity() {
        let now = Instant::now();
        let mut store = Store::default();
        let original = key("account", "T1", "U1");
        let deadline = now + std::time::Duration::from_secs(15);
        store.remember(original.clone(), Wait::Until(deadline), now);
        // Reopening/credential rotation creates a fresh Scope, but the same
        // stable identity cannot shorten the endpoint's original deadline.
        store.remember(key("account", "T1", "U1"), Wait::Until(now), now);
        assert_eq!(
            store.get(&original, now).unwrap().deadline(),
            Some(deadline)
        );
        for other in [
            key("foreign", "T1", "U1"),
            key("account", "T2", "U1"),
            key("account", "T1", "U2"),
        ] {
            assert!(store.get(&other, now).is_none());
        }
        assert!(store.get(&original, deadline).is_none());
        assert_eq!(store.entries.len(), 1, "expiry lookup never schedules work");
    }
    #[test]
    fn bounded_store_and_unrepresentable_delay_fail_closed() {
        let now = Instant::now();
        let mut store = Store::default();
        for i in 0..17 {
            store.remember(key(&i.to_string(), "T1", "U1"), Wait::Blocked, now);
        }
        assert_eq!(store.entries.len(), 16);
        assert!(matches!(
            store.get(&key("other", "T1", "U1"), now),
            Some(Wait::Blocked)
        ));
    }
}
