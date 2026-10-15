//! Shared Editor handoff; no UI coordinator, HTTP, enrollment, or scheduling UI.
use super::super::dispatch_schedule::{clean_schedule_review, same_schedule_view, schedule_view};
use super::*;
use std::sync::atomic::{AtomicU8, Ordering};
use super_platinum_core::drafts::store::sync::{LocalReview, ReviewedScheduleCommit};
use super_platinum_core::drafts::sync::{Snapshot as RawSnapshot, Workflow};

/// An opaque dialog instance, separate from the editor and account visit. The
/// future controller creates one per mount, invalidates on dismissal/replacement
/// and supplies its CURRENT instance with the authentic native editor receipt.
/// This core-facing lease does not prove current DOM/Binding by itself.
#[derive(Clone)]
pub(crate) struct ScheduleVisit(Arc<AtomicU8>);
impl ScheduleVisit {
    pub fn new() -> Self {
        Self(Arc::new(AtomicU8::new(0)))
    }
    pub fn invalidate(&self) {
        let _ = self
            .0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
    }
    fn active(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }
    fn claim(&self) -> Result<(), Fault> {
        self.0
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| Fault::Stale)
    }
}

#[derive(Clone)]
pub(crate) struct ScheduleLease {
    editor: Arc<Editor>,
    visit: ScheduleVisit,
    serial: u64,
    writer: String,
    base: View,
    inventory: LocalReview,
}
impl ScheduleLease {
    pub fn serial(&self) -> u64 {
        self.serial
    }
    pub fn before(&self) -> &View {
        &self.base
    }
    pub fn scope(&self) -> &Scope {
        &self.inventory.scope
    }
}

fn available(state: &State) -> bool {
    !state.running
        && !state.settling
        && !state.observing
        && !state.released
        && state.error.is_none()
        && state.return_handoff.is_none()
        && state.local_writes.is_empty()
        && state.pending.is_none()
        && state.projection.is_none()
        && state.base.blocked.is_none()
        && state.base.uncommitted.is_none()
}
fn eligible(view: &View) -> bool {
    view.stored.as_ref().is_some_and(|row| {
        row.state.acknowledged().is_some()
            && row.state.desired().is_some()
            && row.state.workflow() == &Workflow::Idle
            && row.state.consumption().is_none()
            && row.state.blocked_observation().is_none()
    }) && !view.local.composer.text.trim().is_empty()
        && view.local.attachments.is_empty()
}
fn validate(
    editor: &Arc<Editor>,
    state: &State,
    lease: &ScheduleLease,
    visit: &ScheduleVisit,
) -> Result<(), Fault> {
    if !Arc::ptr_eq(editor, &lease.editor)
        || !Arc::ptr_eq(&visit.0, &lease.visit.0)
        || !visit.active()
        || !available(state)
        || !eligible(&state.base)
        || state.serial != lease.serial
        || state.local_writer.id() != lease.writer
        || !same_schedule_view(&state.base, &lease.base)
    {
        return Err(Fault::Stale);
    }
    Ok(())
}

impl Editor {
    pub fn reviewed_schedule_current(
        self: &Arc<Self>,
        lease: &ScheduleLease,
        visit: &ScheduleVisit,
    ) -> bool {
        validate(self, &self.state.lock().unwrap(), lease, visit).is_ok()
    }
    /// Flush before gate acquisition; holding the gate across flush deadlocks
    /// its queued input runner. Never rebase a changed durable predecessor here.
    pub async fn prepare_reviewed_schedule(
        self: &Arc<Self>,
        live: LocalDraft,
        visit: ScheduleVisit,
    ) -> Result<ScheduleLease, Fault> {
        loop {
            self.flush().await?;
            let guard = self.gate.lock().await;
            if self.state.lock().unwrap().running {
                drop(guard);
                continue;
            }
            let (serial, writer, base) = {
                let state = self.state.lock().unwrap();
                if !available(&state)
                    || !eligible(&state.base)
                    || !visit.active()
                    || !same_local(&state.base.local, &live)
                {
                    return Err(Fault::Stale);
                }
                (
                    state.serial,
                    state.local_writer.id().to_owned(),
                    state.base.clone(),
                )
            };
            let inventory = self.owner.review_local().await?;
            if inventory.scope != self.owner.scope
                || !clean_schedule_review(&inventory)
                || !same_schedule_view(&base, &schedule_view(&inventory))
            {
                return Err(Fault::Stale);
            }
            let lease = ScheduleLease {
                editor: self.clone(),
                visit: visit.clone(),
                serial,
                writer,
                base,
                inventory,
            };
            validate(self, &self.state.lock().unwrap(), &lease, &visit)?;
            return Ok(lease);
        }
    }
    pub async fn consume_reviewed_schedule(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        reviewed: RawSnapshot,
        post_at: i64,
    ) -> Result<Stored, Fault> {
        self.consume_reviewed_schedule_using(
            lease,
            visit,
            reviewed,
            post_at,
            |backend, expected, raw, time| async move {
                backend.commit_reviewed_schedule(expected, raw, time).await
            },
        )
        .await
    }
    /// UI-only entry: retain a native-thread admission callback through the
    /// final owner read without moving Signals to the Tokio worker runtime.
    /// The detached task still owns persistence if its dialog waiter disappears.
    pub async fn consume_reviewed_schedule_guarded<G>(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        reviewed: RawSnapshot,
        post_at: i64,
        guard: G,
    ) -> Result<Stored, Fault>
    where
        G: FnOnce() -> Result<(), Fault> + 'static,
    {
        let editor = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = editor
                .consume_reviewed_schedule_inner(
                    lease,
                    visit,
                    reviewed,
                    post_at,
                    guard,
                    |backend, expected, raw, time| async move {
                        backend.commit_reviewed_schedule(expected, raw, time).await
                    },
                )
                .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| Fault::Storage)?
    }
    // Only the storage execution boundary is substitutable: tests still pass
    // through the real owner read/guards, pure reducer and SQLite transaction.
    async fn consume_reviewed_schedule_using<C, Fut>(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        reviewed: RawSnapshot,
        post_at: i64,
        commit: C,
    ) -> Result<Stored, Fault>
    where
        C: FnOnce(Backend, LocalReview, RawSnapshot, i64) -> Fut + Send + 'static,
        Fut: Future<Output = Result<ReviewedScheduleCommit, Fault>> + Send,
    {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                editor
                    .consume_reviewed_schedule_inner(
                        lease,
                        visit,
                        reviewed,
                        post_at,
                        || Ok(()),
                        commit,
                    )
                    .await
            })
            .await
            .map_err(|_| Fault::Storage)?
    }
    async fn consume_reviewed_schedule_inner<G, C, Fut>(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        reviewed: RawSnapshot,
        post_at: i64,
        caller_guard: G,
        commit: C,
    ) -> Result<Stored, Fault>
    where
        G: FnOnce() -> Result<(), Fault>,
        C: FnOnce(Backend, LocalReview, RawSnapshot, i64) -> Fut,
        Fut: Future<Output = Result<ReviewedScheduleCommit, Fault>>,
    {
        loop {
            self.flush().await?;
            let guard = self.gate.lock().await;
            if self.state.lock().unwrap().running {
                drop(guard);
                continue;
            }
            validate(self, &self.state.lock().unwrap(), &lease, &visit)?;
            let mut admitted = false;
            let result = self
                .owner
                .consume_reviewed_schedule_with(
                    lease.inventory.clone(),
                    reviewed,
                    post_at,
                    || {
                        let mut state = self.state.lock().unwrap();
                        validate(self, &state, &lease, &visit)?;
                        caller_guard()?;
                        visit.claim()?;
                        state.settling = true;
                        admitted = true;
                        Ok(())
                    },
                    commit,
                )
                .await;
            {
                let mut state = self.state.lock().unwrap();
                if admitted {
                    state.settling = false;
                    match &result {
                        Ok(stored) => {
                            state.direct_local = local::direct_intent(stored);
                            state.base = View {
                                local: LocalDraft::default(),
                                stored: Some(stored.clone()),
                                blocked: None,
                                uncommitted: None,
                            };
                            // Never clear pending input submitted after admission.
                        }
                        Err(problem) => {
                            state.error = Some(*problem);
                        }
                    }
                }
            }
            drop(guard);
            self.changed.notify_waiters();
            return result;
        }
    }
}

#[path = "checkpoint_rejected_schedule.rs"]
mod rejected;

#[cfg(test)]
#[path = "checkpoint_schedule_tests.rs"]
mod tests;
