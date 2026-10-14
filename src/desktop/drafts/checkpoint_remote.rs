//! Explicit reviewed remote choice stays parked until native projection acknowledgment.
//! No request, DOM write, enrollment or autosave.
use super::*;
use super_platinum_core::drafts::store::sync::{LocalReview, RemoteChoice, RemoteReview};
use super_platinum_core::drafts::sync::Workflow;

#[derive(Clone)]
pub(crate) struct RemoteLease {
    editor: Arc<Editor>,
    serial: u64,
    writer: String,
    barrier: Option<Arc<ProjectionBarrier>>,
    base: View,
    pending: Option<LocalDraft>,
    before: LocalDraft,
    inventory: LocalReview,
}
impl RemoteLease {
    pub fn inventory(&self) -> &LocalReview {
        &self.inventory
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
    pub fn before_local(&self) -> &LocalDraft {
        &self.before
    }
}
fn same_view(a: &View, b: &View) -> bool {
    same_local(&a.local, &b.local)
        && match (&a.stored, &b.stored) {
            (Some(a), Some(b)) => a.revision == b.revision && a.state == b.state,
            (None, None) => true,
            _ => false,
        }
        && match (&a.blocked, &b.blocked) {
            (Some(a), Some(b)) => a.token == b.token && same_local(&a.draft, &b.draft),
            (None, None) => true,
            _ => false,
        }
        && same_pending(a.uncommitted.as_ref(), b.uncommitted.as_ref())
}
fn same_pending(a: Option<&LocalDraft>, b: Option<&LocalDraft>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => same_local(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn same_barrier(a: Option<&Arc<ProjectionBarrier>>, b: Option<&Arc<ProjectionBarrier>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn inventory_view(review: &LocalReview) -> View {
    View {
        local: review.local.clone(),
        stored: review.stored.clone(),
        blocked: review.blocked.clone(),
        uncommitted: None,
    }
}
fn same_inventory(a: &LocalReview, b: &LocalReview) -> bool {
    a.scope == b.scope
        && same_view(&inventory_view(a), &inventory_view(b))
        && a.alternates.len() == b.alternates.len()
        && a.alternates.iter().zip(&b.alternates).all(|(a, b)| {
            a.writer_id == b.writer_id && a.revision == b.revision && same_local(&a.draft, &b.draft)
        })
}
fn available(state: &State) -> bool {
    !state.running
        && !state.settling
        && !state.observing
        && !state.released
        && state.return_handoff.is_none()
        && state.local_writes.is_empty()
        && state.base.uncommitted.is_none()
}
fn eligible_inventory(review: &LocalReview) -> bool {
    review.stored.as_ref().is_some_and(|row| {
        row.state.scope() == &review.scope
            && row.state.acknowledged().is_some()
            && row.state.pending().is_none()
            && row.state.consumption().is_none()
            && row.state.blocked_observation().is_none()
            && matches!(
                row.state.workflow(),
                Workflow::Idle | Workflow::Conflict { pending: None, .. }
            )
    })
}
fn validate_lease(editor: &Arc<Editor>, state: &State, lease: &RemoteLease) -> Result<(), Fault> {
    if !Arc::ptr_eq(editor, &lease.editor)
        || !available(state)
        || state.serial != lease.serial
        || state.local_writer.id() != lease.writer
        || !same_view(&state.base, &lease.base)
        || !same_barrier(state.projection.as_ref(), lease.barrier.as_ref())
        || !same_pending(
            state.pending.as_ref().map(|(_, local)| local),
            lease.pending.as_ref(),
        )
    {
        return Err(Fault::Stale);
    }
    Ok(())
}
impl Editor {
    pub fn remote_review_current(self: &Arc<Self>, lease: &RemoteLease) -> bool {
        validate_lease(self, &self.state.lock().unwrap(), lease).is_ok()
    }
    /// Cheap render eligibility only; event-time lease/read/CAS guards remain
    /// authoritative. Idle owners surface this action only for a parked handoff.
    pub fn remote_review_available(&self) -> bool {
        let state = self.state.lock().unwrap();
        available(&state)
            && state.base.stored.as_ref().is_some_and(|row| {
                row.state.acknowledged().is_some()
                    && row.state.pending().is_none()
                    && row.state.consumption().is_none()
                    && row.state.blocked_observation().is_none()
                    && matches!(
                        row.state.workflow(),
                        Workflow::Idle | Workflow::Conflict { pending: None, .. }
                    )
                    && (state.projection.is_some()
                        || matches!(row.state.workflow(), Workflow::Conflict { .. }))
            })
    }
    /// Explicitly preserve the immutable previous visible version for a NEW
    /// whole-inventory review when another writer prevents finishing the choice.
    /// The mounted controller proves current live/Book equality; this never
    /// refreshes the barrier or authorizes projecting the already chosen value.
    pub async fn preserve_remote_predecessor(
        self: &Arc<Self>,
        barrier_id: String,
        serial: u64,
        live: LocalDraft,
    ) -> Result<PreservedAlternate, Fault> {
        {
            let state = self.state.lock().unwrap();
            let barrier = state.projection.as_ref().ok_or(Fault::Stale)?;
            let after = barrier.after.as_ref().ok_or(Fault::Stale)?;
            if !available(&state)
                || barrier.id != barrier_id
                || barrier.scope != self.owner.scope
                || state.serial != serial
                || barrier.serial != serial
                || state.error != Some(Fault::Stale)
                || !live.attachments.is_empty()
                || !same_local(&barrier.before.local, &live)
                || !same_view(&state.base, after)
            {
                return Err(Fault::Stale);
            }
        }
        self.preserve_projection_input(barrier_id, serial, live)
            .await
    }

    /// Supply the exact complete foreground predecessor. The controller must
    /// guard its native mount/live/Book before and after this await and the read.
    pub async fn prepare_remote_review(
        self: &Arc<Self>,
        live: LocalDraft,
    ) -> Result<RemoteLease, Fault> {
        let _editor = self.gate.lock().await;
        let (serial, before, pending, writer, barrier) = {
            let state = self.state.lock().unwrap();
            if !available(&state) || !live.attachments.is_empty() {
                return Err(Fault::Stale);
            }
            (
                state.serial,
                state.base.clone(),
                state.pending.as_ref().map(|(_, local)| local.clone()),
                state.local_writer.id().to_owned(),
                state.projection.clone(),
            )
        };
        let inventory = self.owner.review_local().await?;
        if inventory.scope != self.owner.scope
            || !eligible_inventory(&inventory)
            || !inventory.contains_local(&live)
            || pending
                .as_ref()
                .is_some_and(|local| !same_local(local, &live) || !inventory.contains_local(local))
        {
            return Err(Fault::AlternateEdits);
        }
        let mut state = self.state.lock().unwrap();
        if !available(&state)
            || state.serial != serial
            || !same_view(&state.base, &before)
            || !same_barrier(state.projection.as_ref(), barrier.as_ref())
            || state.local_writer.id() != writer
        {
            return Err(Fault::Stale);
        }
        let current = inventory_view(&inventory);
        // Explicit recovery of an unknown/changed disk predecessor never clears
        // the park. A new human review can subsequently choose and project it.
        if !same_view(&state.base, &current) {
            let mut predecessor = before;
            predecessor.local = live.clone();
            state.base = current;
            state.error = Some(Fault::Stale);
            state.direct_local = None;
            state.projection = Some(Arc::new(ProjectionBarrier {
                id: super_platinum_core::drafts::store::LocalWriter::default()
                    .id()
                    .into(),
                scope: self.owner.scope.clone(),
                serial,
                before: predecessor,
                // A refreshed inventory is not a chosen version. No projection
                // ticket may be minted until the explicit whole-review CAS.
                after: None,
            }));
        }
        let lease = RemoteLease {
            editor: self.clone(),
            serial,
            writer,
            barrier: state.projection.clone(),
            base: state.base.clone(),
            pending,
            before: live,
            inventory,
        };
        drop(state);
        self.changed.notify_waiters();
        Ok(lease)
    }
    pub async fn reconcile_remote(
        self: &Arc<Self>,
        lease: RemoteLease,
        review: RemoteReview,
        choice: RemoteChoice,
    ) -> Result<Arc<ProjectionBarrier>, Fault> {
        self.reconcile_remote_using(lease, review, choice, |owner, review, choice| async move {
            owner.reconcile_remote_review(review, choice).await
        })
        .await
    }
    async fn reconcile_remote_using<F, Fut>(
        self: &Arc<Self>,
        lease: RemoteLease,
        review: RemoteReview,
        choice: RemoteChoice,
        apply: F,
    ) -> Result<Arc<ProjectionBarrier>, Fault>
    where
        F: FnOnce(Owner, RemoteReview, RemoteChoice) -> Fut + Send + 'static,
        Fut: Future<Output = Result<View, Fault>> + Send,
    {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                editor
                    .reconcile_remote_inner(lease, review, choice, apply)
                    .await
            })
            .await
            .map_err(|_| Fault::Storage)?
    }
    async fn reconcile_remote_inner<F, Fut>(
        self: &Arc<Self>,
        lease: RemoteLease,
        review: RemoteReview,
        choice: RemoteChoice,
        apply: F,
    ) -> Result<Arc<ProjectionBarrier>, Fault>
    where
        F: FnOnce(Owner, RemoteReview, RemoteChoice) -> Fut,
        Fut: Future<Output = Result<View, Fault>>,
    {
        let guard = self.gate.lock().await;
        {
            let mut state = self.state.lock().unwrap();
            validate_lease(self, &state, &lease)?;
            if !same_inventory(&lease.inventory, review.inventory()) {
                return Err(Fault::Stale);
            }
            state.settling = true;
        }
        self.changed.notify_waiters();
        // Disabled outside the exact installed isolated fixture owner/editor.
        super::super::remote_review_fixture::commit_checkpoint(&self.owner.scope, self).await;
        let result = apply(self.owner.clone(), review, choice).await;
        let barrier = {
            let mut state = self.state.lock().unwrap();
            state.settling = false;
            if let Ok(view) = &result {
                state.base = view.clone();
                state.local_writer = Default::default();
                if state.serial == lease.serial {
                    state.pending = None;
                }
            }
            let mut before = lease.base;
            before.local = lease.before;
            let barrier = Arc::new(ProjectionBarrier {
                id: super_platinum_core::drafts::store::LocalWriter::default()
                    .id()
                    .into(),
                scope: self.owner.scope.clone(),
                serial: lease.serial,
                before,
                after: result.as_ref().ok().cloned(),
            });
            state.projection = Some(barrier.clone());
            state.error = Some(result.as_ref().err().copied().unwrap_or(Fault::Stale));
            state.direct_local = None;
            barrier
        };
        drop(guard);
        self.changed.notify_waiters();
        // Newer input owns an already retained runner waiting at the editor gate;
        // it sees the park and persists as an alternate, never over the choice.
        result.map(|_| barrier)
    }
}
#[cfg(test)]
#[path = "checkpoint_remote_tests.rs"]
pub(crate) mod tests;
