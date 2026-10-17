//! Small process-local Retry-After fence survives closing/reopening a reader.
//! No task polls this store. Exhaustion conservatively fences all readers until
//! the latest retained deadline; unrepresentable server delays stay blocked.
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
            Self::Until(t) => t > now,
            Self::Blocked => true,
        }
    }
    fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::Until(a), Self::Until(b)) => Self::Until(a.max(b)),
            _ => Self::Blocked,
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
        let overflow = self.overflow.filter(|v| v.active(now));
        match (own, overflow) {
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
pub(super) fn remember(shell: &ShellState, seconds: Option<u64>) {
    let Some(key) = key(shell) else { return };
    let now = Instant::now();
    let wait = now
        .checked_add(std::time::Duration::from_secs(seconds.unwrap_or(60)))
        .map(Wait::Until)
        .unwrap_or(Wait::Blocked);
    STORE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .remember(key, wait, now);
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(user: &str) -> Key {
        Key {
            account: Some("fixture".into()),
            team: "T1".into(),
            user: user.into(),
        }
    }
    #[test]
    fn reopen_and_credential_rotation_cannot_shorten_owned_deadline() {
        let now = Instant::now();
        let mut s = Store::default();
        s.remember(
            key("U1"),
            Wait::Until(now + std::time::Duration::from_secs(20)),
            now,
        );
        s.remember(
            key("U1"),
            Wait::Until(now + std::time::Duration::from_secs(5)),
            now,
        );
        assert!(
            s.get(&key("U1"), now + std::time::Duration::from_secs(10))
                .is_some()
        );
        assert!(s.get(&key("U2"), now).is_none());
        assert!(
            s.get(&key("U1"), now + std::time::Duration::from_secs(21))
                .is_none()
        );
    }
    #[test]
    fn overflow_never_discards_a_server_delay() {
        let now = Instant::now();
        let mut s = Store::default();
        for i in 0..17 {
            s.remember(key(&i.to_string()), Wait::Blocked, now)
        }
        assert_eq!(s.entries.len(), 16);
        assert!(matches!(s.get(&key("new"), now), Some(Wait::Blocked)));
    }
}
