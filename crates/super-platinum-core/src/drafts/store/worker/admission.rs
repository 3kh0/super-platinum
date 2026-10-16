//! Private Owned queue admission. No account certificate, drain, close or Current.
use super::*;
use std::{any::Any, sync::Arc};
const MAX_SLOTS: usize = 64;
const MAX_COMPONENT: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reason {
    Fencing,
    Unsupported,
    Stale,
    Capacity,
    Counter,
    Metadata,
    Incomplete,
    Dead,
    ColdRequired,
}
pub(super) struct Refused<T> {
    pub reason: Reason,
    pub generation: Option<u64>,
    pub input: T,
}
impl<T> std::fmt::Debug for Refused<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Refused")
            .field("reason", &self.reason)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}
pub(super) struct Identity {
    // Retained handles keep the real owner lock, not the Queue (no strong cycle).
    pub owner: Arc<super::super::protected::upgrade::Owned>,
    pub worker: String,
}
pub(super) struct State {
    pub identity: Arc<Identity>,
    pub generation: u64,
    next: u64,
    slots: BTreeMap<u64, Entry>,
    fence: Option<Arc<()>>,
    pub unsupported: bool,
    pub dead: bool,
    pub cold_ready: bool,
    incomplete: Option<Reason>,
}
struct Entry {
    identity: Arc<()>,
    generation: u64,
    parent: Option<u64>,
    family: &'static str,
    retained: Arc<dyn Any + Send + Sync>,
}
#[derive(Clone)]
pub(super) struct Key {
    route: Arc<Identity>,
    id: u64,
    identity: Arc<()>,
}
pub(super) struct RetainedKey {
    pub key: Key,
    pub family: &'static str,
    pub generation: u64,
}
#[derive(Clone)]
pub(super) struct Fence {
    identity: Arc<Identity>,
    generation: u64,
    nonce: Arc<()>,
    // Queued, detached AND historical IDs; this is inventory, never readiness.
    pub records: Vec<u64>,
    pub slots: Vec<u64>,
}
impl State {
    pub(super) fn new(owner: Arc<super::super::protected::upgrade::Owned>, worker: &str) -> Self {
        Self {
            identity: Arc::new(Identity {
                owner,
                worker: worker.into(),
            }),
            generation: 1,
            next: 0,
            slots: BTreeMap::new(),
            fence: None,
            unsupported: false,
            dead: false,
            cold_ready: false,
            incomplete: None,
        }
    }
    pub(super) fn check(&self, account: &str, completion: bool) -> Result<(), Reason> {
        if self.dead {
            return Err(Reason::Dead);
        }
        if account != self.identity.owner.account() {
            return Err(Reason::Stale);
        }
        if self.unsupported {
            return Err(Reason::Unsupported);
        }
        if let Some(reason) = self.incomplete {
            return Err(reason);
        }
        if !self.cold_ready {
            return Err(Reason::ColdRequired);
        }
        if self.fence.is_some() && !completion {
            return Err(Reason::Fencing);
        }
        Ok(())
    }
    pub(super) fn reserve(&mut self, account: &str) -> Result<u64, Reason> {
        self.check(account, false)?;
        if self.slots.len() >= MAX_SLOTS {
            self.incomplete = Some(Reason::Capacity);
            return Err(Reason::Capacity);
        }
        let Some(id) = self.next.checked_add(1) else {
            self.incomplete = Some(Reason::Counter);
            return Err(Reason::Counter);
        };
        self.next = id;
        Ok(id)
    }
    fn register(
        &mut self,
        id: u64,
        parent: Option<u64>,
        family: &'static str,
        retained: Arc<dyn Any + Send + Sync>,
    ) -> Key {
        let identity = Arc::new(());
        self.slots.insert(
            id,
            Entry {
                identity: identity.clone(),
                generation: self.generation,
                parent,
                family,
                retained,
            },
        );
        Key {
            route: self.identity.clone(),
            id,
            identity,
        }
    }
    fn entry(&self, key: &Key) -> Result<&Entry, Reason> {
        if !Arc::ptr_eq(&self.identity, &key.route) {
            return Err(Reason::Stale);
        }
        self.slots
            .get(&key.id)
            .filter(|entry| Arc::ptr_eq(&entry.identity, &key.identity))
            .ok_or(Reason::Stale)
    }
    pub(super) fn begin(&mut self, records: Vec<u64>) -> Result<Fence, Reason> {
        self.check(self.identity.owner.account(), false)?;
        let nonce = Arc::new(());
        self.fence = Some(nonce.clone());
        Ok(Fence {
            identity: self.identity.clone(),
            generation: self.generation,
            nonce,
            records,
            slots: self.slots.keys().copied().collect(),
        })
    }
    pub(super) fn inspect(&self, fence: &Fence) -> Result<(), Reason> {
        if self.dead {
            return Err(Reason::Dead);
        }
        if self.unsupported {
            return Err(Reason::Unsupported);
        }
        if let Some(reason) = self.incomplete {
            return Err(reason);
        }
        if !Arc::ptr_eq(&self.identity, &fence.identity)
            || self.generation != fence.generation
            || self
                .fence
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &fence.nonce))
        {
            return Err(Reason::Stale);
        }
        Ok(())
    }
    pub(super) fn resume(&mut self, fence: &Fence) -> Result<(), Reason> {
        self.inspect(fence)?;
        let Some(next) = self.generation.checked_add(1) else {
            self.incomplete = Some(Reason::Counter);
            return Err(Reason::Counter);
        };
        self.generation = next;
        self.fence = None;
        Ok(())
    }
}
fn refusal<T>(pending: &mut Pending, reason: Reason, input: T) -> Refused<T> {
    if matches!(
        reason,
        Reason::Metadata | Reason::Capacity | Reason::Counter | Reason::Incomplete
    ) {
        if let Some(state) = &mut pending.admission {
            state.incomplete.get_or_insert(reason);
        }
    }
    Refused {
        reason,
        generation: pending.admission.as_ref().map(|a| a.generation),
        input,
    }
}
pub(super) fn bounded(account: &str, key: Option<&DraftKey>) -> bool {
    account.len() <= MAX_COMPONENT
        && key.is_none_or(|key| {
            key.team.len() <= MAX_COMPONENT
                && key.channel.len() <= MAX_COMPONENT
                && key.thread.as_ref().is_none_or(|s| s.len() <= MAX_COMPONENT)
        })
}
impl Worker {
    // A private owned holder has already retained the input/receiver. Rejection
    // returns the exact Request including its sender, without dropping or cloning.
    fn try_submit_locked(
        pending: &mut Pending,
        key: &Key,
        request: Request,
    ) -> Result<u64, Refused<Request>> {
        let Some(admission) = &pending.admission else {
            return Err(refusal(pending, Reason::Unsupported, request));
        };
        let entry = match admission.entry(key) {
            Ok(entry) => entry,
            Err(reason) => return Err(refusal(pending, reason, request)),
        };
        let completion = entry.parent.is_some();
        if !completion && entry.generation != admission.generation {
            return Err(refusal(pending, Reason::Stale, request));
        }
        let actual = request.identity();
        if let Err(reason) = admission.check(actual.account, completion) {
            return Err(refusal(pending, reason, request));
        }
        if !bounded(actual.account, actual.key) {
            return Err(refusal(pending, Reason::Metadata, request));
        }
        if pending
            .accounting
            .as_ref()
            .is_none_or(|book| !book.admission_room(1))
        {
            return Err(refusal(pending, Reason::Incomplete, request));
        }
        let Some(upgrade_generation) = pending.upgrade_generation.checked_add(1) else {
            return Err(refusal(pending, Reason::Counter, request));
        };
        let index = pending.requests.len();
        let Some(id) = pending
            .accounting
            .as_mut()
            .unwrap()
            .request(index, request.identity())
        else {
            return Err(refusal(pending, Reason::Incomplete, request));
        };
        pending.upgrade_generation = upgrade_generation;
        pending.requests.push(request);
        Ok(id)
    }
    pub(super) fn try_enqueue(
        &self,
        identity: &Arc<Identity>,
        generation: u64,
        account: String,
        entries: BTreeMap<DraftKey, LocalDraft>,
    ) -> Result<(), Refused<(String, BTreeMap<DraftKey, LocalDraft>)>> {
        let mut pending = self.pending.lock().unwrap();
        let error = (|| {
            let admission = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
            if !Arc::ptr_eq(identity, &admission.identity) || generation != admission.generation {
                return Err(Reason::Stale);
            }
            admission.check(&account, false)?;
            if !bounded(&account, None) || entries.keys().any(|key| !bounded(&account, Some(key))) {
                return Err(Reason::Metadata);
            }
            if pending
                .accounting
                .as_ref()
                .is_none_or(|book| !book.admission_room(entries.len()))
            {
                return Err(Reason::Incomplete);
            }
            pending
                .upgrade_generation
                .checked_add(1)
                .ok_or(Reason::Counter)
        })();
        let next = match error {
            Ok(next) => next,
            Err(reason) => return Err(refusal(&mut pending, reason, (account, entries))),
        };
        pending.upgrade_generation = next;
        for (key, draft) in entries {
            pending
                .accounting
                .as_mut()
                .unwrap()
                .update(&account, &key, false);
            pending.updates.insert((account.clone(), key), draft);
        }
        drop(pending);
        self.ready.notify_one();
        Ok(())
    }
}
#[path = "retained.rs"]
mod retained;
pub(super) use retained::*;

#[path = "completion.rs"]
mod completion;
pub(super) use completion::{
    BaselineInput, CheckInput, Checkpointed, Claimed, ClaimedBaseline, ClaimedCheck, Completion,
    Dispatch, Finished, ObservationInput, Prepared, Preserved, Raw, Registered, Released,
    ReservedBaseline, ReservedCheck, ReservedObservation, SendClaimed, SendConsumed, SendPrepared,
    Step, StepState, TakeoverInput,
};

pub use completion::{
    CheckpointInput, LocalOutcome, LocalWriteInput, MutationInput, OpenInput, PreserveInput,
    ReleaseInput, SendInput, TeamInput, WriteOutcome,
};

#[path = "accounted_wait.rs"]
mod accounted_wait;

pub(in crate::drafts::store::worker) use completion::{
    SlashClaimed, SlashCompletion, SlashReserved,
};
