//! Small action adapter for the shared structural save pipeline.
use super::*;
use super_platinum_core::{
    canvas::edit::{
        Error,
        structure::{
            AcceptedStartTyping, AcceptedStructure, PreparedStartTypingEdit, PreparedStructureEdit,
            StartTypingIntent, StartTypingWitness, StructureIntent,
        },
        text::TextDraft,
    },
    slack::{
        api::{self, CanvasCollabSnapshot},
        transport::CanvasEditFailure,
    },
};

pub(super) enum Intent {
    Structure(StructureIntent),
    StartTyping(StartTypingIntent, StartTypingWitness),
}
pub(super) enum Prepared {
    Structure(PreparedStructureEdit),
    StartTyping(PreparedStartTypingEdit),
}
pub(super) enum Accepted {
    Structure(AcceptedStructure),
    StartTyping(AcceptedStartTyping),
}
pub(super) fn begin(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    section: &str,
    action: Action,
    value: &TextDraft,
    start: Option<&super::split::Start>,
    now: u64,
) -> Result<(checklist::Attempt, Intent), &'static str> {
    if let Some(start) = start.filter(|s| !s.updates().is_empty()) {
        let witness = StartTypingWitness::new(start.original(), start.updates())
            .map_err(|_| "Keep this edit to supported paragraph text.")?;
        let (attempt, intent) = checklist::start_typing_ops::begin_start_typing(
            shell,
            visit,
            read,
            section,
            witness.clone(),
            now,
        )?;
        Ok((attempt, Intent::StartTyping(intent, witness)))
    } else {
        checklist::structure_ops::begin_structure(shell, visit, read, section, action, value, now)
            .map(|(a, i)| (a, Intent::Structure(i)))
    }
}
impl Intent {
    pub(super) fn prepare(self, fresh: &CanvasCollabSnapshot, now: u64) -> Result<Prepared, Error> {
        match self {
            Self::Structure(i) => i.prepare(fresh, now).map(Prepared::Structure),
            Self::StartTyping(i, w) => i.prepare(fresh, &w, now).map(Prepared::StartTyping),
        }
    }
}
impl Prepared {
    pub(super) async fn reserve(
        &self,
        state: Signal<ShellState>,
        attempt: &checklist::Attempt,
    ) -> Result<checklist::recovery::Ticket, &'static str> {
        match self {
            Self::Structure(p) => checklist::recovery::reserve(state, attempt, p).await,
            Self::StartTyping(p) => {
                checklist::recovery::reserve_start_typing(state, attempt, p).await
            }
        }
    }
    pub(super) fn mark_dispatched(
        &self,
        shell: &mut ShellState,
        attempt: &checklist::Attempt,
    ) -> bool {
        match self {
            Self::Structure(p) => checklist::structure_ops::mark_dispatched(shell, attempt, p),
            Self::StartTyping(p) => checklist::start_typing_ops::mark_dispatched(shell, attempt, p),
        }
    }
    pub(super) async fn execute(
        self,
        ctx: &crate::scheduled::Context,
        fresh: &CanvasCollabSnapshot,
    ) -> Result<Accepted, CanvasEditFailure> {
        match self {
            Self::Structure(p) => api::execute_canvas_structure_edit(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                fresh,
                p,
            )
            .await
            .map(Accepted::Structure),
            Self::StartTyping(p) => api::execute_canvas_start_typing_edit(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                fresh,
                p,
            )
            .await
            .map(Accepted::StartTyping),
        }
    }
}
impl Accepted {
    pub(super) fn finish(self, shell: &mut ShellState, attempt: &checklist::Attempt) -> bool {
        match self {
            Self::Structure(a) => checklist::structure_ops::finish_structure(shell, attempt, a),
            Self::StartTyping(a) => {
                checklist::start_typing_ops::finish_start_typing(shell, attempt, a)
            }
        }
    }
}
