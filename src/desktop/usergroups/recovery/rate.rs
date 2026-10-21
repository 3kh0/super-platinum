//! Endpoint debt outlives an opening, but is never an instruction to retry.
//! Credentials authenticate each received response; the stable account/team/user
//! key prevents Cancel, reopen, or credential rotation from bypassing its delay.
use super::*;
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    account: String,
    team: String,
    user: String,
}
impl Key {
    fn capture(owner: &Context) -> Self {
        Self {
            account: owner.account.clone(),
            team: owner.session.team_id.clone(),
            user: owner.session.user_id.clone(),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Endpoint {
    Directory,
    Direct,
}
impl Endpoint {
    fn index(self) -> usize {
        match self {
            Self::Directory => 0,
            Self::Direct => 1,
        }
    }
}
struct Debt {
    revision: u64,
    header: Option<u64>,
    received: Instant,
    // None means unrepresentably distant, not an absent header. Missing headers
    // use the existing member-recovery default of sixty seconds.
    deadline: Option<Instant>,
    notified: bool,
}
impl Debt {
    fn blocked(&self, now: Instant) -> bool {
        self.deadline.is_none_or(|deadline| now < deadline)
    }
    fn error(&self, now: Instant) -> Error {
        Error::RateLimited {
            retry_after_secs: self.deadline.map(|deadline| {
                let remaining = deadline.saturating_duration_since(now);
                remaining
                    .as_secs()
                    .saturating_add(u64::from(remaining.subsec_nanos() != 0))
            }),
        }
    }
    fn snapshot(&self, now: Instant) -> Value {
        serde_json::json!({
            "revision":self.revision,
            "retryAfterSecs":self.header,
            "blocked":self.blocked(now),
        })
    }
}
#[derive(Default)]
struct Store {
    next: u64,
    entries: BTreeMap<Key, [Option<Debt>; 2]>,
}
impl Store {
    fn record(&mut self, key: Key, endpoint: Endpoint, header: Option<u64>, now: Instant) {
        let deadline = now.checked_add(Duration::from_secs(header.unwrap_or(60)));
        let debts = self.entries.entry(key).or_default();
        let slot = &mut debts[endpoint.index()];
        let deadline = match slot.as_ref() {
            // A concurrent shorter response must never shorten existing debt.
            Some(previous) => match (previous.deadline, deadline) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => None,
            },
            None => deadline,
        };
        self.next = self.next.saturating_add(1);
        *slot = Some(Debt {
            revision: self.next,
            header,
            received: now,
            deadline,
            notified: false,
        });
    }
    fn receive(
        &mut self,
        shell: &ShellState,
        owner: &Context,
        endpoint: Endpoint,
        error: &Error,
        now: Instant,
    ) {
        if let Error::RateLimited { retry_after_secs } = error {
            // Consumer/source retirement is separate from the account authority
            // that actually received this response. Never capture a new owner.
            if owner.account_current(shell) {
                self.record(Key::capture(owner), endpoint, *retry_after_secs, now);
            }
        }
    }
    fn debt(&self, key: &Key, endpoint: Endpoint) -> Option<&Debt> {
        self.entries.get(key)?[endpoint.index()].as_ref()
    }
    fn status(&self, key: &Key, now: Instant) -> RateStatus {
        let blocked = |endpoint| self.debt(key, endpoint).is_some_and(|d| d.blocked(now));
        RateStatus {
            observed: self.entries.contains_key(key),
            blocked: blocked(Endpoint::Directory) && blocked(Endpoint::Direct),
        }
    }
    fn poll(&mut self, key: &Key, now: Instant) -> bool {
        let Some(debts) = self.entries.get_mut(key) else {
            return false;
        };
        let mut changed = false;
        for debt in debts.iter_mut().flatten() {
            if !debt.notified && !debt.blocked(now) {
                debt.notified = true;
                changed = true;
            }
        }
        changed
    }
}
fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::default()))
}

#[derive(Clone, Copy, Default)]
pub(crate) struct RateStatus {
    pub(crate) observed: bool,
    pub(crate) blocked: bool,
}
pub(crate) const LIMITED: &str =
    "Slack asked us to wait before loading group names. Your description is unchanged.";
pub(crate) const READY: &str =
    "You can retry loading group names now. Your description is unchanged.";

pub(super) fn receive(shell: &ShellState, owner: &Context, endpoint: Endpoint, error: &Error) {
    store()
        .lock()
        .unwrap()
        .receive(shell, owner, endpoint, error, Instant::now());
}
pub(super) fn check(owner: &Context, endpoint: Endpoint) -> Result<(), Error> {
    let now = Instant::now();
    let store = store().lock().unwrap();
    match store
        .debt(&Key::capture(owner), endpoint)
        .filter(|d| d.blocked(now))
    {
        Some(debt) => Err(debt.error(now)),
        None => Ok(()),
    }
}
pub(crate) fn rate_status(shell: &ShellState, team: &str) -> RateStatus {
    let Ok(owner) = Context::capture(shell, team) else {
        return RateStatus::default();
    };
    store()
        .lock()
        .unwrap()
        .status(&Key::capture(&owner), Instant::now())
}
pub(crate) fn poll_rate(shell: &ShellState, team: &str) -> bool {
    let Ok(owner) = Context::capture(shell, team) else {
        return false;
    };
    store()
        .lock()
        .unwrap()
        .poll(&Key::capture(&owner), Instant::now())
}
pub(crate) fn rate_snapshot(shell: &ShellState, team: &str) -> Value {
    let Ok(owner) = Context::capture(shell, team) else {
        return Value::Null;
    };
    let key = Key::capture(&owner);
    let now = Instant::now();
    let store = store().lock().unwrap();
    let status = store.status(&key, now);
    serde_json::json!({
        "scope":{"account":key.account,"team":key.team,"user":key.user},
        "directory":store.debt(&key,Endpoint::Directory).map(|d|d.snapshot(now)),
        "direct":store.debt(&key,Endpoint::Direct).map(|d|d.snapshot(now)),
        "observed":status.observed,"blocked":status.blocked,
    })
}
pub(crate) fn rate_elapsed_ms(shell: &ShellState, team: &str, endpoint: &str) -> Option<u128> {
    let endpoint = match endpoint {
        "directory" => Endpoint::Directory,
        "direct" => Endpoint::Direct,
        _ => return None,
    };
    let owner = Context::capture(shell, team).ok()?;
    store()
        .lock()
        .unwrap()
        .debt(&Key::capture(&owner), endpoint)
        .map(|d| d.received.elapsed().as_millis())
}

#[cfg(test)]
mod tests;
