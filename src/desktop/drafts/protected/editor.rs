//! Detached input persistence carrying immutable registered writer provenance.
use super::ownership::Tracked as Mutex;
use super::*;
use super_platinum_core::drafts::sync::ConsumeIntent;

pub(crate) struct Editor {
    pub(super) owner: Owner,
    pub(super) runtime: tokio::runtime::Handle,
    pub(super) gate: tokio::sync::Mutex<()>,
    pub(super) state: Mutex<State>,
    changed: tokio::sync::Notify,
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub(super) owned_write: Mutex<Option<Arc<super::owned_editor::Write>>>,
    pub(super) identity: Arc<()>,
    pub(super) retention_id: String,
}
pub(super) struct State {
    pub review: durable::Review,
    pub writer: durable::Writer,
    pub serial: u64,
    pub pending: Option<(u64, LocalDraft)>,
    pub running: bool,
    pub busy: bool,
    pub parked: bool,
    pub error: Option<durable::Error>,
    pub preserved: Option<durable::LocalInventory>,
    pub direct: Option<ConsumeIntent>,
    pub barrier: Option<Arc<super::actions::ProjectionBarrier>>,
    pub released: Option<Arc<super::local_writer::LocalWriter>>,
    pub handoff_lost: bool,
    pub handoff_expected: Option<durable::Review>,
    pub choice_retry: Option<Arc<super::baseline_choice::Retry>>,
    pub(super) retry: Option<(Command, bool)>,
    pub retired: bool,
}
#[derive(Clone)]
pub(super) struct Command {
    pub(super) writer: durable::Writer,
    pub(super) serial: u64,
    pub(super) expected: durable::Review,
    pub(super) local: LocalDraft,
    pub(super) direct: Option<ConsumeIntent>,
}
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub view: View,
    pub serial: u64,
    pub writer_id: String,
    pub enrollment: String,
    pub pending: Option<LocalDraft>,
    pub running: bool,
    pub parked: bool,
    pub error: Option<durable::Error>,
    pub projection: Option<Arc<super::actions::ProjectionBarrier>>,
    pub failed_inputs: Vec<LocalDraft>,
    pub choice_retry_available: bool,
}
impl Editor {
    pub fn scope(&self) -> &Scope {
        self.owner.scope()
    }
    pub async fn open(owner: Owner) -> Result<Arc<Self>, durable::Error> {
        Self::open_inner(owner, None).await
    }
    /// Exact displayed/enrolled proof. A newer row is never silently substituted.
    pub async fn open_expected(
        owner: Owner,
        expected: durable::Review,
    ) -> Result<Arc<Self>, durable::Error> {
        owner.check(&expected)?;
        Self::open_inner(owner, Some(expected)).await
    }
    async fn open_inner(
        owner: Owner,
        expected: Option<durable::Review>,
    ) -> Result<Arc<Self>, durable::Error> {
        let opened = super::editor_open::Request { owner, expected }
            .begin()
            .map_err(|refused| refused.error)?;
        opened.ready().await;
        opened.result()
    }
    pub(super) fn registered(
        owner: Owner,
        review: durable::Review,
        writer: durable::Writer,
        runtime: tokio::runtime::Handle,
        retention_id: String,
    ) -> Arc<Self> {
        let serial = writer.initial_serial();
        let direct = review.row().state().local_preservation_intent().cloned();
        Arc::new(Self {
            owner,
            runtime,
            gate: tokio::sync::Mutex::new(()),
            state: Mutex::new(State {
                review,
                writer,
                serial,
                pending: None,
                running: false,
                busy: false,
                parked: false,
                error: None,
                preserved: None,
                direct,
                barrier: None,
                released: None,
                handoff_lost: false,
                handoff_expected: None,
                choice_retry: None,
                retry: None,
                retired: false,
            }),
            changed: tokio::sync::Notify::new(),
            #[cfg(any(test, feature = "protected-native-fixture"))]
            owned_write: Mutex::default(),
            identity: Arc::new(()),
            retention_id,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap();
        if let Some(writer) = &state.released {
            let local = writer.snapshot();
            return Snapshot {
                view: View::new(
                    local
                        .inventory
                        .map(Inventory::Local)
                        .unwrap_or_else(|| Inventory::Managed(state.review.clone())),
                    local.pending.clone(),
                ),
                serial: state.serial,
                writer_id: state.writer.id().to_owned(),
                enrollment: state.writer.enrollment().to_owned(),
                pending: local.pending,
                running: local.running || state.busy,
                parked: local.error.is_some(),
                error: local.error.or(state.error),
                projection: None,
                failed_inputs: local
                    .failed_inputs
                    .into_iter()
                    .map(|(_, local)| local)
                    .collect(),
                choice_retry_available: false,
            };
        }
        let inventory = state
            .preserved
            .clone()
            .map(Inventory::Local)
            .unwrap_or_else(|| Inventory::Managed(state.review.clone()));
        Snapshot {
            view: View::new(
                inventory,
                state.pending.as_ref().map(|(_, local)| local.clone()),
            ),
            serial: state.serial,
            writer_id: state.writer.id().to_owned(),
            enrollment: state.writer.enrollment().to_owned(),
            pending: state.pending.as_ref().map(|(_, local)| local.clone()),
            running: state.running || state.busy,
            parked: state.parked,
            error: state.error,
            projection: state.barrier.clone(),
            failed_inputs: state
                .retry
                .as_ref()
                .map(|(command, _)| command.local.clone())
                .into_iter()
                .collect(),
            choice_retry_available: state.choice_retry.is_some(),
        }
    }
    /// Borrowed admission leaves an excessive/refused candidate with its caller.
    /// No actor serial or retained bytes change until the size check succeeds.
    pub fn submit(self: &Arc<Self>, local: &LocalDraft) -> Result<u64, durable::Error> {
        retention::input_size(local)?;
        let (serial, start) = {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.retired {
                return Err(durable::Error::Stale);
            }
            state.serial = state
                .serial
                .checked_add(1)
                .ok_or(durable::Error::Unsupported)?;
            let serial = state.serial;
            if let Some(writer) = &state.released {
                writer.submit(local)?;
                return Ok(serial);
            }
            state.pending = Some((serial, local.clone()));
            // A failed immutable command must be repaired explicitly. Newer
            // input stays owned in memory and is never relabeled as its retry.
            let start = !state.running && state.retry.is_none() && !state.handoff_lost;
            if start {
                state.running = true;
            }
            (serial, start)
        };
        if start {
            self.start();
        }
        Ok(serial)
    }
    fn start(self: &Arc<Self>) {
        let editor = self.clone();
        self.runtime.spawn(async move {
            editor.run().await;
        });
    }
    async fn run(self: Arc<Self>) {
        let _gate = self.gate.lock().await;
        loop {
            let next = {
                let mut state = self.state.lock().unwrap();
                if let Some((command, preserve)) = state.retry.take() {
                    Some((command, preserve, true))
                } else if let Some((serial, local)) = state.pending.clone() {
                    Some((
                        Command {
                            writer: state.writer.clone(),
                            serial,
                            expected: state.review.clone(),
                            local,
                            direct: state.direct.clone(),
                        },
                        state.parked || state.error.is_some(),
                        false,
                    ))
                } else {
                    state.running = false;
                    None
                }
            };
            let Some((mut command, mut preserve, retry)) = next else {
                break;
            };
            #[cfg(any(test, feature = "protected-native-fixture"))]
            let owned = if let Some(route) = &self.owner.account.owned {
                let held = {
                    let mut slot = self.owned_write.lock().unwrap();
                    if !retry {
                        *slot = Some(super::owned_editor::Write::new(command.clone()));
                    }
                    slot.clone()
                };
                let result = match held {
                    Some(held) => {
                        held.run(route, &self.owner, &mut command, &mut preserve, retry)
                            .await
                    }
                    None => Err(super::owned_route::o::AdmissionError::Incomplete),
                };
                match result {
                    Ok(value) => Some(value),
                    Err(_) => {
                        // Retain exact Command and Held (including returned
                        // refusal input). Never run the durable Preserve path.
                        let mut state = self.state.lock().unwrap();
                        state.retry = Some((command, preserve));
                        state.error = Some(durable::Error::Unsupported);
                        state.running = false;
                        break;
                    }
                }
            } else {
                None
            };
            #[cfg(not(any(test, feature = "protected-native-fixture")))]
            let owned: Option<Result<durable::WriteResult, durable::Error>> = None;
            let result = if let Some(result) = owned {
                result
            } else {
                let mut result = if preserve {
                    self.preserve_command(&command).await
                } else if retry {
                    self.checkpoint_command(&command).await
                } else {
                    // Rebase only before a fresh serial's first attempt. A storage
                    // retry retains its exact original expected proof and payload.
                    let fresh = self.owner.review().await;
                    match fresh {
                        Ok(review)
                            if editor_proof::can_rebase(
                                &command.expected,
                                &review,
                                &command.writer,
                            ) =>
                        {
                            command.expected = review;
                            self.checkpoint_command(&command).await
                        }
                        Ok(_) => {
                            preserve = true;
                            self.preserve_command(&command).await
                        }
                        Err(error) => Err(error),
                    }
                };
                if !preserve
                    && result
                        .as_ref()
                        .is_err_and(|error| *error != durable::Error::Storage)
                {
                    // Recognized precondition failure leaves this serial unused.
                    // Preserve this exact input immediately, not on another keystroke.
                    preserve = true;
                    result = self.preserve_command(&command).await;
                }

                result
            };
            match result {
                Ok(durable::WriteResult::Applied(review)) => {
                    if review.scope() != self.owner.scope()
                        || review.row().lineage().enrollment() != command.writer.enrollment()
                        || !same_local(review.local(), &command.local)
                    {
                        self.stop(durable::Error::Stale);
                        break;
                    }
                    // Equal-serial disk retries return the original result,
                    // even if another writer changed this row afterward. That
                    // receipt proves persistence, not current canonical input.
                    match self.owner.review().await {
                        Ok(current) if current.same(&review) => {}
                        Ok(_) => {
                            let inventory = self.owner.local_inventory().await;
                            let mut state = self.state.lock().unwrap();
                            state.preserved = inventory.ok();
                            state.error = Some(durable::Error::ReviewRequired);
                            state.parked = true;
                            state.running = false;
                            break;
                        }
                        Err(error) => {
                            let mut state = self.state.lock().unwrap();
                            state.retry = Some((command, preserve));
                            state.error = Some(error);
                            state.running = false;
                            break;
                        }
                    }
                    let mut state = self.state.lock().unwrap();
                    state.review = review;
                    state.preserved = None;
                    state.error = None;
                    if state
                        .pending
                        .as_ref()
                        .is_some_and(|(id, _)| *id == command.serial)
                    {
                        state.pending = None;
                    }
                }
                Ok(durable::WriteResult::Preserved(inventory)) => {
                    let mut state = self.state.lock().unwrap();
                    state.preserved = Some(inventory);
                    state.error = Some(durable::Error::ReviewRequired);
                    state.parked = true;
                    if state
                        .pending
                        .as_ref()
                        .is_none_or(|(id, _)| *id == command.serial)
                    {
                        state.running = false;
                        break;
                    }
                }
                Ok(durable::WriteResult::Retired) => {
                    self.stop(durable::Error::ReviewRequired);
                    break;
                }
                Err(durable::Error::Storage) => {
                    let mut state = self.state.lock().unwrap();
                    // Preserve retries need their own closed command variant;
                    // never reinterpret them as a canonical checkpoint.
                    state.retry = Some((command, preserve));
                    state.error = Some(durable::Error::Storage);
                    state.running = false;
                    break;
                }
                Err(error) => {
                    // Typed precondition failures did not consume this serial.
                    // A future input remains preserved by its original Writer.
                    self.stop(error);
                    break;
                }
            }
            self.changed.notify_waiters();
        }
        self.changed.notify_waiters();
    }
    async fn preserve_command(
        &self,
        command: &Command,
    ) -> Result<durable::WriteResult, durable::Error> {
        account::wait(self.owner.preserve(
            command.writer.clone(),
            command.serial,
            command.local.clone(),
        )?)
        .await
    }
    async fn checkpoint_command(
        &self,
        command: &Command,
    ) -> Result<durable::WriteResult, durable::Error> {
        account::wait(self.owner.checkpoint(
            command.writer.clone(),
            command.serial,
            command.expected.clone(),
            command.local.clone(),
            command.direct.clone(),
        )?)
        .await
    }
    pub fn retry_storage(self: &Arc<Self>) -> Result<(), durable::Error> {
        {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.running || state.busy || state.retry.is_none() {
                return Err(durable::Error::Pending);
            }
            state.running = true;
        }
        self.start();
        Ok(())
    }
    fn stop(&self, error: durable::Error) {
        let mut state = self.state.lock().unwrap();
        state.error = Some(error);
        state.parked = true;
        state.running = false;
    }
    pub(super) fn notify(&self) {
        self.changed.notify_waiters();
    }
    pub async fn flush(&self) -> Result<Snapshot, durable::Error> {
        loop {
            let released = self.state.lock().unwrap().released.clone();
            if let Some(writer) = released {
                writer.flush().await?;
                return Ok(self.snapshot());
            }
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let snapshot = self.snapshot();
            if !snapshot.running {
                return snapshot.error.map_or_else(
                    || {
                        if snapshot.parked {
                            Err(durable::Error::ReviewRequired)
                        } else if snapshot.pending.is_some() {
                            Err(durable::Error::Pending)
                        } else {
                            Ok(snapshot)
                        }
                    },
                    Err,
                );
            }
            notified.await;
        }
    }
    pub(super) fn capture(&self, visit: Visit) -> Result<editor_proof::Capture, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.retired
            || state.running
            || state.busy
            || state.parked
            || state.retry.is_some()
            || state.error.is_some()
            || state.released.is_some()
            || state.handoff_lost
        {
            return Err(durable::Error::Pending);
        }
        Ok(editor_proof::Capture {
            identity: self.identity.clone(),
            writer: state.writer.clone(),
            serial: state.serial,
            review: state.review.clone(),
            pending: state.pending.as_ref().map(|(_, local)| local.clone()),
            visit,
            visible: state
                .pending
                .as_ref()
                .map(|(_, local)| local.clone())
                .unwrap_or_else(|| state.review.local().clone()),
        })
    }
}
