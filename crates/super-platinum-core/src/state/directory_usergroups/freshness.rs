//! yk0Q/Tst9 known-group freshness. This is independent of Store boot lineage.
//! Selectors return their original row immediately; this module owns no rows,
//! network, timers, or canonical publication. Group stamps have infinite age.
use super::{Binding, Error, Result, WorkspaceSession};
pub use crate::state::directory_channels::freshness::{Clock, Pause, TimerId};
use std::{collections::HashSet, sync::Arc};

#[cfg(test)]
mod tests;

pub const CADENCE_MS: u64 = 200;
pub const TIMEOUT_MS: u64 = 60_000;

/// Auth identity excludes socket generation: actual slow-count reset/requeue
/// and the host's transport/current-consumer fence are separate observations.
#[derive(Clone)]
pub struct Scope(Binding);
impl Scope {
    pub fn new(account: &str, session: &WorkspaceSession, epoch: u64) -> Result<Self> {
        if account.is_empty()
            || session.team_id.is_empty()
            || session.user_id.is_empty()
            || session.url.is_empty()
            || session.token.is_empty()
        {
            return Err(Error::Owner);
        }
        Ok(Self(Binding {
            account: account.into(),
            owner: session.clone(),
            epoch,
        }))
    }
    fn matches(&self, other: &Self) -> bool {
        self.0
            .current(&other.0.account, &other.0.owner, other.0.epoch)
    }
}

/// Original XE4p.d4 / WWP4.Q values, not an inferred socket generation.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub slow_connection_count: Option<u64>,
    pub major_version: Option<u64>,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Batch {
    identity: Arc<()>,
    ids: Vec<String>,
    connection: Option<u64>,
    deadline_ms: u64,
}
impl Batch {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn deadline_ms(&self) -> u64 {
        self.deadline_ms
    }
}

enum Active {
    Waiting(u64),
    Fetching(Batch),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub notify_immediately: bool,
}

pub struct Manager {
    scope: Scope,
    // Insertion order matches the source freshness object's non-index ID keys.
    fresh: Vec<String>,
    fresh_index: HashSet<String>,
    pending: Vec<String>,
    pending_index: HashSet<String>,
    active: Option<Active>,
    last_reservation_ms: u64,
    slow_count: u64,
    version: Option<u64>,
}
impl Manager {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            fresh: vec![],
            fresh_index: HashSet::new(),
            pending: vec![],
            pending_index: HashSet::new(),
            active: None,
            last_reservation_ms: 0,
            slow_count: 1,
            version: None,
        }
    }
    pub fn current(&self, scope: &Scope) -> bool {
        self.scope.matches(scope)
    }
    fn owned(&self, scope: &Scope) -> Result<()> {
        self.current(scope).then_some(()).ok_or(Error::Owner)
    }
    fn reset(&mut self, observation: Observation) {
        if !observation.enabled {
            return;
        }
        let count = observation.slow_connection_count.unwrap_or(1).max(1);
        if self.version.is_none_or(|v| v == 0) {
            self.version = observation.major_version;
        }
        if self.slow_count != count || self.version != observation.major_version {
            self.fresh.clear();
            self.fresh_index.clear();
        }
        self.slow_count = count;
        self.version = observation.major_version;
    }
    pub fn is_fresh(
        &mut self,
        scope: &Scope,
        _now: Clock,
        observation: Observation,
        id: &str,
    ) -> Result<bool> {
        self.owned(scope)?;
        valid_id(id)?;
        if !observation.enabled {
            return Ok(true);
        }
        self.reset(observation);
        Ok(self.fresh_index.contains(id))
    }
    /// Source getFreshObjects neither resets nor checks enabled.
    pub fn fresh_ids(&self, scope: &Scope, _now: Clock) -> Result<Vec<String>> {
        self.owned(scope)?;
        Ok(self.fresh.clone())
    }
    pub fn pending(&self) -> &[String] {
        &self.pending
    }
    pub fn next_wake(&self) -> Option<u64> {
        match &self.active {
            Some(Active::Waiting(due)) => Some(*due),
            Some(Active::Fetching(batch)) => Some(batch.deadline_ms),
            None => None,
        }
    }
    fn enqueue(&mut self, id: &str) {
        if self.pending_index.insert(id.into()) {
            self.pending.push(id.into());
        }
    }
    fn schedule(&mut self, now: Clock, enabled: bool) {
        if enabled && self.active.is_none() && !self.pending.is_empty() {
            let due = self
                .last_reservation_ms
                .saturating_add(CADENCE_MS)
                .max(now.millis());
            self.last_reservation_ms = now.millis();
            self.active = Some(Active::Waiting(due));
        }
    }
    pub fn maybe_refetch(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        id: &str,
        paused: bool,
    ) -> Result<()> {
        self.owned(scope)?;
        if paused || self.is_fresh(scope, now, observation, id)? {
            return Ok(());
        }
        self.enqueue(id);
        self.schedule(now, observation.enabled);
        Ok(())
    }
    /// A previously reserved delay is not cancelled by keyboard pause.
    /// The source takes the then-current pending Set and raw slow count.
    pub fn begin_due(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
    ) -> Result<Option<Batch>> {
        self.owned(scope)?;
        if !matches!(self.active, Some(Active::Waiting(due)) if due <= now.millis()) {
            return Ok(None);
        }
        self.pending_index.clear();
        let batch = Batch {
            identity: Arc::new(()),
            ids: std::mem::take(&mut self.pending),
            connection: observation.slow_connection_count,
            deadline_ms: now.millis() + TIMEOUT_MS,
        };
        self.active = Some(Active::Fetching(batch.clone()));
        Ok(Some(batch))
    }
    pub fn batch_current(&self, scope: &Scope, batch: &Batch) -> bool {
        self.current(scope)
            && matches!(&self.active, Some(Active::Fetching(current))
                if Arc::ptr_eq(&current.identity, &batch.identity))
    }
    pub fn mark_fresh(
        &mut self,
        scope: &Scope,
        _now: Clock,
        observation: Observation,
        ids: &[String],
    ) -> Result<()> {
        self.owned(scope)?;
        if !observation.enabled {
            return Ok(());
        }
        for id in ids {
            valid_id(id)?;
        }
        self.reset(observation);
        for id in ids {
            if self.fresh_index.insert(id.clone()) {
                self.fresh.push(id.clone());
            }
        }
        Ok(())
    }
    /// Same finally path for success, error and timeout. No metadata is inferred.
    /// Major-only changes reset then stamp; only the raw slow-count predicate
    /// below requeues. The host independently fences actual row publication.
    pub fn finish(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        batch: &Batch,
    ) -> Result<Effects> {
        self.finish_admitted(scope, now, observation, batch, &batch.ids)?;
        self.schedule(now, observation.enabled);
        Ok(Effects {
            notify_immediately: true,
        })
    }
    pub fn timeout(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        batch: &Batch,
    ) -> Result<Effects> {
        if now.millis() < batch.deadline_ms {
            return Err(Error::Stale);
        }
        self.finish(scope, now, observation, batch)
    }
    /// Native preflight refusal is not a completed source request. Preserve
    /// pending IDs without freshness credit or an automatic admission loop.
    pub fn return_unsent(&mut self, scope: &Scope, batch: &Batch) -> Result<()> {
        if !self.batch_current(scope, batch) {
            return Err(Error::Stale);
        }
        self.active = None;
        for id in &batch.ids {
            self.enqueue(id);
        }
        Ok(())
    }
    /// Native adaptation for chunked reads stopped before all IDs were sent.
    /// Attempted IDs must be an exact ordered subsequence; unsent IDs remain
    /// pending, with no automatic retry. Ordinary full completion uses finish.
    pub fn finish_admitted(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        batch: &Batch,
        attempted: &[String],
    ) -> Result<Effects> {
        if !self.batch_current(scope, batch) {
            return Err(Error::Stale);
        }
        let mut cursor = 0;
        for id in attempted {
            let Some(index) = batch.ids[cursor..].iter().position(|value| value == id) else {
                return Err(Error::Shape);
            };
            cursor += index + 1;
        }
        if attempted.is_empty() {
            self.return_unsent(scope, batch)?;
            return Ok(Effects::default());
        }
        let reconnect = observation
            .slow_connection_count
            .is_some_and(|n| n > 1 && Some(n) != batch.connection);
        if !reconnect {
            self.mark_fresh(scope, now, observation, attempted)?;
        }
        self.active = None;
        if reconnect {
            for id in &batch.ids {
                self.enqueue(id);
            }
            self.reset(observation);
        } else {
            let completed: HashSet<&str> = attempted.iter().map(String::as_str).collect();
            self.pending.retain(|id| !completed.contains(id.as_str()));
            self.pending_index
                .retain(|id| !completed.contains(id.as_str()));
            for id in &batch.ids {
                if !completed.contains(id.as_str()) {
                    self.enqueue(id);
                }
            }
        }
        Ok(Effects {
            notify_immediately: true,
        })
    }
    /// Explicitly resume a locally deferred, unsent queue.
    pub fn resume(&mut self, scope: &Scope, now: Clock, observation: Observation) -> Result<()> {
        self.owned(scope)?;
        self.schedule(now, observation.enabled);
        Ok(())
    }
}

fn valid_id(id: &str) -> Result<()> {
    // Same bounded original group-ID shape as directory_endpoint::usergroup_info.
    if (9..=255).contains(&id.len())
        && id.starts_with('S')
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        Ok(())
    } else {
        Err(Error::Shape)
    }
}
