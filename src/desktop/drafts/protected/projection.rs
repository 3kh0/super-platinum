//! Full protected inventory and writer identity remain attached to native proof.
use super::*;
use super::{
    actions::{ProjectionBarrier, projection_current},
    editor::Editor,
};

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
                let writer = if ticket.barrier.renew_writer {
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
                    state.writer = writer;
                }
                state.barrier = None;
                state.parked = false;
                state.error = None;
                state.preserved = None;
                state.direct = current.row().state().local_preservation_intent().cloned();
                drop(state);
                editor.notify();
                Ok(())
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
}
