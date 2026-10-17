use super::{Source, Storage, delivery, domain, retain};
use domain::{Attempt, Conflict, Content, Draft, LateResultEvidence, RetiredAttempt};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::watch;

// Each reservation charges the maximum pair before native evaluation. No
// eviction. The root must carry this allocation across ShellState replacement.
pub(in crate::canvas_create) const LIMIT: usize = 32;

#[derive(Clone)]
pub(in crate::canvas_create) enum Phase {
    CaptureReserved,
    Saving,
    Saved(Draft),
    Delivering,
    Conflict(Conflict),
    Canceled(Attempt),
    Settled(Attempt),
    RetiredSame(RetiredAttempt),
    LateEvidence(LateResultEvidence),
    CaptureDisposed,
    Review {
        draft: Draft,
        attempt: Option<Attempt>,
    },
    // Safe fixed categories only. Input/result remains in Job; no {error} log.
    Held(Failure),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::canvas_create) enum Failure {
    Storage,
    Changed,
    Capacity,
    Blocked,
    Cooldown,
}
impl From<domain::Fault> for Failure {
    fn from(value: domain::Fault) -> Self {
        match value {
            domain::Fault::Capacity => Self::Capacity,
            domain::Fault::Stale | domain::Fault::Retired => Self::Changed,
            domain::Fault::Blocked => Self::Blocked,
            domain::Fault::Cooldown => Self::Cooldown,
            _ => Self::Storage,
        }
    }
}

pub(in crate::canvas_create) struct Job {
    active: AtomicBool,
    pub(in crate::canvas_create) cancel_requested: AtomicBool,
    pub(in crate::canvas_create) storage: Storage,
    pub(in crate::canvas_create) base: Draft,
    pub(in crate::canvas_create) writer: String,
    pub(in crate::canvas_create) operation: String,
    pub(in crate::canvas_create) captured: Mutex<Option<Content>>,
    pub(in crate::canvas_create) saved: Mutex<Option<Draft>>,
    pub(in crate::canvas_create) reserve_started: AtomicBool,
    pub(in crate::canvas_create) reservation: Mutex<Option<domain::Reservation>>,
    pub(in crate::canvas_create) observed: Mutex<Option<(Attempt, delivery::Evidence)>>,
    phase: Mutex<Phase>,
    changed: watch::Sender<u64>,
}
impl Job {
    pub(in crate::canvas_create) fn publish(&self, phase: Phase) {
        *self.phase.lock().expect("Canvas custody lock") = phase;
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }
}
#[derive(Clone, Default)]
pub(in crate::canvas_create) struct Custody(Arc<Mutex<BTreeMap<String, Arc<Job>>>>);
#[derive(Clone)]
pub(in crate::canvas_create) struct Ticket {
    pub(in crate::canvas_create) job: Arc<Job>,
}
impl Custody {
    /// Controller-only handoff from an already acknowledged editor save. This
    /// does not advance the draft revision. It grants no dispatch permit: the
    /// worker still performs the exact durable reserve/admit transactions.
    pub(in crate::canvas_create) fn reserve_saved(
        &self,
        storage: Storage,
        base: Draft,
    ) -> domain::Result<Ticket> {
        let ticket = self.reserve(storage, base.clone())?;
        *ticket.job.captured.lock().expect("Canvas custody lock") = Some(base.content().clone());
        *ticket.job.saved.lock().expect("Canvas custody lock") = Some(base.clone());
        ticket.job.publish(Phase::Saved(base));
        Ok(ticket)
    }
    /// Must precede native eval. Base is a coherent durable snapshot loaded by
    /// the future explicit controller, never synthesized from visible text.
    pub(in crate::canvas_create) fn reserve(
        &self,
        storage: Storage,
        base: Draft,
    ) -> domain::Result<Ticket> {
        if storage.owner() != base.owner() || base.archived() {
            return Err(domain::Fault::Stale);
        }
        let mut jobs = self.0.lock().expect("Canvas custody lock");
        if jobs.len() >= LIMIT {
            return Err(domain::Fault::Capacity);
        }
        let operation = uuid::Uuid::new_v4().to_string();
        let (changed, _) = watch::channel(0);
        let job = Arc::new(Job {
            active: AtomicBool::new(true),
            cancel_requested: AtomicBool::new(false),
            storage,
            base,
            writer: uuid::Uuid::new_v4().to_string(),
            operation: operation.clone(),
            captured: Mutex::new(None),
            saved: Mutex::new(None),
            reserve_started: AtomicBool::new(false),
            reservation: Mutex::new(None),
            observed: Mutex::new(None),
            phase: Mutex::new(Phase::CaptureReserved),
            changed,
        });
        jobs.insert(operation, job.clone());
        Ok(Ticket { job })
    }

    // Only a failed capture, before accepting any native payload, can free its
    // empty reservation. Never call this for a late successful native reply.
    pub(in crate::canvas_create) fn abandon_empty(&self, ticket: &Ticket) -> bool {
        if !matches!(ticket.phase(), Phase::CaptureReserved) {
            return false;
        }
        self.remove_exact(ticket)
    }
    /// Controller has observed the durable result. UI dismissal itself cannot
    /// acknowledge. Held/Saving/Delivering slots stay charged for recovery.
    pub(in crate::canvas_create) fn acknowledge(&self, ticket: &Ticket) -> bool {
        if !matches!(
            ticket.phase(),
            Phase::Saved(_)
                | Phase::Conflict(_)
                | Phase::Canceled(_)
                | Phase::Settled(_)
                | Phase::RetiredSame(_)
                | Phase::LateEvidence(_)
                | Phase::CaptureDisposed
                | Phase::Review { .. }
        ) {
            return false;
        }
        self.remove_exact(ticket)
    }
    fn remove_exact(&self, ticket: &Ticket) -> bool {
        let mut jobs = self.0.lock().expect("Canvas custody lock");
        if !jobs
            .get(&ticket.job.operation)
            .is_some_and(|j| Arc::ptr_eq(j, &ticket.job))
        {
            return false;
        }
        jobs.remove(&ticket.job.operation);
        ticket.job.active.store(false, Ordering::Release);
        true
    }
    /// Bounded stable-owner recovery; never exposes another owner's content.
    pub(in crate::canvas_create) fn tickets(&self, owner: &domain::Owner) -> Vec<Ticket> {
        self.0
            .lock()
            .expect("Canvas custody lock")
            .values()
            .filter(|j| j.base.owner() == owner)
            .map(|job| Ticket { job: job.clone() })
            .collect()
    }
}
impl Ticket {
    pub(in crate::canvas_create) fn phase(&self) -> Phase {
        self.job.phase.lock().expect("Canvas custody lock").clone()
    }
    pub(in crate::canvas_create) fn changes(&self) -> watch::Receiver<u64> {
        self.job.changed.subscribe()
    }
    pub(in crate::canvas_create) fn captured(&self) -> Option<Content> {
        self.job
            .captured
            .lock()
            .expect("Canvas custody lock")
            .clone()
    }
    /// Synchronous custody transfer first, then retained persistence. No source
    /// or current-visit check may discard a successful late capture here.
    pub(in crate::canvas_create) fn save(
        &self,
        content: Content,
    ) -> Result<(), (domain::Fault, Content)> {
        if !self.job.active.load(Ordering::Acquire)
            || !matches!(self.phase(), Phase::CaptureReserved)
        {
            return Err((domain::Fault::Stale, content));
        }
        *self.job.captured.lock().expect("Canvas custody lock") = Some(content.clone());
        self.job.publish(Phase::Saving);
        let job = self.job.clone();
        retain(async move {
            super::recovery::save(&job, content).await;
        });
        Ok(())
    }
    /// Explicit user Create only. A clone cannot replay this job. No admission
    /// exists until the already-saved exact snapshot commits its Started row.
    pub(in crate::canvas_create) fn create(&self, source: impl Source) -> domain::Result<()> {
        if !self.job.active.load(Ordering::Acquire) {
            return Err(domain::Fault::Stale);
        }
        let Phase::Saved(saved) = self.phase() else {
            return Err(domain::Fault::Blocked);
        };
        if source.owner() != saved.owner() || !source.current() {
            return Err(domain::Fault::Stale);
        }
        let request = super_platinum_core::slack::api::canvas_create(
            source.client(),
            source.workspace(),
            saved.content().title(),
            saved.content().markdown(),
        )
        .map_err(|_| domain::Fault::Invalid)?;
        source.prepared(&request);
        self.job.cancel_requested.store(false, Ordering::Release);
        self.job.publish(Phase::Delivering);
        let job = self.job.clone();
        retain(async move {
            delivery::run(job, saved, source, request).await;
        });
        Ok(())
    }

    // Requests cancellation, not proof of known-unsent delivery. Only the
    // eventual Canceled(core receipt) phase provides that proof.
    pub(in crate::canvas_create) fn cancel_before_admission(&self) {
        if self.job.active.load(Ordering::Acquire) && matches!(self.phase(), Phase::Delivering) {
            self.job.cancel_requested.store(true, Ordering::Release);
        }
    }

    /// Explicit local persistence retry only; never rebuilds or executes a request.
    pub(in crate::canvas_create) fn retry_result_storage(&self) -> domain::Result<()> {
        if !self.job.active.load(Ordering::Acquire) || !matches!(self.phase(), Phase::Held(_)) {
            return Err(domain::Fault::Blocked);
        }
        let (original, evidence) = self
            .job
            .observed
            .lock()
            .expect("Canvas custody lock")
            .clone()
            .ok_or(domain::Fault::Blocked)?;
        self.job.publish(Phase::Delivering);
        let job = self.job.clone();
        retain(async move {
            if let Err(error) = delivery::persist(&job, original, evidence).await {
                job.publish(Phase::Held(error.into()));
            }
        });
        Ok(())
    }
    /// Explicit local reconciliation only; no Source or request is captured.
    pub(in crate::canvas_create) fn retry_storage(&self) -> domain::Result<()> {
        if !self.job.active.load(Ordering::Acquire) || !matches!(self.phase(), Phase::Held(_)) {
            return Err(domain::Fault::Blocked);
        }
        let has_result = self
            .job
            .observed
            .lock()
            .expect("Canvas custody lock")
            .is_some();
        if has_result {
            return self.retry_result_storage();
        }
        self.job.publish(Phase::Saving);
        let job = self.job.clone();
        retain(async move {
            super::recovery::retry(&job).await;
        });
        Ok(())
    }
}
