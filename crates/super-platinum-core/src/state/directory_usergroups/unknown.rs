//! Shared rLUS/0mwJ missing-selector custody. Terminal means local cleanup,
//! never server absence. Transport and socket currentness remain caller-owned.
use super::*;
use std::{
    sync::{Mutex, atomic::AtomicU8},
    time::Duration,
};
mod lookup;
mod queue;
pub use lookup::{Lookup, LookupKind};
#[cfg(test)]
mod tests;

pub const CADENCE: Duration = Duration::from_millis(200);
pub const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(65);
pub const MAX_ATTEMPTS: u8 = 3;

#[derive(Clone)]
pub struct Scope {
    account: String,
    owner: WorkspaceSession,
    epoch: u64,
    realtime_generation: u64,
}
impl Scope {
    pub fn new(
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        realtime_generation: u64,
    ) -> Result<Self> {
        if account.is_empty() || owner.team_id.is_empty() || owner.user_id.is_empty() {
            return Err(Error::Owner);
        }
        Ok(Self {
            account: account.into(),
            owner: owner.clone(),
            epoch,
            realtime_generation,
        })
    }
    fn matches(&self, other: &Self) -> bool {
        self.account == other.account
            && same(&self.owner, &other.owner)
            && self.epoch == other.epoch
            && self.realtime_generation == other.realtime_generation
    }
    fn snapshot(&self, store: &Store, native: &Native) -> Result<Snapshot> {
        store.snapshot(&self.account, &self.owner, self.epoch, native)
    }
}

#[derive(Clone)]
pub struct Flight {
    identity: Arc<AtomicU8>,
    // 0 fetching, 1 settlement captured, 2 queued, 3 published.
    phase: Arc<AtomicU8>,
    completed: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    ids: Vec<String>,
    attempt: u8,
}
impl Flight {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn attempt(&self) -> u8 {
        self.attempt
    }
}

#[derive(Clone)]
pub(super) struct Authority {
    alive: Arc<AtomicU8>,
    identity: Arc<AtomicU8>,
    phase: Arc<AtomicU8>,
    attempt: u8,
}
impl Authority {
    pub(super) fn current(&self) -> bool {
        self.alive.load(Ordering::Acquire) == 1
            && self.identity.load(Ordering::Acquire) == self.attempt
            && self.phase.load(Ordering::Acquire) == 2
    }
}
pub struct Missing {
    scope: Scope,
    observed: Snapshot,
    authority: Authority,
    completed: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    ids: Vec<String>,
}
impl Missing {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
}
pub struct Settlement {
    flight: Flight,
    missing: Missing,
}
impl Settlement {
    pub fn missing(&self) -> &Missing {
        &self.missing
    }
}

pub struct Manager {
    scope: Scope,
    alive: Arc<AtomicU8>,
    source: Option<Observation>,
    pending: Vec<String>,
    lookups: BTreeMap<String, (Arc<AtomicU8>, Arc<Value>)>,
    flight: Option<Flight>,
    last_start: Option<Duration>,
}
impl Manager {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            alive: Arc::new(AtomicU8::new(1)),
            source: None,
            pending: vec![],
            lookups: BTreeMap::new(),
            flight: None,
            last_start: None,
        }
    }
    pub fn current(&self, scope: &Scope) -> bool {
        self.scope.matches(scope) && self.alive.load(Ordering::Acquire) == 1
    }
    fn same_source(&self, observed: &Observation) -> bool {
        self.source.as_ref().is_none_or(|source| {
            Arc::ptr_eq(&source.identity, &observed.identity)
                && Arc::ptr_eq(&source.binding, &observed.binding)
        })
    }
    /// Ready successors of the same accepted FIFO history are valid; a boot,
    /// account rebind, discarded operation or forged typed row is not.
    pub fn source_current(&self, scope: &Scope, store: &Store, native: &Native) -> bool {
        self.current(scope)
            && scope
                .snapshot(store, native)
                .is_ok_and(|s| self.same_source(&s.observed))
    }
    pub fn pending(&self) -> &[String] {
        &self.pending
    }
    pub fn flight_current(&self, scope: &Scope, flight: &Flight) -> bool {
        self.current(scope)
            && self.flight.as_ref().is_some_and(|current| {
                Arc::ptr_eq(&current.identity, &flight.identity)
                    && current.attempt == flight.attempt
                    && flight.identity.load(Ordering::Acquire) == flight.attempt
            })
    }
    pub fn remaining(&self, now: Duration) -> Duration {
        self.last_start
            .map(|last| last.saturating_add(CADENCE).saturating_sub(now))
            .unwrap_or_default()
    }
    pub fn begin(&mut self, scope: &Scope, now: Duration) -> Result<Option<Flight>> {
        if !self.current(scope) {
            return Err(Error::Owner);
        }
        if self.flight.is_some() || self.pending.is_empty() || !self.remaining(now).is_zero() {
            return Ok(None);
        }
        if self.source.is_none() {
            return Err(Error::Unknown);
        }
        let flight = Flight {
            identity: Arc::new(AtomicU8::new(1)),
            phase: Arc::new(AtomicU8::new(0)),
            completed: Arc::new(Mutex::new(None)),
            ids: self.pending.clone(),
            attempt: 1,
        };
        self.last_start = Some(now);
        self.flight = Some(flight.clone());
        Ok(Some(flight))
    }
    /// Ordinary wrapper rejection settles its returned/retained IDs; only an
    /// outer timeout retries, with the original bounded three attempts.
    pub fn retry_timeout(&mut self, scope: &Scope, flight: &Flight) -> Result<Option<Flight>> {
        if !self.flight_current(scope, flight) || flight.phase.load(Ordering::Acquire) != 0 {
            return Err(Error::Stale);
        }
        if flight.attempt >= MAX_ATTEMPTS {
            return Ok(None);
        }
        let mut next = flight.clone();
        next.attempt += 1;
        next.identity.store(next.attempt, Ordering::Release);
        self.flight = Some(next.clone());
        Ok(Some(next))
    }
    /// Returned IDs include actual results AND any truthy dispatch-retained
    /// source rows, including unknown/nonexistent rows even on raw rejection.
    /// Caller must supply exact accepted acquisition evidence and current socket.
    pub fn settlement(
        &self,
        scope: &Scope,
        flight: &Flight,
        store: &Store,
        native: &Native,
        returned: &[String],
    ) -> Result<Settlement> {
        if !self.flight_current(scope, flight) || returned.iter().any(|id| !flight.ids.contains(id))
        {
            return Err(Error::Stale);
        }
        let observed = scope.snapshot(store, native)?;
        if !self.same_source(&observed.observed) {
            return Err(Error::Stale);
        }
        flight
            .phase
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Stale)?;
        Ok(Settlement {
            flight: flight.clone(),
            missing: Missing {
                scope: scope.clone(),
                observed,
                authority: Authority {
                    alive: self.alive.clone(),
                    identity: flight.identity.clone(),
                    phase: flight.phase.clone(),
                    attempt: flight.attempt,
                },
                completed: flight.completed.clone(),
                ids: flight
                    .ids
                    .iter()
                    .filter(|id| !returned.contains(id))
                    .cloned()
                    .collect(),
            },
        })
    }
    /// Completing a nonempty settlement requires the exact queued paired commit,
    /// not merely reservation. A dropped/cancelled work cannot clear pending IDs.
    pub fn published(&mut self, scope: &Scope, settlement: Settlement) -> Result<()> {
        let phase = settlement.flight.phase.load(Ordering::Acquire);
        let done = settlement
            .flight
            .completed
            .lock()
            .map_err(|_| Error::Stale)?
            .as_ref()
            .is_some_and(|done| done.load(Ordering::Acquire));
        if !self.flight_current(scope, &settlement.flight)
            || !((phase == 2 && done) || (phase == 1 && settlement.missing.ids.is_empty()))
        {
            return Err(Error::Stale);
        }
        self.pending
            .retain(|id| !settlement.flight.ids.contains(id));
        self.retire_lookups(&settlement.flight.ids);
        settlement.flight.phase.store(3, Ordering::Release);
        settlement.flight.identity.store(0, Ordering::Release);
        self.flight = None;
        Ok(())
    }
    pub fn cancel(&mut self, scope: &Scope, flight: &Flight) -> bool {
        if !self.flight_current(scope, flight) {
            return false;
        }
        flight.identity.store(0, Ordering::Release);
        self.retire_lookups(&flight.ids);
        self.flight = None;
        true
    }
    fn retire_lookups(&mut self, ids: &[String]) {
        for id in ids {
            if let Some((token, _)) = self.lookups.remove(id) {
                token.store(0, Ordering::Release);
            }
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.alive.store(0, Ordering::Release);
        if let Some(flight) = &self.flight {
            flight.identity.store(0, Ordering::Release);
        }
    }
}
fn valid_id(id: &str) -> bool {
    (9..=255).contains(&id.len())
        && id.starts_with('S')
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
