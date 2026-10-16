//! Root-lived, bounded explicit draft I/O. No UI wiring, autosave or Slack work.
//! Reserve synchronously under Custody before capturing editor input. A result
//! waiter owns no worker; terminal evidence remains until explicit acknowledgement.
use super::{Refusal, admission, drafts::Visit, recovery::Storage};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use super_platinum_core::file_sharing::{
    Receipt,
    store::drafts::{Content, Draft, DraftScope},
};
use tokio::sync::watch;
mod bounds;
mod diagnostic;
mod execute;
const LIMIT: usize = 32;
type Key = (admission::Key, String);

#[derive(Clone)]
pub(super) enum Command {
    Read,
    Retain {
        content: Content,
        reason: Refusal,
    },
    LoadClaim {
        initial: Content,
    },
    Claim {
        expected: Option<Draft>,
        initial: Content,
    },
    Save {
        expected: Draft,
        content: Content,
    },
    /// Empty input captured from an editable Close; distinct from deleting a
    /// readonly saved body so failure review never substitutes the old content.
    Clear {
        expected: Draft,
        content: Content,
    },
    Discard {
        expected: Draft,
    },
}
#[derive(Clone)]
pub(super) enum Cleanup {
    Removed,
    KeptNonempty,
    Conflict(Option<Draft>),
    Blocked(Receipt),
    Failed(Refusal),
}
#[derive(Clone)]
pub(super) enum Outcome {
    Read(Option<Draft>),
    Claimed { draft: Draft, created: bool },
    Review(Draft),
    Saved(Draft),
    Discarded,
    Conflict(Option<Draft>),
    Blocked(Receipt),
    Capacity,
    Failed(Refusal),
    Canceled { claim: Draft, cleanup: Cleanup },
}
struct Job {
    id: u64,
    key: Key,
    visit: Arc<Visit>,
    storage: Storage,
    command: OnceLock<Command>,
    outcome: OnceLock<Outcome>,
    observed: AtomicBool,
    cancel_load: Mutex<bool>,
    done: watch::Sender<bool>,
    runtime: tokio::runtime::Handle,
    #[cfg(test)]
    gate: Option<(GatePoint, Arc<TestGate>)>,
}
struct Inner {
    next: Option<u64>,
    jobs: BTreeMap<u64, Arc<Job>>,
    scopes: BTreeMap<Key, VecDeque<u64>>,
    running: BTreeSet<Key>,
    #[cfg(test)]
    gate: Option<(GatePoint, Arc<TestGate>)>,
}
impl Default for Inner {
    fn default() -> Self {
        Self {
            next: Some(0),
            jobs: Default::default(),
            scopes: Default::default(),
            running: Default::default(),
            #[cfg(test)]
            gate: None,
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct Queue(Arc<Mutex<Inner>>);
/// An unsubmitted reservation owns no captured input. Dropping it removes its
/// FIFO slot and wakes the next ready job; an abandoned slot cannot wedge scope.
pub(super) struct Reservation {
    queue: Queue,
    job: Arc<Job>,
    submitted: bool,
}
/// Rejected input stays owned by the caller, never silently dropped or retained
/// outside the queue's byte budget. The unused reservation releases its slot.
pub(super) struct Rejected {
    pub reason: Refusal,
    pub command: Command,
}
#[derive(Clone)]
pub(super) struct Ticket {
    job: Arc<Job>,
}
impl Queue {
    pub(super) fn reserve(
        &self,
        storage: Storage,
        visit: Arc<Visit>,
    ) -> Result<Reservation, Refusal> {
        if storage.key != visit.key.0 || !visit.active() {
            return Err(Refusal::Changed);
        }
        if !bounds::visit(&visit) {
            return Err(Refusal::Source);
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| Refusal::Storage)?;
        let mut inner = self.0.lock().map_err(|_| Refusal::Storage)?;
        if inner.jobs.len() >= LIMIT {
            return Err(Refusal::Capacity);
        }
        let id = inner.next.ok_or(Refusal::Capacity)?;
        inner.next = id.checked_add(1);
        let (done, _) = watch::channel(false);
        let job = Arc::new(Job {
            id,
            key: visit.key.clone(),
            visit,
            storage,
            command: OnceLock::new(),
            outcome: OnceLock::new(),
            observed: AtomicBool::new(false),
            cancel_load: Mutex::new(false),
            done,
            runtime,
            #[cfg(test)]
            gate: inner.gate.take(),
        });
        inner
            .scopes
            .entry(job.key.clone())
            .or_default()
            .push_back(id);
        inner.jobs.insert(id, job.clone());
        Ok(Reservation {
            queue: self.clone(),
            job,
            submitted: false,
        })
    }
    /// Bounded retained inputs/results, including lost result waiters. Reading
    /// this list does not acknowledge or remove any operation.
    pub(super) fn retained(&self) -> Result<Vec<Ticket>, Refusal> {
        let inner = self.0.lock().map_err(|_| Refusal::Storage)?;
        Ok(inner
            .jobs
            .values()
            .map(|job| Ticket { job: job.clone() })
            .collect())
    }
    /// Explicit caller confirmation after consuming success or reviewing a
    /// retained conflict/failure. Unobserved and unfinished outcomes cannot drop.
    pub(super) fn acknowledge(&self, ticket: &Ticket) -> Result<bool, Refusal> {
        let mut inner = self.0.lock().map_err(|_| Refusal::Storage)?;
        if !inner
            .jobs
            .get(&ticket.job.id)
            .is_some_and(|j| Arc::ptr_eq(j, &ticket.job))
            || ticket.job.outcome.get().is_none()
            || !ticket.job.observed.load(Ordering::Acquire)
        {
            return Ok(false);
        }
        inner.jobs.remove(&ticket.job.id);
        Ok(true)
    }
    fn kick(&self, key: Key) {
        let runtime = {
            let Ok(mut inner) = self.0.lock() else { return };
            if inner.running.contains(&key) {
                return;
            }
            let Some(job) = inner
                .scopes
                .get(&key)
                .and_then(|q| q.front())
                .and_then(|id| inner.jobs.get(id))
            else {
                return;
            };
            if job.command.get().is_none() {
                return;
            }
            let runtime = job.runtime.clone();
            inner.running.insert(key.clone());
            runtime
        };
        let queue = self.clone();
        // Detached from component/task lifetime. No JoinHandle is exposed to UI.
        runtime.spawn(async move {
            queue.run(key).await;
        });
    }
    async fn run(self, key: Key) {
        loop {
            let job = {
                let Ok(mut inner) = self.0.lock() else { return };
                let job = inner
                    .scopes
                    .get(&key)
                    .and_then(|q| q.front())
                    .and_then(|id| inner.jobs.get(id))
                    .cloned();
                let Some(job) = job.filter(|j| j.command.get().is_some()) else {
                    inner.running.remove(&key);
                    return;
                };
                job
            };
            #[cfg(test)]
            if let Some((GatePoint::BeforeIo, gate)) = &job.gate {
                gate.pause().await;
            }
            let outcome = execute::run(&job).await;
            #[cfg(test)]
            if let Some((GatePoint::AfterIo, gate)) = &job.gate {
                gate.pause().await;
            }
            // Serialize cancellation request with publication. If publication
            // already won, cancel_load returns false and cannot claim cleanup.
            let canceled = {
                let canceled = job.cancel_load.lock();
                match canceled {
                    Ok(flag) if *flag && matches!(&outcome, Outcome::Claimed { .. }) => true,
                    _ => {
                        let _ = job.outcome.set(outcome.clone());
                        false
                    }
                }
            };
            if canceled {
                let cleaned = execute::cancel(&job, outcome).await;
                let _ = job.outcome.set(cleaned);
            }
            job.done.send_replace(true);
            #[cfg(test)]
            if let Some((GatePoint::Published, gate)) = &job.gate {
                gate.pause().await;
            }
            let Ok(mut inner) = self.0.lock() else { return };
            if let Some(scope) = inner.scopes.get_mut(&key) {
                if scope.front() == Some(&job.id) {
                    scope.pop_front();
                }
                if scope.is_empty() {
                    inner.scopes.remove(&key);
                }
            }
        }
    }
    #[cfg(test)]
    pub(super) fn gate_next(&self, point: GatePoint, gate: Arc<TestGate>) {
        self.0.lock().unwrap().gate = Some((point, gate));
    }
}
impl Reservation {
    pub(super) fn observe(&self) -> Ticket {
        Ticket {
            job: self.job.clone(),
        }
    }
    pub(super) fn submit(mut self, command: Command) -> Result<Ticket, Rejected> {
        if let Err(reason) = bounds::command(&command) {
            let _ = self.job.outcome.set(Outcome::Failed(reason));
            return Err(Rejected { reason, command });
        }
        // This slot was reserved before capture. Bounded stale commands remain
        // retained through typed failures/conflicts; oversized input is returned.
        let _ = self.job.command.set(command);
        self.submitted = true;
        let ticket = Ticket {
            job: self.job.clone(),
        };
        self.queue.kick(self.job.key.clone());
        Ok(ticket)
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.submitted {
            return;
        }
        let _ = self.job.outcome.set(Outcome::Failed(Refusal::Changed));
        self.job.done.send_replace(true);
        if let Ok(mut inner) = self.queue.0.lock() {
            inner.jobs.remove(&self.job.id);
            if let Some(scope) = inner.scopes.get_mut(&self.job.key) {
                scope.retain(|id| *id != self.job.id);
                if scope.is_empty() {
                    inner.scopes.remove(&self.job.key);
                }
            }
        }
        self.queue.kick(self.job.key.clone());
    }
}
impl Ticket {
    pub(super) fn input_ref(&self) -> Option<&Command> {
        self.job.command.get()
    }
    pub(super) fn outcome_ref(&self) -> Option<&Outcome> {
        self.job.outcome.get()
    }
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.job, &other.job)
    }
    pub(super) fn id(&self) -> u64 {
        self.job.id
    }
    pub(super) fn storage(&self) -> Storage {
        self.job.storage.clone()
    }
    /// Captured scope/source identity for later projection; never a live grant.
    pub(super) fn visit(&self) -> &Arc<Visit> {
        &self.job.visit
    }
    pub(super) fn input(&self) -> Option<Command> {
        self.job.command.get().cloned()
    }
    pub(super) fn outcome(&self) -> Option<Outcome> {
        let outcome = self.job.outcome.get()?.clone();
        self.job.observed.store(true, Ordering::Release);
        Some(outcome)
    }
    pub(super) async fn wait(&self) -> Outcome {
        let mut changed = self.job.done.subscribe();
        loop {
            if let Some(outcome) = self.outcome() {
                return outcome;
            }
            if changed.changed().await.is_err() {
                return Outcome::Failed(Refusal::Storage);
            }
        }
    }
    pub(super) fn cancel_load(&self) -> bool {
        if !matches!(
            self.job.command.get(),
            Some(Command::LoadClaim { .. } | Command::Claim { .. })
        ) {
            return false;
        }
        let Ok(mut canceled) = self.job.cancel_load.lock() else {
            return false;
        };
        if self.job.outcome.get().is_some() {
            return false;
        }
        *canceled = true;
        true
    }
}
#[cfg(test)]
pub(super) enum GatePoint {
    BeforeIo,
    AfterIo,
    Published,
}
#[cfg(test)]
pub(super) struct TestGate {
    pub reached: tokio::sync::Semaphore,
    pub release: tokio::sync::Semaphore,
}
#[cfg(test)]
impl TestGate {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            reached: tokio::sync::Semaphore::new(0),
            release: tokio::sync::Semaphore::new(0),
        })
    }
    async fn pause(&self) {
        self.reached.add_permits(1);
        if let Ok(permit) = self.release.acquire().await {
            permit.forget();
        }
    }
}
