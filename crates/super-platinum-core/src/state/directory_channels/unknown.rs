//! Shared 0mwJ scheduling custody. A terminal marker is local manager state,
//! never proof that Slack has no resource. No network or timers run here.
use super::*;
use std::{
    sync::atomic::{AtomicU8, Ordering},
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
        Identity::capture(account, owner)?;
        Ok(Self {
            account: account.into(),
            owner: owner.clone(),
            epoch,
            realtime_generation,
        })
    }
    fn matches(&self, other: &Self) -> bool {
        self.account == other.account
            && session_equal(&self.owner, &other.owner)
            && self.epoch == other.epoch
            && self.realtime_generation == other.realtime_generation
    }
}

#[derive(Clone, Debug)]
pub struct Flight {
    identity: Arc<AtomicU8>,
    // 0 running, 1 settlement staged, 2 markers enqueued, 3 published.
    terminal: Arc<AtomicU8>,
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

pub struct Missing {
    scope: Scope,
    identity: Arc<AtomicU8>,
    terminal: Arc<AtomicU8>,
    phase: lifecycle::CapturedPhase,
    attempt: u8,
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
    phase: Option<lifecycle::CapturedPhase>,
    pending: Vec<String>,
    lookups: BTreeMap<String, Arc<AtomicU8>>,
    flight: Option<Flight>,
    last_start: Option<Duration>,
}
impl Manager {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            alive: Arc::new(AtomicU8::new(1)),
            phase: None,
            pending: vec![],
            lookups: BTreeMap::new(),
            flight: None,
            last_start: None,
        }
    }
    pub fn current(&self, scope: &Scope) -> bool {
        self.scope.matches(scope) && self.alive.load(Ordering::Acquire) == 1
    }
    pub fn pending(&self) -> &[String] {
        &self.pending
    }
    pub fn flight_current(&self, scope: &Scope, flight: &Flight) -> bool {
        self.current(scope)
            && self.flight.as_ref().is_some_and(|f| {
                Arc::ptr_eq(&f.identity, &flight.identity)
                    && f.attempt == flight.attempt
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
            return Err(Error::MissingObservation);
        }
        if self.flight.is_some() || self.pending.is_empty() || !self.remaining(now).is_zero() {
            return Ok(None);
        }
        if self.phase.is_none() {
            return Err(Error::MissingObservation);
        }
        let flight = Flight {
            identity: Arc::new(AtomicU8::new(1)),
            terminal: Arc::new(AtomicU8::new(0)),
            ids: self.pending.clone(),
            attempt: 1,
        };
        self.last_start = Some(now);
        self.flight = Some(flight.clone());
        Ok(Some(flight))
    }
    /// Only an outer timeout retries. Ordinary wrapper failures settle empty.
    pub fn retry_timeout(&mut self, scope: &Scope, flight: &Flight) -> Result<Option<Flight>> {
        if !self.flight_current(scope, flight) || flight.terminal.load(Ordering::Acquire) != 0 {
            return Err(Error::MissingObservation);
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
    /// Returned includes real results and source-known rows retained at dispatch.
    /// Native caller must retain original response/publication authority.
    pub fn settlement(
        &self,
        scope: &Scope,
        flight: &Flight,
        returned: &[String],
    ) -> Result<Settlement> {
        if !self.flight_current(scope, flight) || returned.iter().any(|id| !flight.ids.contains(id))
        {
            return Err(Error::MissingObservation);
        }
        let phase = self.phase.clone().ok_or(Error::MissingObservation)?;
        if flight
            .terminal
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::MissingObservation);
        }
        Ok(Settlement {
            flight: flight.clone(),
            missing: Missing {
                scope: self.scope.clone(),
                identity: flight.identity.clone(),
                terminal: flight.terminal.clone(),
                phase,
                attempt: flight.attempt,
                ids: flight
                    .ids
                    .iter()
                    .filter(|id| !returned.contains(id))
                    .cloned()
                    .collect(),
            },
        })
    }
    /// Only after the paired canonical queue publication succeeds. Pending IDs
    /// added during this flight survive; no receipt may clear another flight.
    pub fn published(&mut self, scope: &Scope, settlement: Settlement) -> Result<()> {
        let phase = settlement.flight.terminal.load(Ordering::Acquire);
        if !self.flight_current(scope, &settlement.flight)
            || !(phase == 2 || (phase == 1 && settlement.missing.ids.is_empty()))
        {
            return Err(Error::MissingObservation);
        }
        self.pending
            .retain(|id| !settlement.flight.ids.contains(id));
        self.retire_lookups(&settlement.flight.ids);
        settlement.flight.terminal.store(3, Ordering::Release);
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
            if let Some(token) = self.lookups.remove(id) {
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
