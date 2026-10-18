//! Bounded ordered ingress, retaining actual owned storage through projection acknowledgment.
use super::*;
use super::{editor::Editor, editor_proof::Capture, source::Source};
use crate::{drafts::observation::Prepared, state::ShellState};
use dioxus::prelude::{ReadableExt, Signal};
use std::{collections::VecDeque, sync::Mutex};
use super_platinum_core::slack::events::RawEvent;

mod delivery;
mod ingress;
pub(super) mod inventory;
mod owned;
pub(crate) use delivery::Delivery;

const MAX_PENDING: usize = 64;
pub(crate) struct ObservationQueue {
    source: Arc<Source>,
    account: Arc<Account>,
    state: Mutex<Queue>,
}
#[derive(Default)]
struct Queue {
    raw: Option<Arc<RawEvent>>,
    pending: VecDeque<Arc<Work>>,
    active: Option<Arc<Work>>,
    abandoned: Vec<Arc<Work>>,
    results: VecDeque<Arc<Completed>>,
    delivered: Vec<Arc<Completed>>,
    unknown: bool,
    running: bool,
    failed: bool,
}
struct Completed {
    work: Option<Arc<Work>>,
    result: Result<Observed, durable::Error>,
}
struct Work {
    owner: Owner,
    editor: Option<Arc<Editor>>,
    prepared: Prepared,
    storage: Mutex<owned::State>,
}
#[derive(Clone)]
pub(crate) struct Observed {
    pub before: durable::Review,
    pub after: durable::Review,
    pub barrier: Option<Arc<actions::ProjectionBarrier>>,
}
impl ObservationQueue {
    pub fn new(account: Arc<Account>, source: Arc<Source>) -> Arc<Self> {
        let queue = Arc::new(Self {
            account,
            source,
            state: Mutex::default(),
        });
        queue.source.inventory.register(&queue);
        queue
    }
    /// Admission remains tied to the original socket/frame and exact owner.
    /// Completed but unconsumed results count toward the bound too.
    pub fn enqueue(
        self: &Arc<Self>,
        state: Signal<ShellState>,
        key: DraftKey,
        editor: Option<Arc<Editor>>,
        raw: &RawEvent,
    ) -> Result<(), durable::Error> {
        let owner = self.account.owner(key.clone())?;
        if editor
            .as_ref()
            .is_some_and(|editor| editor.owner.scope() != owner.scope())
        {
            return Err(durable::Error::Stale);
        }
        let prepared = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            self.source.prepare(&shell, &key, raw)?
        };
        let start = self.admit(Arc::new(Work {
            owner,
            editor,
            prepared,
            storage: Mutex::default(),
        }))?;
        if start {
            let guard = inventory::Running {
                queue: self.clone(),
                finished: false,
            };
            dioxus::core::spawn_forever(async move {
                let mut guard = guard;
                guard.queue.clone().run(state).await;
                guard.finished = true;
            });
        }
        Ok(())
    }
    fn admit(&self, work: Arc<Work>) -> Result<bool, durable::Error> {
        if !self.source.inventory.same_account(&self.account)
            || !Arc::ptr_eq(&work.owner.account, &self.account)
            || work
                .editor
                .as_ref()
                .is_some_and(|editor| !Arc::ptr_eq(&editor.owner.account, &self.account))
        {
            return Err(durable::Error::Stale);
        }
        let mut queue = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if queue.failed {
            return Err(durable::Error::Gap);
        }
        self.source.inventory.clock.change();
        if queue.pending.len()
            + queue.results.len()
            + queue.delivered.len()
            + usize::from(queue.active.is_some())
            + queue.abandoned.len()
            + usize::from(queue.raw.is_some())
            >= MAX_PENDING
        {
            queue.failed = true;
            queue.abandon_pending();
            drop(queue);
            self.source.gap(durable::GapReason::QueueOverflow);
            return Err(durable::Error::Gap);
        }
        queue.pending.push_back(work);
        let start = !queue.running;
        queue.running = true;
        Ok(start)
    }
    fn next(&self) -> Option<Arc<Work>> {
        let mut queue = self.state.lock().unwrap();
        self.source.inventory.clock.change();
        if queue.failed {
            queue.abandon_pending();
        }
        let next = queue.pending.pop_front();
        queue.active = next.clone();
        if next.is_none() {
            queue.running = false;
        }
        next
    }
    fn finish(&self, result: Result<Observed, durable::Error>) {
        let mut queue = self.state.lock().unwrap();
        self.source.inventory.clock.change();
        queue.failed |= result.is_err();
        let work = queue.active.take();
        if work.is_none() {
            queue.unknown = true;
            queue.failed = true;
        }
        queue
            .results
            .push_back(Arc::new(Completed { work, result }));
    }
    pub fn take_result(&self) -> Option<Result<Observed, durable::Error>> {
        let mut queue = match self.state.lock() {
            Ok(queue) => queue,
            Err(_) => {
                self.source.inventory.clock.unknown();
                return None;
            }
        };
        if queue.results.is_empty() {
            return None;
        }
        self.source.inventory.clock.change();
        let completed = queue.results.pop_front()?;
        let result = completed.result.clone();
        // Taking a result is not proof that its projection was installed. Keep
        // the accepted input and result until a future typed handoff exists.
        queue.delivered.push(completed);
        Some(result)
    }
    async fn run(self: Arc<Self>, state: Signal<ShellState>) {
        loop {
            let next = self.next();
            let Some(work) = next else {
                self.account.retire_closed_ingress();
                return;
            };
            let result = self.process(state, &work).await;
            if let Err(error) = result.as_ref() {
                self.source.gap(if *error == durable::Error::Storage {
                    durable::GapReason::StorageFailure
                } else {
                    durable::GapReason::InterruptedAdmission
                });
            }
            self.finish(result);
            if self.account.owned.is_some() {
                let handled = match self.take_delivery() {
                    Some(delivery) => delivery.apply(state).await,
                    None => Err(durable::Error::Stale),
                };
                if let Err(error) = handled {
                    let mut queue = self.state.lock().unwrap();
                    self.source.inventory.clock.change();
                    queue.failed = true;
                    queue.abandon_pending();
                    drop(queue);
                    self.source.gap(if error == durable::Error::Storage {
                        durable::GapReason::StorageFailure
                    } else {
                        durable::GapReason::InterruptedAdmission
                    });
                }
            }
        }
    }
    async fn process(
        &self,
        state: Signal<ShellState>,
        work: &Work,
    ) -> Result<Observed, durable::Error> {
        let owner = work.owner.clone();
        let editor = work.editor.clone();
        let prepared = &work.prepared;
        let _active = journal::Active::begin(&owner)?;
        if prepared.scope() != owner.scope() {
            return Err(durable::Error::Stale);
        }
        if let Some(editor) = &editor {
            editor.flush().await?;
        }
        let _gate = match &editor {
            Some(editor) => Some(editor.gate.lock().await),
            None => None,
        };
        let captured = match &editor {
            Some(editor) => Some(editor.capture(Visit::new())?),
            None => None,
        };
        let before = owner.review().await?;
        if let Some(captured) = &captured {
            if !editor_proof::can_rebase(&captured.review, &before, &captured.writer) {
                return Err(durable::Error::Stale);
            }
        }
        let event = prepared
            .resolve(before.row().state())
            .map_err(|_| durable::Error::Unsupported)?;
        let Some(event) = event else {
            return Ok(Observed {
                after: before.clone(),
                before,
                barrier: None,
            });
        };
        let stream = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            self.source.stream(&shell)?
        };
        owner.check_stream(&stream)?;
        if let Some(editor) = &editor {
            let mut current = editor.state.lock().map_err(|_| durable::Error::Storage)?;
            let captured = captured.as_ref().unwrap();
            if current.serial != captured.serial
                || current.running
                || current.busy
                || !current.review.same(&captured.review)
                || current.pending.is_some()
            {
                return Err(durable::Error::Stale);
            }
            current.busy = true;
        }
        let result = if let Some(route) = &owner.account.owned {
            owned::run(self, work, state, route, stream, before.clone(), event)
                .await
                .map(owned::ResultValue::Owned)
        } else {
            async {
                let ticket = account::wait(owner.client()?.reserve_observation(
                    stream,
                    before.clone(),
                    event,
                ))
                .await?;
                if !state
                    .try_peek()
                    .is_ok_and(|shell| self.source.current(&shell))
                {
                    return Err(durable::Error::Gap);
                }
                account::wait(owner.client()?.finish_observation(ticket))
                    .await
                    .map(owned::ResultValue::Legacy)
            }
            .await
        };
        let mut barrier = None;
        if let (Some(editor), Some(captured)) = (&editor, captured) {
            let mut current = editor.state.lock().unwrap();
            current.busy = false;
            match &result {
                Ok(observed)
                    if observed.before().same(&before)
                        && before.row().same_enrollment(observed.after().row()) =>
                {
                    current.review = observed.after().clone();
                    current.direct = observed
                        .after()
                        .row()
                        .state()
                        .local_preservation_intent()
                        .cloned();
                    if !same_local(before.local(), observed.after().local()) {
                        let capture = Capture {
                            review: before.clone(),
                            ..captured
                        };
                        let next = Arc::new(actions::ProjectionBarrier {
                            capture,
                            before: before.clone(),
                            after: observed.after().clone(),
                            id: barrier_id(),
                            renew_writer: false,
                            owned_finish: None,
                            authority: Some(actions::ProjectionAuthority {
                                source: self.source.clone(),
                                binding: None,
                            }),
                        });
                        current.parked = true;
                        current.barrier = Some(next.clone());
                        barrier = Some(next);
                    }
                }
                _ => {
                    current.parked = true;
                    current.error = Some(
                        result
                            .as_ref()
                            .err()
                            .copied()
                            .unwrap_or(durable::Error::Stale),
                    );
                }
            }
            drop(current);
            editor.notify();
        }
        let result = result?;
        if !result.before().same(&before) || !before.row().same_enrollment(result.after().row()) {
            return Err(durable::Error::Stale);
        }
        Ok(Observed {
            before,
            after: result.after().clone(),
            barrier,
        })
    }
}

impl Queue {
    fn abandon_pending(&mut self) {
        self.abandoned.extend(self.pending.drain(..));
    }
}
impl Drop for ObservationQueue {
    fn drop(&mut self) {
        self.source.inventory.clock.change();
    }
}
