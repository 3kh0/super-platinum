//! Default (non-treatment) TruP SubscriptionFetchManager and Tst9 scheduling.
//! Pure source accounting: subscribed does NOT mean a confirmed server ACK.
//! Native owns source-mode admission, shared pause, IO, original socket/source
//! fences, operation timers and source/native row publication.
use super::{Error, WorkspaceSession};
use crate::block_actions::directory_endpoint::member_info::Read;
use std::{collections::VecDeque, sync::Arc};

mod cycle;
#[cfg(test)]
mod tests;
use cycle::Running;

pub const PARENT_CADENCE_MS: u64 = 200;
pub const OPERATION_CADENCE_MS: u64 = 1000;
pub const TIMEOUT_MS: u64 = 60_000;
pub const MAX_BATCH: usize = 100;
const MAX_TIME: u64 = 9_007_199_254_740_991 - TIMEOUT_MS;
fn unavailable() -> Error {
    Error::Api("member_subscription_observation_unavailable".into())
}
fn valid_time(now: u64) -> Result<(), Error> {
    (now <= MAX_TIME).then_some(()).ok_or_else(unavailable)
}
fn valid_ids(ids: &[String]) -> Result<(), Error> {
    for id in ids {
        Read::capture(vec![id.clone()])?;
    }
    Ok(())
}
fn add(set: &mut Vec<String>, id: &str) {
    if !set.iter().any(|v| v == id) {
        set.push(id.into());
    }
}
fn chunks(ids: &[String]) -> VecDeque<Vec<String>> {
    ids.chunks(MAX_BATCH).map(<[_]>::to_vec).collect()
}

/// Logical account/session lifetime; socket generations are not source counts.
#[derive(Clone)]
pub struct Scope {
    account: String,
    owner: WorkspaceSession,
    epoch: u64,
}
impl Scope {
    pub fn capture(account: &str, owner: &WorkspaceSession, epoch: u64) -> Result<Self, Error> {
        if account.is_empty() || owner.team_id.is_empty() || owner.user_id.is_empty() {
            return Err(unavailable());
        }
        Ok(Self {
            account: account.into(),
            owner: owner.clone(),
            epoch,
        })
    }
    fn matches(&self, other: &Self) -> bool {
        self.account == other.account
            && self.epoch == other.epoch
            && self.owner.team_id == other.owner.team_id
            && self.owner.user_id == other.owner.user_id
            && self.owner.enterprise_id == other.owner.enterprise_id
            && self.owner.name == other.owner.name
            && self.owner.url == other.owner.url
            && self.owner.token == other.owner.token
    }
}
/// Actual accepted lifecycle facts. Caller must not derive these from socket ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub connection_count: u64,
    pub slow_connection_count: u64,
    pub major_version: u64,
}
impl Observation {
    fn normalized(self) -> Self {
        Self {
            connection_count: self.connection_count.max(1),
            slow_connection_count: self.slow_connection_count.max(1),
            ..self
        }
    }
}
#[derive(Clone)]
pub struct Cycle {
    token: Arc<()>,
    ids: Vec<String>,
    parent_deadline: u64,
    reconnect: bool,
}
impl Cycle {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn parent_deadline_ms(&self) -> u64 {
        self.parent_deadline
    }
    pub fn is_reconnect(&self) -> bool {
        self.reconnect
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Subscribe,
    Unsubscribe,
}
#[derive(Clone)]
pub struct Operation {
    token: Arc<()>,
    cycle: Arc<()>,
    kind: Kind,
    ids: Vec<String>,
    due: u64,
    deadline: Option<u64>,
}
impl Operation {
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn due_ms(&self) -> u64 {
        self.due
    }
    /// Source per-requested-chunk deadline, absent for initial unsubscribe and
    /// independent fast-reconnect chains. This is not a socket ACK timeout.
    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentOutcome {
    Completed,
    Timeout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Stats {
    pub pending: usize,
    pub subscribed: usize,
    pub stale: usize,
    pub cycles: usize,
    pub issued: usize,
}

pub struct Manager {
    scope: Scope,
    observed: Option<Observation>,
    pending: Vec<String>,
    subscribed: Vec<String>,
    stale: Vec<String>,
    parent: Option<Arc<()>>,
    due: Option<u64>,
    last_reservation: u64,
    last_call: u64,
    cycles: Vec<Running>,
}
impl Manager {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            observed: None,
            pending: vec![],
            subscribed: vec![],
            stale: vec![],
            parent: None,
            due: None,
            last_reservation: 0,
            last_call: 0,
            cycles: vec![],
        }
    }
    pub fn current(&self, scope: &Scope) -> bool {
        self.scope.matches(scope)
    }
    fn owned(&self, scope: &Scope) -> Result<(), Error> {
        self.current(scope).then_some(()).ok_or_else(unavailable)
    }
    pub fn pending(&self) -> &[String] {
        &self.pending
    }
    pub fn subscribed(&self) -> &[String] {
        &self.subscribed
    }
    pub fn stale(&self) -> &[String] {
        &self.stale
    }
    pub fn next_parent_due_ms(&self) -> Option<u64> {
        self.due
    }
    pub fn stats(&self) -> Stats {
        Stats {
            pending: self.pending.len(),
            subscribed: self.subscribed.len(),
            stale: self.stale.len(),
            cycles: self.cycles.len(),
            issued: self
                .cycles
                .iter()
                .filter(|v| v.operation.as_ref().is_some_and(|(_, issued)| *issued))
                .count(),
        }
    }
    pub fn has_work_for(&self, ids: &[String]) -> bool {
        ids.iter().any(|id| {
            self.pending.contains(id) || self.cycles.iter().any(|v| v.ticket.ids.contains(id))
        })
    }
    /// Includes an independent fast-reconnect resubscribe chain and issued
    /// chunks retained after the parent deadline. Each token owns only itself.
    pub fn cycles(&self) -> Vec<Cycle> {
        self.cycles.iter().map(|v| v.ticket.clone()).collect()
    }
    fn schedule(&mut self, now: u64) {
        if self.parent.is_none() && self.due.is_none() && !self.pending.is_empty() {
            self.due = Some(
                self.last_reservation
                    .saturating_add(PARENT_CADENCE_MS)
                    .max(now),
            );
            self.last_reservation = now;
        }
    }
    /// Native retirement of old operations is distinct from source failure.
    /// Preserve their IDs, then apply TruP slow > major > fast reset precedence.
    pub fn observe(
        &mut self,
        scope: &Scope,
        now: u64,
        observation: Observation,
    ) -> Result<(), Error> {
        self.owned(scope)?;
        valid_time(now)?;
        let next = observation.normalized();
        if self.observed == Some(next) {
            return Ok(());
        }
        let old = self.observed.replace(next);
        if let Some(old) = old {
            for cycle in self.cycles.drain(..) {
                for id in &cycle.ticket.ids {
                    add(&mut self.pending, id);
                }
                cycle.retire_into(&mut self.pending, &mut self.stale, &self.subscribed);
            }
            self.parent = None;
            if old.slow_connection_count != next.slow_connection_count {
                self.stale.clear();
                self.subscribed.clear();
            } else if old.major_version != next.major_version {
                for id in self.subscribed.drain(..) {
                    add(&mut self.stale, &id);
                }
            } else if old.connection_count != next.connection_count && !self.subscribed.is_empty() {
                let ticket = Cycle {
                    token: Arc::new(()),
                    ids: self.subscribed.clone(),
                    parent_deadline: now + TIMEOUT_MS,
                    reconnect: true,
                };
                self.cycles.push(Running::new(ticket, vec![]));
            }
        }
        self.schedule(now);
        Ok(())
    }
    pub fn hit(
        &mut self,
        scope: &Scope,
        now: u64,
        observation: Observation,
        id: &str,
        paused: bool,
    ) -> Result<(), Error> {
        self.owned(scope)?;
        valid_time(now)?;
        valid_ids(&[id.into()])?;
        if paused {
            return Ok(());
        }
        self.observe(scope, now, observation)?;
        if !self.subscribed.iter().any(|v| v == id) {
            add(&mut self.pending, id);
            self.schedule(now);
        }
        Ok(())
    }
    /// Source ingress markFresh means ensure/subscription, NOT a TTL/ACK grant.
    /// It deliberately ignores the keyboard pause used by selector hits.
    pub fn mark_fresh(
        &mut self,
        scope: &Scope,
        now: u64,
        observation: Observation,
        ids: &[String],
    ) -> Result<(), Error> {
        self.owned(scope)?;
        valid_time(now)?;
        valid_ids(ids)?;
        if ids.is_empty() {
            return Ok(());
        }
        self.observe(scope, now, observation)?;
        for id in ids {
            if !self.subscribed.contains(id) {
                add(&mut self.pending, id);
            }
        }
        self.schedule(now);
        Ok(())
    }
    /// Normal SFM override: moves subscribed to stale, no parent TTL/burst
    /// notification or eager read. Next real selector hit schedules recovery.
    pub fn invalidate(&mut self, scope: &Scope, ids: &[String]) -> Result<(), Error> {
        self.owned(scope)?;
        valid_ids(ids)?;
        for id in ids {
            if let Some(index) = self.subscribed.iter().position(|v| v == id) {
                self.subscribed.remove(index);
                add(&mut self.stale, id);
            }
        }
        Ok(())
    }
    pub fn begin_due(
        &mut self,
        scope: &Scope,
        now: u64,
        observation: Observation,
    ) -> Result<Option<Cycle>, Error> {
        self.observe(scope, now, observation)?;
        if self.parent.is_some() || self.due.is_none_or(|v| v > now) {
            return Ok(None);
        }
        self.due = None;
        if self.pending.is_empty() {
            return Ok(None);
        }
        let ticket = Cycle {
            token: Arc::new(()),
            ids: std::mem::take(&mut self.pending),
            parent_deadline: now + TIMEOUT_MS,
            reconnect: false,
        };
        self.parent = Some(ticket.token.clone());
        let initial = std::mem::take(&mut self.stale);
        self.cycles.push(Running::new(ticket.clone(), initial));
        Ok(Some(ticket))
    }
    fn index(&self, scope: &Scope, cycle: &Cycle) -> Result<usize, Error> {
        self.owned(scope)?;
        self.cycles
            .iter()
            .position(|v| Arc::ptr_eq(&v.ticket.token, &cycle.token))
            .ok_or_else(unavailable)
    }
    pub fn cycle_current(&self, scope: &Scope, cycle: &Cycle) -> bool {
        self.index(scope, cycle).is_ok()
    }
    /// Whether this exact cycle still owns the outer parent deadline/finally.
    /// An issued chunk may remain current after its parent has already ended.
    pub fn parent_active(&self, scope: &Scope, cycle: &Cycle) -> bool {
        self.cycle_current(scope, cycle)
            && self
                .parent
                .as_ref()
                .is_some_and(|token| Arc::ptr_eq(token, &cycle.token))
    }
    pub fn operation_current(&self, scope: &Scope, op: &Operation) -> bool {
        self.current(scope)
            && self.cycles.iter().any(|v| {
                Arc::ptr_eq(&v.ticket.token, &op.cycle)
                    && v.operation
                        .as_ref()
                        .is_some_and(|(current, _)| Arc::ptr_eq(&current.token, &op.token))
            })
    }
    pub fn cycle_done(&self, scope: &Scope, cycle: &Cycle) -> Result<bool, Error> {
        Ok(self.cycles[self.index(scope, cycle)?].done)
    }
    pub fn chunk_deadline_ms(&self, scope: &Scope, cycle: &Cycle) -> Result<Option<u64>, Error> {
        Ok(self.cycles[self.index(scope, cycle)?]
            .group
            .as_ref()
            .map(|v| v.deadline))
    }
}
