//! Component observations only: no account-wide fence, Current or close proof.
use super::*;
use std::sync::{Mutex, Weak, atomic::AtomicU64};

const MAX_QUEUES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    Account,
    Changed,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Live,
    GapRequested(durable::GapReason),
    Dropped,
}
#[derive(Default)]
pub(in super::super) struct Clock {
    generation: AtomicU64,
    incomplete: AtomicBool,
}
impl Clock {
    /// Call under the actual owner's mutation lock, before changing its state.
    /// The clock never wraps or recovers after tracking becomes incomplete.
    pub(in super::super) fn change(&self) {
        if self
            .generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .is_err()
        {
            self.unknown();
        }
    }
    pub(in super::super) fn unknown(&self) {
        self.incomplete.store(true, Ordering::Release);
    }
    fn read(&self) -> Result<u64, Refused> {
        if self.incomplete.load(Ordering::Acquire) {
            return Err(Refused::Unknown);
        }
        Ok(self.generation.load(Ordering::Acquire))
    }
}

pub(in super::super) struct Record {
    account: Weak<Account>,
    socket: Arc<observation::Source>,
    frame_generation: u64,
    stream: durable::Stream,
    revoked: Arc<AtomicBool>,
    pub(in super::super) clock: Arc<Clock>,
    state: Mutex<State>,
}
struct State {
    phase: Phase,
    gap: Option<durable::GapReason>,
    queues: Vec<Weak<ObservationQueue>>,
}
/// Retains stream/provenance without retaining the Source whose Drop revokes it.
/// Queue registrations are weak to avoid Source -> barrier -> Source cycles.
#[derive(Clone)]
pub(crate) struct Handle(Arc<Record>);
pub(crate) struct Snapshot {
    record: Arc<Record>,
    generation: u64,
    pub(crate) phase: Phase,
    /// No completion status is inferred from the Account's asynchronous gap job.
    pub(crate) gap: Option<durable::GapReason>,
    socket_current: bool,
    stream_live: bool,
    revoked: bool,
    team_blocked: bool,
    registrations: Vec<Weak<ObservationQueue>>,
    pub(crate) queues: Vec<super::super::observations::inventory::Snapshot>,
}
impl Record {
    pub(super) fn new(
        account: &Arc<Account>,
        socket: Arc<observation::Source>,
        frame_generation: u64,
        stream: durable::Stream,
        revoked: Arc<AtomicBool>,
    ) -> Arc<Self> {
        Arc::new(Self {
            account: Arc::downgrade(account),
            socket,
            frame_generation,
            stream,
            revoked,
            clock: Arc::default(),
            state: Mutex::new(State {
                phase: Phase::Live,
                gap: None,
                queues: vec![],
            }),
        })
    }
    pub(in super::super) fn register(&self, queue: &Arc<ObservationQueue>) {
        let Ok(mut state) = self.state.lock() else {
            self.clock.unknown();
            return;
        };
        self.clock.change();
        if state.queues.len() == MAX_QUEUES {
            self.clock.unknown();
            return;
        }
        state.queues.push(Arc::downgrade(queue));
    }
    pub(super) fn phase(&self, phase: Phase) {
        let Ok(mut state) = self.state.lock() else {
            self.clock.unknown();
            return;
        };
        self.clock.change();
        match phase {
            Phase::GapRequested(reason) => {
                state.gap = Some(reason);
            }
            Phase::Dropped => {
                state
                    .gap
                    .get_or_insert(durable::GapReason::InterruptedAdmission);
            }
            Phase::Live => {}
        }
        state.phase = phase;
    }
    pub(in super::super) fn same_account(&self, account: &Arc<Account>) -> bool {
        self.account
            .upgrade()
            .is_some_and(|a| Arc::ptr_eq(&a, account))
    }
    fn account(&self, account: &Arc<Account>) -> Result<(), Refused> {
        self.account
            .upgrade()
            .filter(|a| Arc::ptr_eq(a, account))
            .map(|_| ())
            .ok_or(Refused::Account)
    }
}
impl Source {
    pub(crate) fn ownership(&self) -> Handle {
        Handle(self.inventory.clone())
    }
}
impl Handle {
    pub(crate) fn snapshot(
        &self,
        account: &Arc<Account>,
        shell: &ShellState,
    ) -> Result<Snapshot, Refused> {
        self.0.account(account)?;
        let generation = self.0.clock.read()?;
        let (phase, gap, registrations) = {
            let state = self.0.state.lock().map_err(|_| Refused::Unknown)?;
            (state.phase, state.gap, state.queues.clone())
        };
        let mut queues = Vec::with_capacity(registrations.len());
        for weak in &registrations {
            let Some(queue) = weak.upgrade() else {
                // A disappeared queue is not evidence that its results were consumed.
                self.0.clock.unknown();
                return Err(Refused::Unknown);
            };
            queues.push(queue.inventory()?);
        }
        let snapshot = Snapshot {
            record: self.0.clone(),
            generation,
            phase,
            gap,
            socket_current: self.0.socket.current(shell, self.0.frame_generation),
            stream_live: self.0.stream.is_live(),
            revoked: self.0.revoked.load(Ordering::Acquire),
            team_blocked: account.team_blocked(self.0.stream.team()),
            registrations,
            queues,
        };
        snapshot.validate(self, account, shell)?;
        Ok(snapshot)
    }
}
impl Snapshot {
    /// An unchanged component observation, never an admission capability.
    /// Real state may change immediately after return. A later joint fence must
    /// revalidate this with every other owner under its own admission protocol.
    pub(crate) fn validate(
        &self,
        handle: &Handle,
        account: &Arc<Account>,
        shell: &ShellState,
    ) -> Result<(), Refused> {
        if !Arc::ptr_eq(&self.record, &handle.0) {
            return Err(Refused::Changed);
        }
        self.record.account(account)?;
        if self.record.state.is_poisoned() {
            self.record.clock.unknown();
            return Err(Refused::Unknown);
        }
        for weak in &self.registrations {
            if weak.upgrade().is_none_or(|queue| !queue.inventory_known()) {
                self.record.clock.unknown();
                return Err(Refused::Unknown);
            }
        }
        if self.generation != self.record.clock.read()?
            || self.socket_current
                != self
                    .record
                    .socket
                    .current(shell, self.record.frame_generation)
            || self.stream_live != self.record.stream.is_live()
            || self.revoked != self.record.revoked.load(Ordering::Acquire)
            || self.team_blocked != account.team_blocked(self.record.stream.team())
        {
            return Err(Refused::Changed);
        }
        if self.generation != self.record.clock.read()? {
            return Err(Refused::Changed);
        }
        Ok(())
    }
}

#[cfg(test)]
pub(in super::super) mod tests;
