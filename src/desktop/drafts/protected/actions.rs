//! Closed Editor actions retain whole reviews and one-use native visit leases.
use super::editor::{Editor, State};
use super::editor_proof::{Capture, ReviewLease};
use super::*;
use super_platinum_core::drafts::{
    store::sync::{LocalChoice, RemoteChoice},
    sync::{ConsumeKind, RejectedScheduleChoice, Snapshot as RawSnapshot},
};

enum ReviewReply {
    Legacy(durable::Reply<durable::Review>),
    #[cfg(any(test, feature = "protected-native-fixture"))]
    Owned(Arc<super::owned_send::Send>),
}
impl ReviewReply {
    async fn wait(self) -> Result<durable::Review, durable::Error> {
        match self {
            Self::Legacy(reply) => account::wait(reply).await,
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Self::Owned(held) => held.consumed().await,
        }
    }
}
pub(crate) enum Action {
    Consume(ConsumeKind),
    Schedule {
        snapshot: RawSnapshot,
        post_at: i64,
    },
    RejectedSchedule {
        choice: RejectedScheduleChoice,
        now_secs: i64,
    },
    Control(durable::Control),
    Settle(LocalChoice),
    Baseline {
        evidence: Box<super::baseline::Evidence>,
        choice: RemoteChoice,
        state: dioxus::prelude::Signal<crate::state::ShellState>,
    },
}
pub(crate) struct Applied {
    pub before: durable::Review,
    pub after: durable::Review,
    pub serial: u64,
    pub barrier: Option<Arc<ProjectionBarrier>>,
    pub native_clear: NativeClear,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeClear {
    NotAttempted,
    Cleared,
    Deferred,
}
pub(crate) struct ProjectionBarrier {
    pub(super) capture: Capture,
    pub(super) before: durable::Review,
    pub(super) after: durable::Review,
    pub(super) id: String,
    pub(super) renew_writer: bool,
    pub(super) authority: Option<ProjectionAuthority>,
}
#[derive(Clone)]
pub(super) struct ProjectionAuthority {
    pub source: Arc<source::Source>,
    pub binding: Option<crate::drafts::network::Binding>,
}
impl ProjectionAuthority {
    pub fn current(&self, shell: &crate::state::ShellState) -> bool {
        self.source.current(shell)
            && self
                .binding
                .as_ref()
                .is_none_or(|binding| binding.current(shell))
    }
    pub fn live(&self) -> bool {
        self.source.live()
    }
}
impl ProjectionBarrier {
    pub fn before(&self) -> &durable::Review {
        &self.before
    }
    pub fn after(&self) -> &durable::Review {
        &self.after
    }
    pub fn serial(&self) -> u64 {
        self.capture.serial
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl Editor {
    pub(crate) async fn apply_review_owned<G>(
        self: &Arc<Self>,
        lease: ReviewLease,
        action: Action,
        operation: operation::Operation,
        guard: G,
    ) -> Result<Box<Applied>, durable::Error>
    where
        G: FnOnce() -> Result<Option<durable::Stream>, durable::Error> + 'static,
    {
        if !operation.belongs(self) || !operation.permits(&action) {
            return Err(durable::Error::Stale);
        }
        // This path is polled only inside the registered renderer-root future.
        // Keep the actual one-use lease there, rather than moving it into a
        // second task whose response could outlive its receiving waiter.
        let value = self
            .clone()
            .apply_inner_owned(lease, action, guard, Some(operation.clone()))
            .await?;
        operation.consumed(&value)?;
        Ok(value)
    }
    pub async fn prepare_review(
        &self,
        visit: Visit,
        live: &LocalDraft,
    ) -> Result<ReviewLease, durable::Error> {
        self.flush().await?;
        let _guard = self.gate.lock().await;
        let captured = self.capture(visit.clone())?;
        if captured.pending.is_some() || !same_local(captured.review.local(), live) {
            return Err(durable::Error::ReviewRequired);
        }
        let fresh = self.owner.review().await?;
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.serial != captured.serial
            || !state.review.same(&captured.review)
            || state.running
            || state.busy
            || state.parked
            || !editor_proof::can_rebase(&captured.review, &fresh, &captured.writer)
            || !visit.current()
        {
            return Err(durable::Error::Stale);
        }
        state.review = fresh;
        drop(state);
        ReviewLease::capture(self.capture(visit)?)
    }

    /// Final native/Source check runs on the renderer after the last await.
    /// It cannot await. The core command is enqueued before releasing state.
    pub async fn apply_review<G>(
        self: &Arc<Self>,
        lease: ReviewLease,
        action: Action,
        guard: G,
    ) -> Result<Box<Applied>, durable::Error>
    where
        G: FnOnce() -> Result<Option<durable::Stream>, durable::Error> + 'static,
    {
        let editor = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        // Root-scope completion remains detached; only its handoff is heap-backed.
        dioxus::core::spawn_forever(Box::pin(async move {
            let result = editor.apply_inner(lease, action, guard).await;
            let _ = send.send(result);
        }));
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    pub(super) async fn apply_inner<G>(
        self: Arc<Self>,
        lease: ReviewLease,
        action: Action,
        guard: G,
    ) -> Result<Box<Applied>, durable::Error>
    where
        G: FnOnce() -> Result<Option<durable::Stream>, durable::Error>,
    {
        self.apply_inner_owned(lease, action, guard, None).await
    }
    async fn apply_inner_owned<G>(
        self: Arc<Self>,
        lease: ReviewLease,
        action: Action,
        guard: G,
        operation: Option<operation::Operation>,
    ) -> Result<Box<Applied>, durable::Error>
    where
        G: FnOnce() -> Result<Option<durable::Stream>, durable::Error>,
    {
        let settling = matches!(&action, Action::Settle(_) | Action::Baseline { .. });
        if !settling {
            self.flush().await?;
        }
        let _gate = self.gate.lock().await;
        let fresh = self.owner.review().await?;
        let visit = lease.visit();
        let captured = if settling {
            self.capture_settlement(visit, lease.visible())?
        } else {
            self.capture(visit)?
        };
        if !fresh.same(lease.review()) {
            return Err(durable::Error::Stale);
        }
        let consuming = matches!(&action, Action::Consume(_) | Action::Schedule { .. });
        let renew_writer = matches!(&action, Action::Settle(_) | Action::Baseline { .. });
        let returning = matches!(
            &action,
            Action::Control(durable::Control::ReturnConsumption(_))
        );
        let authority = match &action {
            Action::Baseline { evidence, .. } => Some(ProjectionAuthority {
                source: evidence.source.clone(),
                binding: Some(evidence.binding.clone()),
            }),
            _ => None,
        };
        let choice_retry = match &action {
            Action::Baseline {
                evidence, choice, ..
            } => Some(Arc::new(baseline_choice::Retry {
                evidence: evidence.as_ref().clone(),
                choice: choice.clone(),
                capture: captured.clone(),
            })),
            _ => None,
        };
        let (before, serial, reply) = {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.retired
                || state.serial != captured.serial
                || !state.review.same(&captured.review)
                || state.running
                || state.busy
                || (state.parked && !settling)
            {
                return Err(durable::Error::Stale);
            }
            if let Some(operation) = &operation {
                operation.phase(operation::Phase::Consume)?;
            }
            let stream = guard()?;
            let expected = lease.claim(&captured)?;
            let before = expected.clone();
            #[cfg(any(test, feature = "protected-native-fixture"))]
            let routed = self.owner.account.owned.clone();
            #[cfg(not(any(test, feature = "protected-native-fixture")))]
            let routed: Option<()> = None;
            let reply = if routed.is_some() {
                #[cfg(any(test, feature = "protected-native-fixture"))]
                {
                    let route = routed.as_ref().expect("checked route");
                    if !matches!(action, Action::Consume(ConsumeKind::Send)) {
                        return Err(route.unsupported());
                    }
                    let parent = operation.as_ref().ok_or(durable::Error::Unsupported)?;
                    let stream = stream.ok_or(durable::Error::Gap)?;
                    self.owner.check_stream(&stream)?;
                    self.owner.check(&expected)?;
                    let held = super::owned_send::Send::new(
                        route,
                        super::owned_route::o::SendInput { stream, expected },
                    );
                    // Existing root owns returned input and complete fixed family
                    // before actual Consume enqueue or any await.
                    parent.retain(|s| s.owned_send = Some(held.clone()))?;
                    held.consume.submit().map_err(super::owned_route::ui)?;
                    ReviewReply::Owned(held)
                }
                #[cfg(not(any(test, feature = "protected-native-fixture")))]
                {
                    unreachable!()
                }
            } else {
                let reply = ReviewReply::Legacy(match action {
                    Action::Consume(kind) => {
                        self.owner
                            .consume(stream.ok_or(durable::Error::Gap)?, expected, kind)?
                    }
                    Action::Schedule { snapshot, post_at } => self.owner.schedule(
                        stream.ok_or(durable::Error::Gap)?,
                        expected,
                        snapshot,
                        post_at,
                    )?,
                    Action::RejectedSchedule { choice, now_secs } => self.owner.rejected_schedule(
                        stream.ok_or(durable::Error::Gap)?,
                        expected,
                        choice,
                        now_secs,
                    )?,
                    Action::Control(action) => self.owner.control(expected, action)?,
                    Action::Settle(choice) => self.owner.settle(expected, choice)?,
                    Action::Baseline {
                        evidence,
                        choice,
                        state,
                    } => {
                        use dioxus::prelude::ReadableExt;
                        if !evidence.evidence.review().same(&expected) {
                            return Err(durable::Error::Stale);
                        }
                        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
                        if !evidence.binding.current(&shell) || !evidence.source.current(&shell) {
                            return Err(durable::Error::Gap);
                        }
                        self.owner.choose_baseline(evidence.evidence, choice)?
                    }
                });

                reply
            };
            state.busy = true;
            (before, state.serial, reply)
        };
        let result = reply.wait().await;
        let mut state = self.state.lock().unwrap();
        state.busy = false;
        let result = match result {
            Ok(after)
                if after.scope() == self.owner.scope()
                    && before.row().same_enrollment(after.row()) =>
            {
                state.review = after.clone();
                state.direct = after.row().state().local_preservation_intent().cloned();
                if settling && state.pending.as_ref().is_some_and(|(id, _)| *id == serial) {
                    state.pending = None;
                }
                state.preserved = None;
                state.choice_retry = None;
                let barrier = if renew_writer
                    || returning
                    || (!consuming && !same_local(before.local(), after.local()))
                {
                    state.parked = true;
                    let barrier = Arc::new(ProjectionBarrier {
                        capture: captured,
                        before: before.clone(),
                        after: after.clone(),
                        id: barrier_id(),
                        renew_writer,
                        authority,
                    });
                    state.barrier = Some(barrier.clone());
                    Some(barrier)
                } else {
                    None
                };
                Ok(Box::new(Applied {
                    native_clear: NativeClear::NotAttempted,
                    before,
                    after,
                    serial,
                    barrier,
                }))
            }
            Ok(_) => {
                state.error = Some(durable::Error::Stale);
                state.parked = true;
                Err(durable::Error::Stale)
            }
            Err(error) => {
                state.error = Some(error);
                if error == durable::Error::Storage {
                    state.choice_retry = choice_retry;
                }
                Err(error)
            }
        };
        drop(state);
        self.notify();
        result
    }
}

/// Exact barrier identity and post-choice inventory remain mandatory even when
/// visible text happens to equal the chosen text. Native evidence is supplied
/// separately by the renderer controller, never inferred here.
pub(super) fn projection_current(state: &State, barrier: &Arc<ProjectionBarrier>) -> bool {
    state
        .barrier
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, barrier))
        && state.parked
        && !state.running
        && !state.busy
        && state.pending.is_none()
        && state.serial == barrier.capture.serial
        && state.writer.id() == barrier.capture.writer.id()
        && state.writer.enrollment() == barrier.capture.writer.enrollment()
        && state.review.same(&barrier.after)
}
