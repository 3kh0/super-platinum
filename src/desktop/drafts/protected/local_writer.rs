//! One ordered local continuation. Cloning the lane cannot reset its serial.
use super::ownership::Tracked as Mutex;
use super::*;
use std::collections::VecDeque;
#[path = "owned_local.rs"]
mod owned;

pub(crate) struct LocalWriter {
    account: Arc<Account>,
    lease: durable::LocalWriterLease,
    runtime: tokio::runtime::Handle,
    state: Mutex<State>,
    changed: tokio::sync::Notify,
}
struct State {
    retired: bool,
    serial: u64,
    tail: LocalDraft,
    pending: VecDeque<Submission>,
    running: bool,
    latest_input: Option<(u64, LocalDraft)>,
    inventory: Option<durable::LocalInventory>,
    error: Option<durable::Error>,
    retries: BTreeMap<u64, Command>,
    unsent_latest: Option<LocalDraft>,
}
struct Submission {
    command: Command,
    reply: owned::Reply,
}
#[derive(Clone)]
struct Command {
    owned: Option<
        Arc<
            super::owned_route::Held<
                super::owned_route::o::LocalWriteInput,
                durable::LocalWriteResult,
            >,
        >,
    >,
    predecessor: u64,
    serial: u64,
    expected: LocalDraft,
    local: LocalDraft,
}
#[derive(Clone)]
pub(crate) struct LocalSnapshot {
    pub serial: u64,
    pub pending: Option<LocalDraft>,
    pub inventory: Option<durable::LocalInventory>,
    pub error: Option<durable::Error>,
    pub running: bool,
    pub failed_inputs: Vec<(u64, LocalDraft)>,
}
impl LocalWriter {
    pub(super) fn matches_lease(&self, lease: &durable::LocalWriterLease) -> bool {
        self.lease.scope() == lease.scope()
            && self.lease.writer_id() == lease.writer_id()
            && self.lease.generation() == lease.generation()
            && self.lease.initial_serial() == lease.initial_serial()
    }
    /// Desktop holder membership only. Core job progression is a separate edge.
    pub(super) fn ownership_stamp(
        &self,
    ) -> Result<super::ownership::Stamp, super::ownership::Error> {
        self.state.observe(|_| ()).map(|(stamp, ())| stamp)
    }
    pub(super) fn validate_ownership(
        &self,
        stamp: &super::ownership::Stamp,
    ) -> Result<(), super::ownership::Error> {
        self.state.validate(stamp)
    }

    /// Called only after the one-use start_local receipt while the Editor gate
    /// remains held. The paired canonical value has not changed at handoff.
    pub(super) fn from_handoff(
        account: Arc<Account>,
        lease: durable::LocalWriterLease,
        local: LocalDraft,
    ) -> Result<Arc<Self>, durable::Error> {
        if lease.scope().account_id != account.id() {
            return Err(durable::Error::Stale);
        }
        let serial = lease.initial_serial();
        Ok(Arc::new(Self {
            account,
            lease,
            runtime: tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?,
            state: Mutex::new(State {
                retired: false,
                serial,
                tail: local,
                pending: VecDeque::new(),
                running: false,
                latest_input: None,
                inventory: None,
                error: None,
                retries: BTreeMap::new(),
                unsent_latest: None,
            }),
            changed: tokio::sync::Notify::new(),
        }))
    }
    /// Fresh exact local inventory is required after restart/lost start. There
    /// is deliberately no constructor taking an old cached Release or a number.
    pub(super) async fn renew(
        account: Arc<Account>,
        inventory: durable::LocalInventory,
    ) -> Result<Arc<Self>, durable::Error> {
        if inventory.scope().account_id != account.id()
            || inventory.row().is_some()
            || inventory.uncommitted().is_some()
            || inventory.blocked().is_some()
            || !inventory.alternates().is_empty()
        {
            return Err(durable::Error::ReviewRequired);
        }
        account.initialize().await?;
        let local = inventory.local().clone();
        let lease = match &account.owned {
            Some(route) => {
                super::owned_route::Held::new(route.operations.renew_local(inventory.clone()))
                    .observed_success()
                    .await?
            }
            None => super::account::wait(account.client()?.renew_local(inventory.clone())).await?,
        };
        let writer = Self::from_handoff(account, lease, local)?;
        // Renew advances the durable generation. The pre-renew proof is never
        // published as if it described the newly installed lease.
        writer.refresh_inventory().await?;
        Ok(writer)
    }
    pub(super) async fn refresh_inventory(&self) -> Result<(), durable::Error> {
        let inventory = self.current_inventory().await?;
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if inventory.scope() != self.lease.scope()
            || inventory.row().is_some()
            || !same_local(inventory.local(), &state.tail)
        {
            state.error = Some(durable::Error::ReviewRequired);
            return Err(durable::Error::ReviewRequired);
        }
        state.inventory = Some(inventory);
        Ok(())
    }
    /// Enqueue under the same memory lock as advancing the predecessor. No
    /// executor scheduling, renderer tick or await can reorder two submissions.
    pub fn submit(self: &Arc<Self>, local: &LocalDraft) -> Result<u64, durable::Error> {
        retention::input_size(local)?;
        let (serial, start) = {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.retired {
                return Err(durable::Error::Stale);
            }
            // A bounded FIFO retains every admitted command. Further native
            // typing coalesces only in one *unadmitted* latest slot. A failed
            // predecessor keeps it there until explicit repair/review.
            if state.pending.len() + usize::from(state.running) >= 16 || !state.retries.is_empty() {
                state.unsent_latest = Some(local.clone());
                return Err(durable::Error::Pending);
            }
            let predecessor = state.serial;
            let serial = predecessor
                .checked_add(1)
                .ok_or(durable::Error::Unsupported)?;
            let mut command = Command {
                owned: None,
                predecessor,
                serial,
                expected: state.tail.clone(),
                local: local.clone(),
            };
            let reply = match self.enqueue(&mut command, false) {
                Ok(reply) => reply,
                Err(error) => {
                    if self.account.owned.is_some() {
                        state.unsent_latest = Some(local.clone());
                        state.retries.insert(serial, command);
                        state.error = Some(error);
                    }
                    return Err(error);
                }
            };
            state.serial = serial;
            state.tail = local.clone();
            state.latest_input = Some((serial, local.clone()));
            state.pending.push_back(Submission { command, reply });
            let start = !state.running;
            state.running = true;
            (serial, start)
        };
        if start {
            let writer = self.clone();
            self.runtime.spawn(async move {
                writer.drain().await;
            });
        }
        Ok(serial)
    }
    async fn drain(self: Arc<Self>) {
        loop {
            let next = {
                let mut state = self.state.lock().unwrap();
                match state.pending.pop_front() {
                    Some(next) => Some(next),
                    None => {
                        if state.error.is_none() && state.retries.is_empty() {
                            if let Some(local) = state.unsent_latest.take() {
                                let predecessor = state.serial;
                                if let Some(serial) = predecessor.checked_add(1) {
                                    let mut command = Command {
                                        owned: None,
                                        predecessor,
                                        serial,
                                        expected: state.tail.clone(),
                                        local: local.clone(),
                                    };
                                    match self.enqueue(&mut command, false) {
                                        Ok(reply) => {
                                            state.serial = serial;
                                            state.tail = local.clone();
                                            state.latest_input = Some((serial, local));
                                            Some(Submission { command, reply })
                                        }
                                        Err(error) => {
                                            if self.account.owned.is_some() {
                                                state.retries.insert(serial, command);
                                            }
                                            state.unsent_latest = Some(local);
                                            state.error = Some(error);
                                            state.running = false;
                                            None
                                        }
                                    }
                                } else {
                                    state.unsent_latest = Some(local);
                                    state.error = Some(durable::Error::Unsupported);
                                    state.running = false;
                                    None
                                }
                            } else {
                                state.running = false;
                                None
                            }
                        } else {
                            state.running = false;
                            None
                        }
                    }
                }
            };
            let Some(next) = next else {
                self.changed.notify_waiters();
                return;
            };
            let command = next.command;
            let result = owned::wait(next.reply).await;
            let current = if matches!(&result, Ok(durable::LocalWriteResult::Applied(_))) {
                Some(self.current_inventory().await)
            } else {
                None
            };
            let mut state = self.state.lock().unwrap();
            match result {
                Ok(durable::LocalWriteResult::Applied(inventory))
                    if inventory.scope() == self.lease.scope()
                        && same_local(inventory.local(), &command.local)
                        && current.as_ref().is_some_and(|current| {
                            current
                                .as_ref()
                                .is_ok_and(|current| current.same(&inventory))
                        }) =>
                {
                    state.inventory = Some(inventory);
                    state.error = None;
                    if let Some(held) = &command.owned {
                        let acknowledgment = held.job().and_then(|job| job.observe_local_write());
                        match acknowledgment {
                            Ok((_, acknowledgment)) => {
                                if let Err((error, _)) = acknowledgment.acknowledge() {
                                    state.error = Some(super::owned_route::ui(error));
                                }
                            }
                            Err(_) => {
                                // Storage retry history cannot use the clean
                                // receipt. Settle only this exact applied job;
                                // no other failure or preserved input qualifies.
                                let settled = held
                                    .job()
                                    .and_then(|job| job.observe_local_write_settlement());
                                match settled {
                                    Ok((observed, _history, acknowledgment))
                                        if state
                                            .inventory
                                            .as_ref()
                                            .is_some_and(|v| v.same(&observed)) =>
                                    {
                                        if let Err((error, _)) = acknowledgment.acknowledge() {
                                            state.error = Some(super::owned_route::ui(error));
                                        }
                                    }
                                    Ok(_) => state.error = Some(durable::Error::Stale),
                                    Err(error) => state.error = Some(super::owned_route::ui(error)),
                                }
                            }
                        }
                    }
                    if state.error.is_some() {
                        // A fence or accounting refusal must retain this exact
                        // applied job for explicit acknowledgment retry.
                        state.retries.insert(command.serial, command.clone());
                    } else {
                        state.retries.remove(&command.serial);
                        if state.latest_input.as_ref().is_some_and(|(serial, local)| {
                            *serial == command.serial && same_local(local, &command.local)
                        }) {
                            state.latest_input = None;
                        }
                    }
                }
                Ok(durable::LocalWriteResult::Preserved(inventory))
                    if inventory.scope() == self.lease.scope() =>
                {
                    state.inventory = Some(inventory);
                    state.error = Some(durable::Error::ReviewRequired);
                    state.retries.remove(&command.serial);
                }
                Ok(durable::LocalWriteResult::Retired) => {
                    // Retired is not proof that this exact input is canonical.
                    state.error = Some(durable::Error::ReviewRequired);
                }
                Ok(_) => state.error = Some(durable::Error::Stale),
                Err(error) => {
                    if error == durable::Error::Storage {
                        state.retries.insert(command.serial, command);
                    }
                    state.error = Some(error);
                }
            }
            drop(state);
            self.changed.notify_waiters();
        }
    }
    pub fn snapshot(&self) -> LocalSnapshot {
        let state = self.state.lock().unwrap();
        LocalSnapshot {
            serial: state.serial,
            pending: state
                .unsent_latest
                .clone()
                .or_else(|| state.latest_input.as_ref().map(|(_, local)| local.clone())),
            inventory: state.inventory.clone(),
            error: state.error,
            running: state.running,
            failed_inputs: state
                .retries
                .values()
                .map(|command| (command.serial, command.local.clone()))
                .collect(),
        }
    }
    /// Requeue only the immutable local command. Original generation,
    /// predecessor, expected bytes and serial are retained; no new input or
    /// remote permission is fabricated by a storage retry.
    pub fn retry_storage(self: &Arc<Self>, serial: u64) -> Result<(), durable::Error> {
        {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.retired {
                return Err(durable::Error::Stale);
            }
            if state.running {
                return Err(durable::Error::Pending);
            }
            let mut command = state
                .retries
                .get(&serial)
                .cloned()
                .ok_or(durable::Error::Stale)?;
            let reply = self.enqueue(&mut command, true)?;
            state.pending.push_back(Submission { command, reply });
            state.running = true;
        }
        let writer = self.clone();
        self.runtime.spawn(async move {
            writer.drain().await;
        });
        Ok(())
    }
    pub(super) fn retry_oldest_storage(self: &Arc<Self>) -> Result<(), durable::Error> {
        let serial = self
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .retries
            .keys()
            .next()
            .copied()
            .ok_or(durable::Error::Stale)?;
        self.retry_storage(serial)
    }
    /// Closed local-to-registered transition. Lock order matches existing
    /// Editor snapshots (Editor -> local) and native projection (Editor -> Registry).
    /// All fallible checks precede registry replacement; no await follows it.
    pub(super) fn install_registered(
        &self,
        serial: u64,
        expected: &durable::Review,
        registry: &registry::Registry,
        epoch: u64,
        previous: Option<&Arc<editor::Editor>>,
        next: &Arc<editor::Editor>,
    ) -> Result<(), durable::Error> {
        let managed = next.state.lock().map_err(|_| durable::Error::Storage)?;
        if managed.retired
            || managed.serial != 0
            || managed.running
            || managed.busy
            || managed.parked
            || managed.pending.is_some()
            || managed.error.is_some()
            || managed.retry.is_some()
            || managed.released.is_some()
            || managed.handoff_lost
            || managed.choice_retry.is_some()
            || managed.barrier.is_some()
            || !managed.review.same(expected)
        {
            return Err(durable::Error::Stale);
        }
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if self.lease.scope() != expected.scope()
            || next.scope() != expected.scope()
            || managed.writer.enrollment() != expected.row().lineage().enrollment()
            || state.retired
            || state.serial != serial
            || state.running
            || !state.pending.is_empty()
            || state.latest_input.is_some()
            || state.unsent_latest.is_some()
            || !state.retries.is_empty()
            || state.error.is_some()
            || !same_local(&state.tail, expected.local())
        {
            return Err(durable::Error::ReviewRequired);
        }
        registry.replace(
            &expected.scope().account_id,
            epoch,
            &expected.scope().key,
            previous,
            next.clone(),
        )?;
        state.retired = true;
        Ok(())
    }
    /// Local-only retirement. Enrolled/preserved lanes require a later reviewed
    /// handoff; registration alone is not authority to drop their input.
    pub(super) async fn retire_clean(&self) -> Result<(), durable::Error> {
        if self
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .retired
        {
            return Ok(());
        }
        let before = self.flush().await?;
        if before.pending.is_some() || !before.failed_inputs.is_empty() {
            return Err(durable::Error::ReviewRequired);
        }
        let expected = before.inventory.ok_or(durable::Error::Stale)?;
        let fresh = self.current_inventory().await?;
        if fresh.row().is_some() || !fresh.same(&expected) {
            return Err(durable::Error::Stale);
        }
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.serial != before.serial
            || state.running
            || !state.pending.is_empty()
            || !state.retries.is_empty()
            || state.latest_input.is_some()
            || state.unsent_latest.is_some()
            || state.error.is_some()
        {
            return Err(durable::Error::ReviewRequired);
        }
        state.retired = true;
        Ok(())
    }
    pub async fn flush(&self) -> Result<LocalSnapshot, durable::Error> {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let snapshot = self.snapshot();
            if !snapshot.running {
                return snapshot.error.map_or(Ok(snapshot), Err);
            }
            notified.await;
        }
    }
}
