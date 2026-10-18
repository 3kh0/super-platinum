//! One actual Account load, independent of the lifetime of its UI waiters.
//!
//! The blocking closure owns only synchronized storage custody and Weak<Account>.
//! It never captures Signal, renderer futures or protected HTTP Evidence. A
//! received error is terminal: AfterRestart can revoke before an open fails.
use super::super::ownership::{self, Tracked};
use super::{Account, durable};
use std::sync::{Arc, Weak, atomic::Ordering};

pub(super) struct Holder {
    state: Tracked<State>,
    changed: tokio::sync::watch::Sender<()>,
    #[cfg(test)]
    gate: std::sync::Mutex<Option<tests::WaitGate>>,
}
struct State {
    status: Status,
    reply: Option<durable::Reply<durable::Loaded>>,
    task: Option<tokio::task::JoinHandle<()>>,
    outcome: Option<Result<Arc<durable::Loaded>, durable::Error>>,
    supplied: Option<durable::owned_store::Initialized>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Unstarted,
    MissingRuntime,
    Waiting,
    // Actual Reply::wait outcome. That API maps sender disconnect to Storage;
    // this status cannot distinguish a disconnected sender from a disk error.
    Received,
    ExecutorLost,
    Supplied,
}
impl Holder {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Tracked::new(State {
                status: Status::Unstarted,
                reply: None,
                task: None,
                outcome: None,
                supplied: None,
            }),
            changed: tokio::sync::watch::channel(()).0,
            #[cfg(test)]
            gate: std::sync::Mutex::default(),
        })
    }
    pub(super) fn supplied(value: durable::owned_store::Initialized) -> Arc<Self> {
        let holder = Self::new();
        {
            let mut state = holder.state.lock().unwrap();
            state.supplied = Some(value);
            state.status = Status::Supplied;
        }
        holder
    }
    fn begin(self: &Arc<Self>, account: &Arc<Account>) -> Result<(), durable::Error> {
        let runtime = {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if !matches!(state.status, Status::Unstarted | Status::MissingRuntime) {
                return Ok(());
            }
            let runtime = match tokio::runtime::Handle::try_current() {
                Ok(runtime) => runtime,
                Err(_) => {
                    if state.status != Status::MissingRuntime {
                        state.status = Status::MissingRuntime;
                        self.changed.send_replace(());
                    }
                    return Err(durable::Error::Storage);
                }
            };
            state.reply = Some(account.client.load(durable::LoadMode::AfterRestart));
            state.status = Status::Waiting;
            runtime
        };
        // Construct before spawn: abort of a queued, never-polled blocking task
        // must also publish interruption. Do not spawn/drop a closure while the
        // state lock is held: its destructor takes this same lock.
        let guard = Completion {
            holder: self.clone(),
            account: Arc::downgrade(account),
        };
        let task = runtime.spawn_blocking(move || {
            #[cfg(test)]
            guard.holder.wait_gate();
            let reply = match guard.holder.state.lock() {
                Ok(mut state) => state.reply.take(),
                Err(_) => None,
            };
            if let Some(reply) = reply {
                let outcome = reply.wait().map(Arc::new);
                guard.complete(outcome);
            }
        });
        self.state.lock().map_err(|_| durable::Error::Storage)?.task = Some(task);
        Ok(())
    }
    pub(super) async fn initialize(
        self: &Arc<Self>,
        account: &Arc<Account>,
    ) -> Result<(), durable::Error> {
        let mut changed = self.changed.subscribe();
        self.begin(account)?;
        loop {
            {
                let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
                match state.status {
                    Status::Supplied => return Ok(()),
                    Status::Received => {
                        return state
                            .outcome
                            .as_ref()
                            .ok_or(durable::Error::Storage)?
                            .as_ref()
                            .map(|_| ())
                            .map_err(Clone::clone);
                    }
                    Status::ExecutorLost => return Err(durable::Error::Storage),
                    _ => {}
                }
            }
            changed
                .changed()
                .await
                .map_err(|_| durable::Error::Storage)?;
        }
    }
    pub(super) fn observe(
        self: &Arc<Self>,
        account: &Arc<Account>,
    ) -> Result<Observation, ownership::Error> {
        let (stamp, (status, outcome)) = self
            .state
            .observe(|state| (state.status, state.outcome.clone()))?;
        let observation = Observation {
            holder: self.clone(),
            stamp,
            status,
            _outcome: outcome,
            ready: account.ready.load(Ordering::Acquire),
        };
        observation.validate(account)?;
        Ok(observation)
    }
}
struct Completion {
    holder: Arc<Holder>,
    account: Weak<Account>,
}
impl Completion {
    fn complete(&self, outcome: Result<Arc<durable::Loaded>, durable::Error>) {
        // Preserve the actual reply even if a previous task poisoned the lock;
        // poison never permits readiness or a valid component observation.
        let (mut state, poisoned) = match self.holder.state.lock() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        if state.outcome.is_some() {
            return;
        }
        let success = outcome.is_ok();
        state.outcome = Some(outcome);
        state.status = if poisoned {
            Status::ExecutorLost
        } else {
            Status::Received
        };
        if success && !poisoned {
            if let Some(account) = self.account.upgrade() {
                if Arc::ptr_eq(&account.initialization, &self.holder) {
                    account.ready.store(true, Ordering::Release);
                }
            }
        }
        drop(state);
        // Both exact result and readiness have been published before wakeup.
        self.holder.changed.send_replace(());
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        let mut state = self
            .holder
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.outcome.is_none() && state.status == Status::Waiting {
            state.status = Status::ExecutorLost;
        }
        drop(state);
        self.holder.changed.send_replace(());
    }
}
/// Historical custody only. Retains the exact holder (including supplied Owned
/// provenance), not permission to normalize, write, drain, close or revive it.
pub(super) struct Observation {
    holder: Arc<Holder>,
    stamp: ownership::Stamp,
    pub(super) status: Status,
    _outcome: Option<Result<Arc<durable::Loaded>, durable::Error>>,
    ready: bool,
}
impl Observation {
    pub(super) fn validate(&self, account: &Arc<Account>) -> Result<(), ownership::Error> {
        if !Arc::ptr_eq(&self.holder, &account.initialization) {
            return Err(ownership::Error::Replaced);
        }
        self.holder.state.validate(&self.stamp)?;
        if account.ready.load(Ordering::Acquire) != self.ready {
            return Err(ownership::Error::Changed);
        }
        self.holder.state.validate(&self.stamp)
    }
}

#[cfg(test)]
mod tests;
