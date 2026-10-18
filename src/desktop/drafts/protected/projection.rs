//! Full protected inventory and writer identity remain attached to native proof.
use super::*;
use super::{
    actions::{ProjectionBarrier, projection_current},
    editor::Editor,
};

/// One exact owned registration for a native-applied projection. Failed or
/// superseded work remains attached to the Editor; another barrier cannot
/// silently allocate a replacement job or another writer.
pub(super) struct Registration {
    barrier: Arc<ProjectionBarrier>,
    review: durable::Review,
    held: Arc<super::owned_route::Held<durable::Review, durable::Writer>>,
}

#[cfg(test)]
#[path = "projection/owned_tests.rs"]
mod owned_tests;

/// Constructed only after the actual RendererApplied receipt and the existing
/// current-review/native-authority/registration checks have completed.
pub(crate) struct Acknowledged {
    pub(super) barrier: Arc<ProjectionBarrier>,
    pub(super) review: durable::Review,
    pub(super) editor: std::sync::Weak<Editor>,
}

pub(crate) struct ProjectionTicket {
    barrier: Arc<ProjectionBarrier>,
    review: durable::Review,
}
impl ProjectionTicket {
    pub fn scope(&self) -> &Scope {
        self.review.scope()
    }
    pub fn barrier_id(&self) -> &str {
        self.barrier.id()
    }
    pub fn serial(&self) -> u64 {
        self.barrier.serial()
    }
    pub fn before_local(&self) -> &LocalDraft {
        &self.barrier.capture.visible
    }
    pub fn after_local(&self) -> &LocalDraft {
        self.review.local()
    }
    pub fn authority_current(&self, shell: &crate::state::ShellState) -> bool {
        self.barrier
            .authority
            .as_ref()
            .is_none_or(|authority| authority.current(shell))
    }
    fn authority_live(&self) -> bool {
        self.barrier
            .authority
            .as_ref()
            .is_none_or(|authority| authority.live())
    }
}
pub(crate) struct PreservedAlternate {
    scope: Scope,
    writer: String,
    serial: u64,
    local: LocalDraft,
    review: durable::Review,
}
impl PreservedAlternate {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn writer_id(&self) -> &str {
        &self.writer
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
    pub fn local(&self) -> &LocalDraft {
        &self.local
    }
    pub fn review(&self) -> &durable::Review {
        &self.review
    }
}
impl Editor {
    /// Native text observed after an admission/clear refusal is preserved as a
    /// version, never treated as a newly authorized canonical replacement.
    pub async fn preserve_native_input(
        self: &Arc<Self>,
        serial: u64,
        local: LocalDraft,
    ) -> Result<(), durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let gate = editor.gate.lock().await;
                let writer = {
                    let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                    if state.serial != serial
                        || state.running
                        || state.busy
                        || state.released.is_some()
                    {
                        return Err(durable::Error::Stale);
                    }
                    state.parked = true;
                    state.writer.clone()
                };
                let submitted = editor.submit(&local)?;
                drop(gate);
                let _ = editor.flush().await;
                let review = editor.owner.review().await?;
                if review.alternates().iter().any(|alternate| {
                    alternate.writer_id == writer.id()
                        && alternate.revision == submitted
                        && same_local(&alternate.draft, &local)
                }) {
                    Ok(())
                } else {
                    Err(durable::Error::ReviewRequired)
                }
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
    pub async fn prepare_projection(&self, id: &str) -> Result<ProjectionTicket, durable::Error> {
        let _gate = self.gate.lock().await;
        let barrier = {
            let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            let barrier = state.barrier.as_ref().ok_or(durable::Error::Stale)?;
            if barrier.id() != id || !projection_current(&state, barrier) {
                return Err(durable::Error::Stale);
            }
            barrier.clone()
        };
        let review = self.owner.review().await?;
        if !review.same(barrier.after())
            || !editor_proof::clean(&review)
            || !projection_current(&self.state.lock().unwrap(), &barrier)
        {
            return Err(durable::Error::Stale);
        }
        Ok(ProjectionTicket { barrier, review })
    }
    pub fn with_current_projection<T>(
        &self,
        ticket: ProjectionTicket,
        install: impl FnOnce(ProjectionTicket) -> Result<T, durable::Error>,
    ) -> Result<T, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !projection_current(&state, &ticket.barrier) || !state.review.same(&ticket.review) {
            return Err(durable::Error::Stale);
        }
        install(ticket)
    }
    pub async fn preserve_projection_input(
        self: &Arc<Self>,
        id: String,
        serial: u64,
        local: LocalDraft,
    ) -> Result<PreservedAlternate, durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let gate = editor.gate.lock().await;
                let writer = {
                    let state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                    if state.serial != serial
                        || state.barrier.as_ref().is_none_or(|barrier| {
                            barrier.id() != id || !projection_current(&state, barrier)
                        })
                    {
                        return Err(durable::Error::Stale);
                    }
                    state.writer.clone()
                };
                let submitted = editor.submit(&local)?;
                drop(gate);
                let _ = editor.flush().await;
                let _gate = editor.gate.lock().await;
                let review = editor.owner.review().await?;
                let state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                if state.serial != submitted
                    || state.writer.id() != writer.id()
                    || state.running
                    || state.busy
                    || state
                        .barrier
                        .as_ref()
                        .is_none_or(|barrier| barrier.id() != id)
                    || !review.alternates().iter().any(|alternate| {
                        alternate.writer_id == writer.id()
                            && alternate.revision == submitted
                            && same_local(&alternate.draft, &local)
                    })
                    || !review.origins().iter().any(|origin| {
                        origin.writer_id() == writer.id()
                            && origin.origin()
                                == &durable::WriterOrigin::Enrollment(
                                    writer.enrollment().to_owned(),
                                )
                    })
                {
                    return Err(durable::Error::Stale);
                }
                Ok(PreservedAlternate {
                    scope: editor.owner.scope.clone(),
                    writer: writer.id().to_owned(),
                    serial: submitted,
                    local,
                    review,
                })
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
    pub async fn acknowledge_projection(
        self: &Arc<Self>,
        applied: crate::view::composer::protected_projection::RendererApplied,
    ) -> Result<(), durable::Error> {
        self.acknowledge_projection_observed(applied)
            .await
            .map(|_| ())
    }
    pub(crate) async fn acknowledge_projection_observed(
        self: &Arc<Self>,
        applied: crate::view::composer::protected_projection::RendererApplied,
    ) -> Result<Arc<Acknowledged>, durable::Error> {
        let ticket = applied.into_editor_ticket();
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let _gate = editor.gate.lock().await;
                if !ticket.authority_live()
                    || !projection_current(&editor.state.lock().unwrap(), &ticket.barrier)
                {
                    return Err(durable::Error::Stale);
                }
                let review = editor.owner.review().await?;
                if !review.same(&ticket.review) || !editor_proof::clean(&review) {
                    return Err(durable::Error::Stale);
                }
                let registration = if ticket.barrier.renew_writer {
                    editor.projection_registration(&ticket, &review)?
                } else {
                    None
                };
                let writer = if let Some(registration) = &registration {
                    Some(
                        registration
                            .held
                            .value()
                            .await
                            .map_err(super::owned_route::ui)??,
                    )
                } else if ticket.barrier.renew_writer {
                    Some(account::wait(editor.owner.register(review.clone())?).await?)
                } else {
                    None
                };
                // Registration can race another writer. It is not permission to
                // publish the older inventory or retire a newly typed serial.
                let current = editor.owner.review().await?;
                let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                if !ticket.authority_live()
                    || !projection_current(&state, &ticket.barrier)
                    || !current.same(&review)
                {
                    return Err(durable::Error::Stale);
                }
                if let Some(writer) = writer {
                    if writer.scope() != editor.owner.scope()
                        || writer.enrollment() != review.row().lineage().enrollment()
                    {
                        return Err(durable::Error::Stale);
                    }
                    if let Some(registration) = &registration {
                        if state
                            .projection_registration
                            .as_ref()
                            .is_none_or(|held| !Arc::ptr_eq(held, registration))
                        {
                            return Err(durable::Error::Stale);
                        }
                        let job = registration.held.job().map_err(super::owned_route::ui)?;
                        let (observed, acknowledgment) =
                            job.observe_success().map_err(super::owned_route::ui)?;
                        if observed.id() != writer.id()
                            || observed.scope() != writer.scope()
                            || observed.enrollment() != writer.enrollment()
                        {
                            return Err(durable::Error::Stale);
                        }
                        // The renderer has already accepted the exact projection;
                        // current native authority, barrier, serial and storage
                        // were checked above while holding this same state lock.
                        acknowledgment
                            .acknowledge()
                            .map_err(|(error, _)| super::owned_route::ui(error))?;
                        state.projection_registration = None;
                    }
                    state.writer = writer;
                }
                if let Some(finish) = &ticket.barrier.owned_finish {
                    // Actual durable choice and native projection have now
                    // been observed. Retry history may refuse memory retirement;
                    // keep that exact job in the Editor instead of discarding it
                    // or undoing an already accepted renderer projection.
                    if finish.acknowledge(&review).is_ok() {
                        state
                            .owned_baseline_finishes
                            .retain(|v| !Arc::ptr_eq(v, finish));
                    }
                }
                state.barrier = None;
                state.parked = false;
                state.error = None;
                state.preserved = None;
                state.direct = current.row().state().local_preservation_intent().cloned();
                drop(state);
                editor.notify();
                Ok(Arc::new(Acknowledged {
                    barrier: ticket.barrier,
                    review,
                    editor: Arc::downgrade(&editor),
                }))
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }

    fn projection_registration(
        &self,
        ticket: &ProjectionTicket,
        review: &durable::Review,
    ) -> Result<Option<Arc<Registration>>, durable::Error> {
        let Some(route) = &self.owner.account.owned else {
            return Ok(None);
        };
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !ticket.authority_live()
            || !projection_current(&state, &ticket.barrier)
            || !state.review.same(review)
        {
            return Err(durable::Error::Stale);
        }
        if let Some(held) = &state.projection_registration {
            if !Arc::ptr_eq(&held.barrier, &ticket.barrier) || !held.review.same(review) {
                return Err(durable::Error::Pending);
            }
            return Ok(Some(held.clone()));
        }
        let held = Arc::new(Registration {
            barrier: ticket.barrier.clone(),
            review: review.clone(),
            held: super::owned_route::Held::new(route.operations.register(review.clone())),
        });
        // Retain admission refusal and actual job before submit/await.
        state.projection_registration = Some(held.clone());
        Ok(Some(held))
    }
}
