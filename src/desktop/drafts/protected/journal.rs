//! Account-owned retained raw completions; none contains replay authority.
use super::*;
use crate::drafts::network::{Attempt, Readback};
pub(super) type Key = (DraftKey, String);
#[derive(Default)]
pub(super) struct Journal {
    pub mutations: BTreeMap<Key, Arc<Mutation>>,
    pub checks: BTreeMap<Key, Arc<Check>>,
    pub deferred_checks: BTreeMap<Key, super::recovery::DeferredCheck>,
    pub baselines: BTreeMap<Key, Arc<super::baseline::Completion>>,
    pub deferred_baselines: BTreeMap<Key, super::baseline::DeferredBaseline>,
    pub next_job: u64,
    pub active: BTreeMap<u64, Scope>,
    repairs: std::collections::BTreeSet<(Key, RetainedKind)>,
}
pub(super) struct Mutation {
    pub parent: Option<operation::Operation>,
    pub receipt: durable::Receipt,
    pub attempt: Arc<Attempt>,
}
pub(super) struct Check {
    pub completion: durable::CheckCompletion,
    pub result: durable::CheckResult,
    pub read: Arc<Readback>,
}
pub(super) struct Active {
    account: Arc<Account>,
    id: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RetainedKind {
    MutationStorage,
    CheckStorage,
    BaselineStorage,
    DeferredCheck,
    DeferredBaseline,
}
pub(crate) struct Retained {
    pub key: DraftKey,
    pub id: String,
    pub kind: RetainedKind,
}
pub(super) struct Repair {
    account: Arc<Account>,
    key: Key,
    kind: RetainedKind,
}
impl Repair {
    pub fn begin(owner: &Owner, id: &str, kind: RetainedKind) -> Result<Self, durable::Error> {
        let key = (owner.scope.key.clone(), id.to_owned());
        let mut journal = owner
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if journal.active.values().any(|scope| scope == owner.scope()) {
            return Err(durable::Error::Pending);
        }
        let exists = match kind {
            RetainedKind::MutationStorage => journal.mutations.contains_key(&key),
            RetainedKind::CheckStorage => journal.checks.contains_key(&key),
            RetainedKind::BaselineStorage => journal.baselines.contains_key(&key),
            _ => false,
        };
        if !exists {
            return Err(durable::Error::Pending);
        }
        if !journal.repairs.insert((key.clone(), kind)) {
            return Err(durable::Error::Pending);
        }
        Ok(Self {
            account: owner.account.clone(),
            key,
            kind,
        })
    }
}
impl Drop for Repair {
    fn drop(&mut self) {
        if let Ok(mut journal) = self.account.journal.lock() {
            journal.repairs.remove(&(self.key.clone(), self.kind));
        }
    }
}
impl Account {
    /// Presentation only. An ID never recreates a receipt, request or ticket;
    /// explicit retry methods require the retained opaque value in this Account.
    pub fn retained(&self) -> Result<Vec<Retained>, durable::Error> {
        let journal = self.journal.lock().map_err(|_| durable::Error::Storage)?;
        let mut values = Vec::new();
        for (keys, kind) in [
            (
                journal.mutations.keys().collect::<Vec<_>>(),
                RetainedKind::MutationStorage,
            ),
            (journal.checks.keys().collect(), RetainedKind::CheckStorage),
            (
                journal.baselines.keys().collect(),
                RetainedKind::BaselineStorage,
            ),
            (
                journal.deferred_checks.keys().collect(),
                RetainedKind::DeferredCheck,
            ),
            (
                journal.deferred_baselines.keys().collect(),
                RetainedKind::DeferredBaseline,
            ),
        ] {
            values.extend(keys.into_iter().map(|(key, id)| Retained {
                key: key.clone(),
                id: id.clone(),
                kind,
            }));
        }
        Ok(values)
    }
}
impl Journal {
    fn occupied(&self) -> usize {
        self.active.len()
            + self.mutations.len()
            + self.checks.len()
            + self.deferred_checks.len()
            + self.baselines.len()
            + self.deferred_baselines.len()
    }
    // Caller holds the journal lock. Check all fallible admission work before
    // removing the unique ticket; replacing its slot works even at capacity.
    fn transfer<T>(
        &mut self,
        scope: &Scope,
        take: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Result<(u64, T), durable::Error> {
        let id = self
            .next_job
            .checked_add(1)
            .ok_or(durable::Error::Unsupported)?;
        let retained = take(self).ok_or(durable::Error::Pending)?;
        self.next_job = id;
        self.active.insert(id, scope.clone());
        Ok((id, retained))
    }
    pub fn busy_team(&self, team: &str) -> bool {
        self.active.values().any(|scope| scope.key.team == team)
            || self.repairs.iter().any(|((key, _), _)| key.team == team)
            || self.mutations.keys().any(|(key, _)| key.team == team)
            || self.checks.keys().any(|(key, _)| key.team == team)
            || self.deferred_checks.keys().any(|(key, _)| key.team == team)
            || self.baselines.keys().any(|(key, _)| key.team == team)
            || self
                .deferred_baselines
                .keys()
                .any(|(key, _)| key.team == team)
    }
    pub fn busy_scope(&self, scope: &Scope) -> bool {
        self.active.values().any(|current| current == scope)
            || self.repairs.iter().any(|((key, _), _)| key == &scope.key)
            || self.mutations.keys().any(|(key, _)| key == &scope.key)
            || self.checks.keys().any(|(key, _)| key == &scope.key)
            || self
                .deferred_checks
                .keys()
                .any(|(key, _)| key == &scope.key)
            || self.baselines.keys().any(|(key, _)| key == &scope.key)
            || self
                .deferred_baselines
                .keys()
                .any(|(key, _)| key == &scope.key)
    }
}
impl Active {
    pub fn resume_check(
        owner: &Owner,
        operation: &str,
    ) -> Result<(Self, super::recovery::DeferredCheck), durable::Error> {
        let mut journal = owner
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        let (id, deferred) = journal.transfer(&owner.scope, |journal| {
            journal
                .deferred_checks
                .remove(&(owner.scope.key.clone(), operation.to_owned()))
        })?;
        Ok((
            Self {
                account: owner.account.clone(),
                id,
            },
            deferred,
        ))
    }
    pub fn resume_baseline(
        owner: &Owner,
        enrollment: &str,
    ) -> Result<(Self, super::baseline::DeferredBaseline), durable::Error> {
        let mut journal = owner
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        let (id, deferred) = journal.transfer(&owner.scope, |journal| {
            journal
                .deferred_baselines
                .remove(&(owner.scope.key.clone(), enrollment.to_owned()))
        })?;
        Ok((
            Self {
                account: owner.account.clone(),
                id,
            },
            deferred,
        ))
    }
    pub fn begin(owner: &Owner) -> Result<Self, durable::Error> {
        let mut journal = owner
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if journal.occupied() >= retention::MAX_JOBS {
            return Err(durable::Error::Pending);
        }
        let id = journal
            .next_job
            .checked_add(1)
            .ok_or(durable::Error::Unsupported)?;
        journal.next_job = id;
        journal.active.insert(id, owner.scope().clone());
        Ok(Self {
            account: owner.account.clone(),
            id,
        })
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        if let Ok(mut journal) = self.account.journal.lock() {
            journal.active.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> Scope {
        Scope {
            account_id: "isolated-journal".into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        }
    }
    #[test]
    fn slot_transfer_works_at_capacity_without_an_unreserved_interval() {
        let mut journal = Journal::default();
        for id in 1..=retention::MAX_JOBS as u64 {
            journal.active.insert(id, scope());
        }
        journal.next_job = retention::MAX_JOBS as u64;
        assert_eq!(journal.occupied(), retention::MAX_JOBS);
        // A plain scope marker exercises the same locked transition without
        // fabricating an opaque request ticket or claiming HTTP coverage.
        let (id, retained) = journal.transfer(&scope(), |j| j.active.remove(&1)).unwrap();
        assert_eq!(id, retention::MAX_JOBS as u64 + 1);
        assert_eq!(retained, scope());
        assert_eq!(journal.occupied(), retention::MAX_JOBS);
        assert!(!journal.active.contains_key(&1));
        assert!(journal.active.contains_key(&id));
    }
    #[test]
    fn failed_transfer_never_spends_a_retained_value_or_job_number() {
        let mut journal = Journal::default();
        journal.next_job = u64::MAX;
        let mut retained = Some(7);
        assert!(matches!(
            journal.transfer(&scope(), |_| retained.take()),
            Err(durable::Error::Unsupported)
        ));
        assert_eq!(retained, Some(7));
        journal.next_job = 9;
        assert!(matches!(
            journal.transfer::<u8>(&scope(), |_| None),
            Err(durable::Error::Pending)
        ));
        assert_eq!(journal.next_job, 9);
        assert!(journal.active.is_empty());
    }
}
