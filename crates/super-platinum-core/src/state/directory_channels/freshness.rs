//! xsBi/Tst9 channel conditional freshness. No channel rows, IO or cache writes.
//! Native owns Transport, canonical publication, deadlines and notification IO.
use super::{Error, Identity, Result, WorkspaceSession, session_equal};
use std::{collections::HashSet, sync::Arc};
mod burst;
mod clock;
mod memberships;
mod ordered;
pub mod refresh;
pub use clock::{Clock, Fresh, Pause, RowKind, TimerId};
#[cfg(test)]
mod tests;

pub const CADENCE_MS: u64 = 200;
pub const TIMEOUT_MS: u64 = 60_000;
pub const STALE_NOTIFICATION_THROTTLE_MS: u64 = 2_000;

/// Auth ownership deliberately excludes socket generation. Source slow-count
/// reset/requeue is separate; native still checks its actual Transport/RT lease.
#[derive(Clone)]
pub struct Scope {
    identity: Identity,
    session: WorkspaceSession,
    account_epoch: u64,
}
impl Scope {
    pub fn new(account: &str, session: &WorkspaceSession, account_epoch: u64) -> Result<Self> {
        Ok(Self {
            identity: Identity::capture(account, session)?,
            session: session.clone(),
            account_epoch,
        })
    }
    fn matches(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.account_epoch == other.account_epoch
            && session_equal(&self.session, &other.session)
    }
}

/// Actual XE4p.d4 and WWP4.Qn observations, never inferred from an RT generation.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub slow_connection_count: Option<u64>,
    pub major_version: Option<u64>,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Batch {
    token: Arc<()>,
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

/// Notification *invocations*: the native owner applies the source's shared
/// leading/trailing two-second thunk throttle only to stale notifications.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub notify_immediately: bool,
    pub notify_throttled: bool,
    pub invalidated: Vec<String>,
    pub bursts: Vec<(String, Burst)>,
}
pub use burst::{Burst, InvalidationBursts};

pub struct Manager {
    scope: Scope,
    freshness: ordered::Freshness, // None means Infinity, not absence.
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
            freshness: ordered::Freshness::default(),
            pending: Vec::new(),
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
        if self.current(scope) {
            Ok(())
        } else {
            Err(Error::MissingObservation)
        }
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
            self.freshness.clear();
        }
        self.slow_count = count;
        self.version = observation.major_version;
    }
    fn fresh(&self, id: &str, now: Clock) -> bool {
        self.freshness
            .get(id)
            .is_some_and(|deadline| deadline.is_none_or(|t| t > now.seconds()))
    }
    pub fn is_fresh(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        id: &str,
    ) -> Result<bool> {
        self.owned(scope)?;
        if !observation.enabled {
            return Ok(true);
        }
        self.reset(observation);
        Ok(self.fresh(id, now))
    }
    /// Source getFreshObjects does not reset or check enabled status.
    pub fn fresh_ids(&self, scope: &Scope, now: Clock) -> Result<Vec<String>> {
        self.owned(scope)?;
        Ok(self.freshness.fresh_ids(now.seconds()))
    }
    pub fn pending(&self) -> &[String] {
        &self.pending
    }
    pub fn next_wake(&self) -> Option<u64> {
        match &self.active {
            Some(Active::Waiting(t)) => Some(*t),
            Some(Active::Fetching(b)) => Some(b.deadline_ms),
            None => None,
        }
    }
    fn enqueue(&mut self, id: &str) {
        if self.pending_index.insert(id.into()) {
            self.pending.push(id.into());
        }
    }
    fn remove_pending(&mut self, completed: &HashSet<&str>) {
        self.pending.retain(|id| !completed.contains(id.as_str()));
        self.pending_index
            .retain(|id| !completed.contains(id.as_str()));
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
    /// Resume locally deferred admission without adding IDs or certifying rows.
    pub fn resume(&mut self, scope: &Scope, now: Clock, observation: Observation) -> Result<()> {
        self.owned(scope)?;
        self.schedule(now, observation.enabled);
        Ok(())
    }
    /// Called on an existing canonical selector hit even when allow_fetch=false.
    /// The caller returns that original row immediately, independently of this.
    pub fn maybe_refetch(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        id: &str,
        paused: bool,
    ) -> Result<()> {
        self.owned(scope)?;
        if paused {
            return Ok(());
        }
        clock::valid_id(id)?;
        if self.is_fresh(scope, now, observation, id)? || !observation.enabled {
            return Ok(());
        }
        self.enqueue(id);
        self.schedule(now, observation.enabled);
        Ok(())
    }
    /// Source delay callback takes the current pending Set, not an earlier list.
    /// Keyboard pause only gates maybe_refetch, never a previously reserved turn.
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
            token: Arc::new(()),
            ids: std::mem::take(&mut self.pending),
            connection: observation.slow_connection_count,
            deadline_ms: now.millis() + TIMEOUT_MS,
        };
        debug_assert!(!batch.ids.is_empty());
        self.active = Some(Active::Fetching(batch.clone()));
        Ok(Some(batch))
    }
    pub fn batch_current(&self, scope: &Scope, batch: &Batch) -> bool {
        self.current(scope)
            && matches!(&self.active, Some(Active::Fetching(current)) if Arc::ptr_eq(&current.token, &batch.token))
    }
    pub fn mark_fresh(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        values: &[Fresh],
    ) -> Result<()> {
        self.owned(scope)?;
        if !observation.enabled {
            return Ok(());
        }
        let entries = values
            .iter()
            .map(|v| v.deadline(now))
            .collect::<Result<Vec<_>>>()?;
        self.reset(observation);
        for (id, deadline) in entries {
            self.freshness.insert(id, deadline);
        }
        Ok(())
    }
    /// Terminal attempted read: source catches failures/timeouts, then runs this
    /// same finally path. Values describe current canonical rows at completion,
    /// not the response's success/failure. No canonical mutation is performed.
    pub fn finish(
        &mut self,
        scope: &Scope,
        batch: &Batch,
        now: Clock,
        observation: Observation,
        values: &[Fresh],
    ) -> Result<Effects> {
        if !self.batch_current(scope, batch) {
            return Err(Error::MissingObservation);
        }
        let reconnect = observation
            .slow_connection_count
            .is_some_and(|n| n > 1 && Some(n) != batch.connection);
        if !reconnect {
            if values.iter().map(|v| &v.id).ne(batch.ids.iter()) {
                return Err(Error::MissingObservation);
            }
            self.mark_fresh(scope, now, observation, values)?;
        }
        self.active = None;
        if reconnect {
            for id in &batch.ids {
                self.enqueue(id);
            }
            self.reset(observation);
        } else {
            let completed: HashSet<&str> = batch.ids.iter().map(String::as_str).collect();
            self.remove_pending(&completed);
        }
        self.schedule(now, observation.enabled);
        Ok(Effects {
            notify_immediately: true,
            ..Default::default()
        })
    }
    /// Native admission refused before any read was attempted. This is not the
    /// source failure-finally path: preserve IDs and never certify freshness.
    /// A later maybe_refetch resumes admission; no automatic cancellation loop.
    pub fn return_unsent(&mut self, scope: &Scope, batch: &Batch) -> Result<()> {
        if !self.batch_current(scope, batch) {
            return Err(Error::MissingObservation);
        }
        self.active = None;
        for id in &batch.ids {
            self.enqueue(id);
        }
        Ok(())
    }
    /// Native-only split when local admission stops between ordered HTTP chunks.
    /// Attempted IDs must be an exact unique ordered subsequence of this batch.
    /// Unsent IDs remain pending, with no freshness grant or automatic retry.
    /// This is not a different source failure policy for an attempted request.
    pub fn finish_admitted(
        &mut self,
        scope: &Scope,
        batch: &Batch,
        now: Clock,
        observation: Observation,
        attempted: &[Fresh],
    ) -> Result<Effects> {
        if !self.batch_current(scope, batch) {
            return Err(Error::MissingObservation);
        }
        let mut cursor = 0;
        for row in attempted {
            let Some(index) = batch.ids[cursor..].iter().position(|id| id == &row.id) else {
                return Err(Error::MissingObservation);
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
            let completed: HashSet<&str> = attempted.iter().map(|row| row.id.as_str()).collect();
            self.remove_pending(&completed);
            for id in &batch.ids {
                if !completed.contains(id.as_str()) {
                    self.enqueue(id);
                }
            }
        }
        Ok(Effects {
            notify_immediately: true,
            ..Default::default()
        })
    }
    pub fn mark_stale(
        &mut self,
        scope: &Scope,
        now: Clock,
        observation: Observation,
        ids: &[String],
        bursts: &mut InvalidationBursts,
    ) -> Result<Effects> {
        self.owned(scope)?;
        if !observation.enabled {
            return Ok(Effects::default());
        }
        for id in ids {
            clock::valid_id(id)?;
        }
        self.reset(observation);
        let mut effects = Effects::default();
        for id in ids {
            // Source checks stored TTL truthiness, not current freshness.
            if !self.freshness.get(id).is_some_and(|t| *t != Some(0)) {
                continue;
            }
            let verdict = bursts.check(
                &format!("{}:{id}", self.scope.session.team_id),
                now.millis(),
            );
            if verdict == Burst::Allowed {
                self.freshness.remove(id);
                effects.invalidated.push(id.clone());
            } else {
                effects.bursts.push((id.clone(), verdict));
            }
        }
        effects.notify_throttled = !effects.invalidated.is_empty();
        Ok(effects)
    }
}
