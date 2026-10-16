//! Retained evidence for one renderer-owned Send/Schedule, never dispatch authority.
use super::ownership::Tracked as Mutex;
use super::*;
use crate::drafts::network::Attempt;
use std::sync::{
    Weak,
    atomic::{AtomicBool, Ordering},
};

pub(crate) const MAX_OPERATIONS: usize = 16;
/// Borrowed byte admission before renderer capture clones any payload.
pub(crate) fn bounded<T: serde::Serialize>(value: &T) -> Result<(), durable::Error> {
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > super::retention::MAX_INPUT_BYTES.saturating_sub(self.0) {
                return Err(std::io::Error::other("renderer operation input bound"));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(0), value).map_err(|_| durable::Error::Unsupported)
}
#[derive(Clone)]
pub(crate) enum Request {
    Send,
    Schedule {
        snapshot: super_platinum_core::drafts::sync::Snapshot,
        post_at: i64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Reserved,
    NativeCapture,
    Review,
    Prepared,
    NativeAdmission,
    Consume,
    NativeClear,
    Dispatch,
    Http,
    Storage,
    Publication,
    Release,
    StartLocal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Running,
    ConsumedAwaitingDelivery,
    AwaitingRecovery,
    Finished,
    Failed(durable::Error),
    Unknown,
}
pub(crate) struct Consumed {
    pub before: durable::Review,
    pub after: durable::Review,
    pub serial: u64,
    pub native_clear: NativeClear,
}
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Publications {
    pub initial_inventory: Option<bool>,
    pub pending_send: Option<bool>,
    /// True means the existing pure outcome helper was invoked under its owner
    /// guard, not an independently attested native paint.
    pub send_helper_invoked: Option<bool>,
    pub inventory: Option<bool>,
    pub queue: Option<bool>,
    pub after_release_inventory: Option<bool>,
}
pub(crate) struct Evidence {
    pub phase: Phase,
    pub status: Status,
    pub review: Option<Arc<durable::Review>>,
    pub consumed: Option<Arc<Consumed>>,
    pub attempt: Option<Arc<Attempt>>,
    pub receipt: Option<durable::Receipt>,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub owned_send: Option<Arc<super::owned_send::Send>>,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub owned_release: Option<Arc<super::owned_release::Release>>,
    pub recorded: Option<Arc<durable::Review>>,
    pub preservation: Option<Result<(), durable::Error>>,
    pub publication: Publications,
    pub release: Option<durable::Release>,
    pub local_lease: Option<durable::LocalWriterLease>,
    pub handoff: Option<Result<(), durable::Error>>,
    pub finished: Option<Arc<delivery::Finished>>,
    pub native: Option<Arc<crate::view::composer::protected_projection::NativeSnapshot>>,
    pub preservation_input:
        Option<Arc<crate::view::composer::protected_projection::NativeSnapshot>>,
}
pub(crate) struct Record {
    pub id: u64,
    pub scope: Scope,
    pub request: Request,
    pub local: Arc<LocalDraft>,
    pub book: Option<Arc<LocalDraft>>,
    pub serial: u64,
    pub epoch: u64,
    account: Weak<Account>,
    editor: Weak<editor::Editor>,
    live: AtomicBool,
    pub evidence: Mutex<Evidence>,
    changed: tokio::sync::Notify,
}
#[derive(Clone)]
pub(crate) struct Operation(pub(super) Arc<Record>);
#[derive(Default)]
pub(super) struct Records {
    next: u64,
    entries: BTreeMap<u64, Arc<Record>>,
}
impl Records {
    pub(super) fn ownership_entries(&self) -> Vec<Operation> {
        self.entries.values().cloned().map(Operation).collect()
    }
}
pub(crate) struct Seed {
    pub request: Request,
    pub local: Arc<LocalDraft>,
    pub book: Option<Arc<LocalDraft>>,
    pub serial: u64,
    pub epoch: u64,
}
pub(crate) struct Refused {
    pub error: durable::Error,
    pub seed: Seed,
}
impl Account {
    pub(crate) fn reserve_operation(
        self: &Arc<Self>,
        editor: &Arc<editor::Editor>,
        seed: Seed,
    ) -> Result<Operation, Refused> {
        if !Arc::ptr_eq(self, &editor.owner.account) {
            return Err(Refused {
                error: durable::Error::Stale,
                seed,
            });
        }
        let mut records = match self.operations.lock() {
            Ok(records) => records,
            Err(_) => {
                return Err(Refused {
                    error: durable::Error::Storage,
                    seed,
                });
            }
        };
        if records.entries.len() >= MAX_OPERATIONS {
            return Err(Refused {
                error: durable::Error::Pending,
                seed,
            });
        }
        let Some(id) = records.next.checked_add(1) else {
            return Err(Refused {
                error: durable::Error::Unsupported,
                seed,
            });
        };
        if records.entries.contains_key(&id) {
            return Err(Refused {
                error: durable::Error::Unsupported,
                seed,
            });
        }
        let record = Arc::new(Record {
            id,
            scope: editor.scope().clone(),
            request: seed.request,
            local: seed.local,
            book: seed.book,
            serial: seed.serial,
            epoch: seed.epoch,
            account: Arc::downgrade(self),
            editor: Arc::downgrade(editor),
            live: AtomicBool::new(true),
            evidence: Mutex::new(Evidence {
                phase: Phase::Reserved,
                status: Status::Running,
                review: None,
                consumed: None,
                attempt: None,
                receipt: None,
                #[cfg(any(test, feature = "protected-native-fixture"))]
                owned_send: None,
                #[cfg(any(test, feature = "protected-native-fixture"))]
                owned_release: None,
                recorded: None,
                preservation: None,
                publication: Publications::default(),
                release: None,
                local_lease: None,
                handoff: None,
                finished: None,
                native: None,
                preservation_input: None,
            }),
            changed: Default::default(),
        });
        records.next = id;
        records.entries.insert(id, record.clone());
        Ok(Operation(record))
    }
    fn acknowledge_operation(&self, operation: &Operation) -> Result<(), durable::Error> {
        let mut records = self
            .operations
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        let record = records
            .entries
            .get(&operation.id())
            .ok_or(durable::Error::Stale)?;
        if !Arc::ptr_eq(record, &operation.0) {
            return Err(durable::Error::Stale);
        }
        if record
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .status
            != Status::Finished
        {
            return Err(durable::Error::Pending);
        }
        records.entries.remove(&operation.id());
        Ok(())
    }
    pub(crate) fn retained_operations(&self) -> Result<Vec<Operation>, durable::Error> {
        let records = self
            .operations
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        for record in records.entries.values() {
            drop(
                record
                    .evidence
                    .lock()
                    .map_err(|_| durable::Error::Storage)?,
            );
        }
        Ok(records.entries.values().cloned().map(Operation).collect())
    }
}
impl Operation {
    pub fn reserve(editor: &Arc<editor::Editor>, seed: Seed) -> Result<Self, Refused> {
        editor.owner.account.reserve_operation(editor, seed)
    }
    pub fn review(&self, review: &durable::Review) -> Result<(), durable::Error> {
        self.retain(|s| s.review = Some(Arc::new(review.clone())))
    }
    pub fn native(
        &self,
        value: &crate::view::composer::protected_projection::NativeSnapshot,
        changed: bool,
    ) -> Result<(), durable::Error> {
        self.retain(|s| {
            if changed {
                s.preservation_input = Some(Arc::new(value.clone()));
            } else {
                s.native = Some(Arc::new(value.clone()));
            }
        })
    }
    pub fn preservation(&self, result: Result<(), durable::Error>) -> Result<(), durable::Error> {
        self.retain(|s| s.preservation = Some(result))?;
        result
    }
    pub fn native_cleared(&self) -> Result<bool, durable::Error> {
        Ok(self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .consumed
            .as_ref()
            .is_some_and(|value| value.native_clear == NativeClear::Cleared))
    }
    pub fn delivered(&self, value: Arc<delivery::Finished>) -> Result<Status, durable::Error> {
        let settled = self.native_cleared()?
            && value.local_handoff.is_ok()
            && value.inventory_published
            && match &value.dispatched.attempt.outcome {
                crate::drafts::dispatch::Outcome::Sent { .. } => true,
                crate::drafts::dispatch::Outcome::Scheduled { .. } => value.queue_published,
                _ => false,
            };
        self.retain(|s| {
            s.handoff = Some(value.local_handoff);
            s.finished = Some(value);
        })?;
        Ok(if settled {
            Status::Finished
        } else {
            Status::AwaitingRecovery
        })
    }
    pub(crate) fn acknowledge(&self) -> Result<(), durable::Error> {
        self.0
            .account
            .upgrade()
            .ok_or(durable::Error::Stale)?
            .acknowledge_operation(self)
    }
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn record(&self) -> &Record {
        &self.0
    }
    pub fn current(&self) -> bool {
        self.0.live.load(Ordering::Acquire)
    }
    pub fn belongs(&self, editor: &Arc<editor::Editor>) -> bool {
        Weak::ptr_eq(&self.0.editor, &Arc::downgrade(editor))
            && Weak::ptr_eq(&self.0.account, &Arc::downgrade(&editor.owner.account))
    }
    pub fn belongs_owner(&self, owner: &Owner) -> bool {
        self.0.scope == *owner.scope()
            && Weak::ptr_eq(&self.0.account, &Arc::downgrade(&owner.account))
    }
    pub fn permits(&self, action: &Action) -> bool {
        match (&self.0.request, action) {
            (
                Request::Send,
                Action::Consume(super_platinum_core::drafts::sync::ConsumeKind::Send),
            ) => true,
            (
                Request::Schedule {
                    snapshot: a,
                    post_at: x,
                },
                Action::Schedule {
                    snapshot: b,
                    post_at: y,
                },
            ) => a == b && x == y,
            _ => false,
        }
    }
    pub fn phase(&self, phase: Phase) -> Result<(), durable::Error> {
        if !self.current() {
            return Err(durable::Error::Stale);
        }
        let mut state = self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if state.status != Status::Running {
            return Err(durable::Error::Stale);
        }
        state.phase = phase;
        Ok(())
    }
    /// Evidence may land after retirement; authority and terminal Unknown never revive.
    pub fn retain(&self, update: impl FnOnce(&mut Evidence)) -> Result<(), durable::Error> {
        let mut state = self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        update(&mut state);
        Ok(())
    }
    pub fn consumed(&self, value: &Applied) -> Result<(), durable::Error> {
        if value.barrier.is_some() || value.after.scope() != &self.0.scope {
            return Err(durable::Error::Stale);
        }
        self.retain(|s| {
            s.consumed = Some(Arc::new(Consumed {
                before: value.before.clone(),
                after: value.after.clone(),
                serial: value.serial,
                native_clear: value.native_clear,
            }))
        })
    }
    pub fn finish(&self, result: Result<Status, durable::Error>) {
        if let Ok(mut state) = self.0.evidence.lock() {
            if state.status == Status::Running && self.current() {
                state.status = result.unwrap_or_else(Status::Failed);
            }
        }
        self.0.changed.notify_waiters();
    }
    pub fn unknown(&self) {
        self.0.live.store(false, Ordering::Release);
        if let Ok(mut state) = self.0.evidence.lock() {
            if state.status == Status::Running {
                state.status = Status::Unknown;
            }
        }
        self.0.changed.notify_waiters();
    }
    pub async fn ready(&self) -> Result<Status, durable::Error> {
        loop {
            let changed = self.0.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let status = self
                .0
                .evidence
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .status;
            if status != Status::Running {
                return Ok(status);
            }
            changed.await;
        }
    }
}

#[cfg(test)]
#[path = "operation/tests.rs"]
pub(crate) mod tests;
