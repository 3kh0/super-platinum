//! Exact editor authority for a separate typed native projection protocol.
use super::*;
use super_platinum_core::drafts::store::sync::LocalReview;

/// Created only from an unchanged barrier and a transactional clean inventory.
/// Renderer state belongs to the view's attempt, never to this durable ticket.
#[derive(Debug)]
pub(crate) struct ProjectionTicket {
    barrier: Arc<ProjectionBarrier>,
    writer_id: String,
    review: LocalReview,
}
impl ProjectionTicket {
    pub fn scope(&self) -> &Scope {
        &self.barrier.scope
    }
    pub fn barrier_id(&self) -> &str {
        &self.barrier.id
    }
    pub fn serial(&self) -> u64 {
        self.barrier.serial
    }
    pub fn before_local(&self) -> &LocalDraft {
        &self.barrier.before.local
    }
    pub fn after_local(&self) -> &LocalDraft {
        &self.review.local
    }
}

#[derive(Debug)]
struct Submission {
    scope: Scope,
    writer_id: String,
    serial: u64,
    local: LocalDraft,
}

/// Proof of one exact alternate at the transactional read-back point. It does
/// not authorize settlement, clear a barrier, or promise no later writer edit.
#[derive(Debug)]
pub(crate) struct PreservedAlternate {
    submission: Submission,
    storage_revision: u64,
}
impl PreservedAlternate {
    pub fn scope(&self) -> &Scope {
        &self.submission.scope
    }
    pub fn writer_id(&self) -> &str {
        &self.submission.writer_id
    }
    pub fn serial(&self) -> u64 {
        self.submission.serial
    }
    pub fn local(&self) -> &LocalDraft {
        &self.submission.local
    }
    pub fn storage_revision(&self) -> u64 {
        self.storage_revision
    }
}

fn same_stored(left: Option<&Stored>, right: Option<&Stored>) -> bool {
    matches!((left, right), (Some(left), Some(right))
        if left.revision == right.revision && left.state == right.state)
}

fn clean_review(review: &LocalReview, scope: &Scope, after: &View) -> bool {
    &review.scope == scope
        && review.blocked.is_none()
        && review.alternates.is_empty()
        && after.blocked.is_none()
        && after.uncommitted.is_none()
        && same_local(&review.local, &after.local)
        && same_stored(review.stored.as_ref(), after.stored.as_ref())
}

fn eligible<'a>(
    state: &'a State,
    scope: &Scope,
    id: &str,
) -> Result<&'a Arc<ProjectionBarrier>, Fault> {
    let barrier = state.projection.as_ref().ok_or(Fault::Stale)?;
    let after = barrier.after.as_ref().ok_or(Fault::Stale)?;
    if barrier.id != id
        || &barrier.scope != scope
        || state.serial != barrier.serial
        || state.running
        || state.observing
        || state.settling
        || state.released
        || state.pending.is_some()
        || state.return_handoff.is_some()
        || !state.local_writes.is_empty()
        || state.error != Some(Fault::Stale)
        || state.base.blocked.is_some()
        || state.base.uncommitted.is_some()
        || !same_local(&state.base.local, &after.local)
        || !same_stored(state.base.stored.as_ref(), after.stored.as_ref())
    {
        return Err(Fault::Stale);
    }
    Ok(barrier)
}

fn valid_ticket(state: &State, scope: &Scope, ticket: &ProjectionTicket) -> Result<(), Fault> {
    let barrier = eligible(state, scope, ticket.barrier_id())?;
    if !Arc::ptr_eq(barrier, &ticket.barrier) || state.local_writer.id() != ticket.writer_id {
        return Err(Fault::Stale);
    }
    Ok(())
}

// Caller holds the owner gate. Calling Owner::review_local here would recurse.
async fn read_review(owner: Owner) -> Result<LocalReview, Fault> {
    owner.account.initialize().await?;
    owner.account.backend.read_local_review(owner.scope).await
}

impl Editor {
    pub async fn prepare_projection(
        self: &Arc<Self>,
        barrier_id: &str,
    ) -> Result<ProjectionTicket, Fault> {
        let _editor = self.gate.lock().await;
        let (barrier, writer_id) = {
            let state = self.state.lock().unwrap();
            (
                eligible(&state, &self.owner.scope, barrier_id)?.clone(),
                state.local_writer.id().to_owned(),
            )
        };
        let _owner = self.owner.gate.lock().await;
        let review = read_review(self.owner.clone()).await?;
        if !clean_review(&review, &self.owner.scope, barrier.after.as_ref().unwrap()) {
            return Err(Fault::Stale);
        }
        let ticket = ProjectionTicket {
            barrier,
            writer_id,
            review,
        };
        valid_ticket(&self.state.lock().unwrap(), &self.owner.scope, &ticket)?;
        Ok(ticket)
    }

    /// The callback performs only synchronous, exact-key renderer installation.
    /// It must not await, submit, checkpoint, route, or reenter this editor. The
    /// view alone can turn validated native Applied evidence into RendererApplied.
    pub fn with_current_projection<T>(
        &self,
        ticket: ProjectionTicket,
        install: impl FnOnce(ProjectionTicket) -> Result<T, Fault>,
    ) -> Result<T, Fault> {
        let state = self.state.lock().unwrap();
        valid_ticket(&state, &self.owner.scope, &ticket)?;
        install(ticket)
    }

    pub async fn preserve_projection_input(
        self: &Arc<Self>,
        barrier_id: String,
        expected_serial: u64,
        local: LocalDraft,
    ) -> Result<PreservedAlternate, Fault> {
        self.preserve_using(barrier_id, expected_serial, local, read_review)
            .await
    }

    async fn preserve_using<F, Fut>(
        self: &Arc<Self>,
        barrier_id: String,
        expected_serial: u64,
        local: LocalDraft,
        read: F,
    ) -> Result<PreservedAlternate, Fault>
    where
        F: FnOnce(Owner) -> Fut + Send + 'static,
        Fut: Future<Output = Result<LocalReview, Fault>> + Send + 'static,
    {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                editor
                    .preserve_inner(barrier_id, expected_serial, local, read)
                    .await
            })
            .await
            .map_err(|_| Fault::Storage)?
    }

    async fn preserve_inner<F, Fut>(
        self: &Arc<Self>,
        barrier_id: String,
        expected_serial: u64,
        local: LocalDraft,
        read: F,
    ) -> Result<PreservedAlternate, Fault>
    where
        F: FnOnce(Owner) -> Fut,
        Fut: Future<Output = Result<LocalReview, Fault>>,
    {
        let guard = self.gate.lock().await;
        let (submission, start) =
            {
                let mut state = self.state.lock().unwrap();
                if state.projection.as_ref().is_none_or(|barrier| {
                    barrier.id != barrier_id || barrier.scope != self.owner.scope
                }) || state.serial != expected_serial
                    || state.running
                    || state.observing
                    || state.settling
                    || state.released
                    || state.return_handoff.is_some()
                    || state.error.is_none()
                {
                    return Err(Fault::Stale);
                }
                let writer_id = state.local_writer.id().to_owned();
                let (serial, start) = self.enqueue_locked(&mut state, local.clone());
                (
                    Submission {
                        scope: self.owner.scope.clone(),
                        writer_id,
                        serial,
                        local,
                    },
                    start,
                )
            };
        drop(guard);
        self.start_runner(start);
        // A parked Stale/Storage result is not itself a preservation receipt.
        // The run is the only writer; prove its exact result independently.
        let _ = self.flush().await;
        self.prove_submission(&barrier_id, submission, read).await
    }

    async fn prove_submission<F, Fut>(
        &self,
        barrier_id: &str,
        submission: Submission,
        read: F,
    ) -> Result<PreservedAlternate, Fault>
    where
        F: FnOnce(Owner) -> Fut,
        Fut: Future<Output = Result<LocalReview, Fault>>,
    {
        let _editor = self.gate.lock().await;
        self.valid_submission(barrier_id, &submission)?;
        let _owner = self.owner.gate.lock().await;
        let review = read(self.owner.clone()).await?;
        self.valid_submission(barrier_id, &submission)?;
        if review.scope != submission.scope
            || !review.alternates.iter().any(|row| {
                row.writer_id == submission.writer_id
                    && row.revision == submission.serial
                    && same_local(&row.draft, &submission.local)
            })
        {
            return Err(Fault::Stale);
        }
        let storage_revision = review.stored.as_ref().ok_or(Fault::Stale)?.revision;
        Ok(PreservedAlternate {
            submission,
            storage_revision,
        })
    }

    fn valid_submission(&self, id: &str, submission: &Submission) -> Result<(), Fault> {
        let state = self.state.lock().unwrap();
        if submission.scope != self.owner.scope
            || state
                .projection
                .as_ref()
                .is_none_or(|barrier| barrier.id != id || barrier.scope != submission.scope)
            || state.local_writer.id() != submission.writer_id
            || state.serial != submission.serial
            || state.running
            || state.observing
            || state.settling
            || state.released
            || state.pending.as_ref().is_none_or(|(serial, local)| {
                *serial != submission.serial || !same_local(local, &submission.local)
            })
        {
            return Err(Fault::Stale);
        }
        Ok(())
    }

    /// Only the view's opaque receipt proves provisional native/live/Book
    /// installation. Bare tickets and booleans cannot reach this public entry.
    pub async fn acknowledge_projection(
        self: &Arc<Self>,
        applied: crate::view::composer::RendererApplied,
    ) -> Result<(), Fault> {
        self.acknowledge_using(applied.into_editor_ticket(), read_review)
            .await
    }

    async fn acknowledge_using<F, Fut>(
        self: &Arc<Self>,
        ticket: ProjectionTicket,
        read: F,
    ) -> Result<(), Fault>
    where
        F: FnOnce(Owner) -> Fut + Send + 'static,
        Fut: Future<Output = Result<LocalReview, Fault>> + Send + 'static,
    {
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.acknowledge_inner(ticket, read).await })
            .await
            .map_err(|_| Fault::Storage)?
    }

    async fn acknowledge_inner<F, Fut>(
        &self,
        ticket: ProjectionTicket,
        read: F,
    ) -> Result<(), Fault>
    where
        F: FnOnce(Owner) -> Fut,
        Fut: Future<Output = Result<LocalReview, Fault>>,
    {
        let _editor = self.gate.lock().await;
        valid_ticket(&self.state.lock().unwrap(), &self.owner.scope, &ticket)?;
        let _owner = self.owner.gate.lock().await;
        let review = read(self.owner.clone()).await?;
        let after = ticket.barrier.after.as_ref().ok_or(Fault::Stale)?;
        if !clean_review(&review, ticket.scope(), after)
            || !same_stored(review.stored.as_ref(), ticket.review.stored.as_ref())
            || !same_local(&review.local, &ticket.review.local)
        {
            return Err(Fault::Stale);
        }
        {
            let mut state = self.state.lock().unwrap();
            valid_ticket(&state, &self.owner.scope, &ticket)?;
            state.direct_local = state.base.stored.as_ref().and_then(local::direct_intent);
            state.projection = None;
            state.error = None;
        }
        // The owner gate spans the final read and memory clear. Independent
        // processes may write later; normal load/CAS guards remain mandatory.
        self.changed.notify_waiters();
        Ok(())
    }
}

#[cfg(test)]
#[path = "checkpoint_projection_tests.rs"]
mod tests;
