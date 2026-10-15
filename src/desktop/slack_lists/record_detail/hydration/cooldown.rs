//! Process-retained hydration-only cooldowns, independent of projected visits.
//! No credentials or payloads are stored. Shell replacement does not reset them.
use super::*;
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

type Owner = (Option<String>, String, String);
#[derive(Clone, Copy)]
struct Deadline(Option<Instant>);
impl Deadline {
    fn active(self, now: Instant) -> bool {
        self.0.is_none_or(|deadline| deadline > now)
    }
    fn extend(self, other: Self) -> Self {
        Self(match (self.0, other.0) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        })
    }
}
#[derive(Default)]
struct Table(BTreeMap<Owner, Deadline>);
impl Table {
    fn prune(&mut self, now: Instant) {
        self.0.retain(|_, deadline| deadline.active(now));
    }
    fn blocked(&mut self, owner: &Owner, now: Instant) -> bool {
        self.prune(now);
        self.0.contains_key(owner)
    }
    fn record(&mut self, owner: Owner, seconds: u64, now: Instant) {
        self.prune(now);
        // A duration outside Instant's range cannot become permission to retry.
        let incoming = Deadline(now.checked_add(Duration::from_secs(seconds.max(1))));
        self.0
            .entry(owner)
            .and_modify(|old| *old = old.extend(incoming))
            .or_insert(incoming);
    }
}
fn retained() -> &'static Mutex<Table> {
    static RETAINED: OnceLock<Mutex<Table>> = OnceLock::new();
    RETAINED.get_or_init(Default::default)
}
fn seconds(error: &Error) -> Option<u64> {
    match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => Some(retry_after_secs.unwrap_or(30).max(1)),
        Error::HttpStatus {
            retry_after_secs: Some(seconds),
            ..
        } => Some((*seconds).max(1)),
        _ => None,
    }
}
pub(super) fn blocked(scope: &Scope) -> bool {
    retained()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .blocked(&scope.stable_owner(), Instant::now())
}
/// Record against the captured request owner even if the active account or List
/// has changed. Returned wall-clock hint serves existing UI only; admission uses
/// the retained monotonic deadline, including after ShellState replacement.
pub(super) fn record(scope: &Scope, error: &Error) -> Option<i64> {
    let seconds = seconds(error)?;
    retained().lock().unwrap_or_else(|p| p.into_inner()).record(
        scope.stable_owner(),
        seconds,
        Instant::now(),
    );
    Some(now().saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner(account: &str) -> Owner {
        (Some(account.into()), "T".into(), "U".into())
    }
    #[test]
    fn all_http_retry_after_values_and_default_429_are_preserved() {
        assert_eq!(
            seconds(&Error::RateLimited {
                retry_after_secs: None
            }),
            Some(30)
        );
        assert_eq!(
            seconds(&Error::HttpStatus {
                status: 429,
                retry_after_secs: None
            }),
            Some(30)
        );
        assert_eq!(
            seconds(&Error::HttpStatus {
                status: 503,
                retry_after_secs: Some(75)
            }),
            Some(75)
        );
        assert_eq!(
            seconds(&Error::HttpStatus {
                status: 503,
                retry_after_secs: None
            }),
            None
        );
        assert_eq!(
            seconds(&Error::RateLimited {
                retry_after_secs: Some(0)
            }),
            Some(1)
        );
    }
    #[test]
    fn monotonic_owner_deadlines_extend_expire_and_fail_closed_on_overflow() {
        let now = Instant::now();
        let mut table = Table::default();
        let a = owner("A");
        let b = owner("B");
        table.record(a.clone(), 30, now);
        table.record(a.clone(), 2, now);
        assert!(table.blocked(&a, now + Duration::from_secs(29)));
        assert!(!table.blocked(&b, now));
        table.record(a.clone(), 60, now);
        assert!(table.blocked(&a, now + Duration::from_secs(59)));
        assert!(!table.blocked(&a, now + Duration::from_secs(60)));
        table.record(a.clone(), u64::MAX, now);
        assert!(table.blocked(&a, now + Duration::from_secs(3600)));
    }
}
