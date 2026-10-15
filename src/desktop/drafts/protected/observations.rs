//! Bounded ordered ingress adapter. No realtime receiver calls this module.
use super::*;
use super::{editor::Editor, editor_proof::Capture, source::Source};
use crate::{drafts::observation::Prepared, state::ShellState};
use dioxus::prelude::{ReadableExt, Signal};
use std::{collections::VecDeque, sync::Mutex};
use super_platinum_core::slack::events::RawEvent;

const MAX_PENDING: usize = 64;
pub(crate) struct ObservationQueue {
    source: Arc<Source>,
    account: Arc<Account>,
    state: Mutex<Queue>,
}
#[derive(Default)]
struct Queue {
    pending: VecDeque<Work>,
    results: VecDeque<Result<Observed, durable::Error>>,
    running: bool,
    failed: bool,
}
struct Work {
    owner: Owner,
    editor: Option<Arc<Editor>>,
    prepared: Prepared,
}
pub(crate) struct Observed {
    pub before: durable::Review,
    pub after: durable::Review,
    pub barrier: Option<Arc<actions::ProjectionBarrier>>,
}
impl ObservationQueue {
    pub fn new(account: Arc<Account>, source: Arc<Source>) -> Arc<Self> {
        Arc::new(Self {
            account,
            source,
            state: Mutex::default(),
        })
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
        let start = {
            let mut queue = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if queue.failed {
                return Err(durable::Error::Gap);
            }
            if queue.pending.len() + queue.results.len() + usize::from(queue.running) >= MAX_PENDING
            {
                queue.failed = true;
                queue.pending.clear();
                self.source.gap(durable::GapReason::QueueOverflow);
                return Err(durable::Error::Gap);
            }
            queue.pending.push_back(Work {
                owner,
                editor,
                prepared,
            });
            let start = !queue.running;
            queue.running = true;
            start
        };
        if start {
            let queue = self.clone();
            dioxus::core::spawn_forever(async move {
                queue.run(state).await;
            });
        }
        Ok(())
    }
    pub fn take_result(&self) -> Option<Result<Observed, durable::Error>> {
        self.state.lock().ok()?.results.pop_front()
    }
    async fn run(self: Arc<Self>, state: Signal<ShellState>) {
        loop {
            let next = {
                let mut queue = self.state.lock().unwrap();
                if queue.failed {
                    queue.pending.clear();
                }
                let next = queue.pending.pop_front();
                if next.is_none() {
                    queue.running = false;
                }
                next
            };
            let Some(work) = next else { return };
            let result = self.process(state, work).await;
            if let Err(error) = result.as_ref() {
                self.source.gap(if *error == durable::Error::Storage {
                    durable::GapReason::StorageFailure
                } else {
                    durable::GapReason::InterruptedAdmission
                });
            }
            let mut queue = self.state.lock().unwrap();
            queue.failed |= result.is_err();
            queue.results.push_back(result);
        }
    }
    async fn process(
        &self,
        state: Signal<ShellState>,
        work: Work,
    ) -> Result<Observed, durable::Error> {
        let Work {
            owner,
            editor,
            prepared,
        } = work;
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
        let result = async {
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
            account::wait(owner.client()?.finish_observation(ticket)).await
        }
        .await;
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
