//! Structural attempts share the existing file lock and authenticated Scope.
use super::*;
use super_platinum_core::canvas::edit::{
    structure::{AcceptedStructure, PreparedStructureEdit, Side, StructureIntent},
    text::TextDraft,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::canvas) enum Action {
    Before,
    After,
    Delete,
}
impl Action {
    pub(in crate::canvas) fn capture(
        self,
        snapshot: &CanvasCollabSnapshot,
        anchor: &str,
        draft: &TextDraft,
        now: u64,
    ) -> Result<StructureIntent, super_platinum_core::canvas::edit::Error> {
        match self {
            Self::Before => StructureIntent::insert(snapshot, anchor, Side::Before, draft, now),
            Self::After => StructureIntent::insert(snapshot, anchor, Side::After, draft, now),
            Self::Delete => StructureIntent::delete(snapshot, anchor, now),
        }
    }
}
pub(in crate::canvas) fn begin_structure(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    anchor: &str,
    action: Action,
    draft: &TextDraft,
    now: u64,
) -> Result<(Attempt, StructureIntent), &'static str> {
    let view = presentation(shell)
        .filter(|view| view.visit == visit && view.read == read)
        .ok_or("This canvas view changed. Cancel and refresh.")?;
    let snapshot = view.snapshot.ok_or("Refresh this canvas before editing.")?;
    let intent = action
        .capture(&snapshot, anchor, draft, now)
        .map_err(|_| "This paragraph cannot be changed here. Cancel and refresh.")?;
    let mut attempt = begin(shell, visit, read, anchor, false)?;
    attempt.section = intent.target_section().to_owned();
    attempt.mutation = Mutation::Structure(Arc::new(draft.clone()));
    shell
        .canvas_checklist
        .operations
        .last_mut()
        .expect("begin owns operation")
        .attempt = attempt.clone();
    Ok((attempt, intent))
}
pub(in crate::canvas) fn mark_dispatched(
    shell: &mut ShellState,
    attempt: &Attempt,
    prepared: &PreparedStructureEdit,
) -> bool {
    if !matches!(attempt.mutation, Mutation::Structure(_))
        || !dispatchable(shell, attempt)
        || prepared.workspace_id() != attempt.snapshot.init.workspace_id()
        || prepared.user_id()
            != attempt
                .snapshot
                .init
                .options()
                .string(33)
                .ok()
                .flatten()
                .unwrap_or("")
        || prepared.file_id() != attempt.snapshot.file_id()
        || prepared.thread_id() != attempt.snapshot.thread_id()
        || prepared.document_id() != attempt.snapshot.document_id()
        || prepared.target_section() != attempt.section
    {
        return false;
    }
    let operation = shell
        .canvas_checklist
        .operations
        .iter_mut()
        .find(|op| op.attempt.serial == attempt.serial)
        .unwrap();
    // Retain the exact sent CRDT/head/content projection for read-only recovery
    // even if the consuming transport has no sealed ACK and the editor closes.
    operation.structural_request = Some(prepared.document_data().clone());
    operation.stage = Stage::Saving;
    operation.dispatched = Some((prepared.session_id().into(), prepared.sequence()));
    if let Some(view) = shell.canvas_checklist.view.as_mut() {
        view.stale = true;
    }
    true
}
pub(in crate::canvas) fn finish_structure(
    shell: &mut ShellState,
    attempt: &Attempt,
    accepted: AcceptedStructure,
) -> bool {
    let Some(operation) = shell.canvas_checklist.operations.iter_mut().find(|op| {
        op.attempt.serial == attempt.serial
            && op.stage == Stage::Saving
            && matches!(op.attempt.mutation, Mutation::Structure(_))
    }) else {
        return false;
    };
    let matches = operation
        .dispatched
        .as_ref()
        .is_some_and(|(session, sequence)| {
            session == accepted.session_id() && *sequence == accepted.sequence()
        })
        && accepted.target_section() == attempt.section
        && accepted.section().thread_id().ok().flatten() == Some(attempt.snapshot.thread_id())
        && accepted.section().document_id().ok().flatten() == Some(attempt.snapshot.document_id());
    operation.stage = if matches {
        Stage::AwaitingRead
    } else {
        Stage::Uncertain
    };
    if matches {
        operation.accepted = Some(Accepted::Structure(accepted));
    }
    true
}

#[cfg(test)]
pub(in crate::canvas) fn retained_request(
    shell: &ShellState,
    attempt: &Attempt,
) -> Option<super_platinum_core::canvas::wire::RawMessage> {
    let operation = shell
        .canvas_checklist
        .operations
        .iter()
        .find(|operation| operation.attempt.serial == attempt.serial)?;
    operation.structural_request.clone()
}
