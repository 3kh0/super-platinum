//! Small publication coordinator. No timers, tasks, renderer or mutation hooks.
//! The renderer supplies its complete account/epoch/team/destination/mount owner,
//! runs Build::finish on a worker, and validates its query sequence at delivery.
use super::{CheckedIndex, Snapshot, Suggestion, Trigger};
use crate::state::Workspace;
use std::sync::Arc;

#[derive(Clone)]
pub struct Ticket {
    generation: Arc<()>,
    serial: u64,
}
pub struct Build<O> {
    ticket: Ticket,
    owner: O,
    snapshot: Snapshot,
}
pub struct Completed<O> {
    ticket: Ticket,
    owner: O,
    index: CheckedIndex,
}
pub struct Cache<O> {
    generation: Arc<()>,
    owner: O,
    serial: u64,
    pending: Option<u64>,
    ready: Option<CheckedIndex>,
}
impl<O: Clone + Eq> Cache<O> {
    pub fn new(owner: O) -> Self {
        Self {
            generation: Arc::new(()),
            owner,
            serial: 0,
            pending: None,
            ready: None,
        }
    }
    /// Retire every outstanding job, including an A→B→A owner sequence.
    pub fn reset(&mut self, owner: O) {
        *self = Self::new(owner);
    }
    pub fn suggestions(
        &self,
        current_owner: &O,
        ws: &Workspace,
        trigger: &Trigger,
    ) -> Option<Vec<Suggestion>> {
        if &self.owner != current_owner {
            return None;
        }
        self.ready.as_ref()?.suggestions(ws, trigger)
    }
    /// Preparation copies only thin metadata. Measure this UI-side copy cost;
    /// the expensive folding/build belongs to the caller's background worker.
    pub fn prepare(&mut self, current_owner: &O, ws: &Workspace) -> Option<Build<O>> {
        if &self.owner != current_owner || self.pending.is_some() {
            return None;
        }
        let serial = self.serial.checked_add(1)?;
        self.serial = serial;
        self.pending = Some(serial);
        Some(Build {
            ticket: Ticket {
                generation: self.generation.clone(),
                serial,
            },
            owner: self.owner.clone(),
            snapshot: Snapshot::capture(ws),
        })
    }
    fn current_ticket(&self, ticket: &Ticket) -> bool {
        Arc::ptr_eq(&self.generation, &ticket.generation) && self.pending == Some(ticket.serial)
    }
    /// A worker failure may retire only its own job, never a newer pending one.
    pub fn failed(&mut self, ticket: &Ticket) -> bool {
        if !self.current_ticket(ticket) {
            return false;
        }
        self.pending = None;
        true
    }
    /// Recheck exact canonical fields after the await, not only the owner. A
    /// rejected current job is retired so the latest query can drive a new one.
    pub fn publish(&mut self, current_owner: &O, ws: &Workspace, result: Completed<O>) -> bool {
        if !self.current_ticket(&result.ticket) {
            return false;
        }
        self.pending = None;
        if &self.owner != current_owner || result.owner != self.owner || !result.index.matches(ws) {
            return false;
        }
        self.ready = Some(result.index);
        true
    }
    /// This does not replace the renderer's final query/mount handshake. It makes
    /// current canonical labels and the exact choice membership prerequisites.
    pub fn insert_if_current(
        &self,
        current_owner: &O,
        ws: &Workspace,
        composer: &mut crate::ComposerState,
        trigger: &Trigger,
        choice: &Suggestion,
    ) -> bool {
        &self.owner == current_owner
            && self
                .ready
                .as_ref()
                .is_some_and(|index| index.insert_if_current(ws, composer, trigger, choice))
    }
}
impl<O> Build<O> {
    pub fn ticket(&self) -> Ticket {
        self.ticket.clone()
    }
    /// Pure owned CPU work: suitable for spawn_blocking, never called by render.
    pub fn finish(self) -> Completed<O> {
        Completed {
            ticket: self.ticket,
            owner: self.owner,
            index: CheckedIndex::build(self.snapshot),
        }
    }
}

#[cfg(test)]
mod tests;
