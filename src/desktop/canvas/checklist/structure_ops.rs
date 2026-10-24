//! Structural attempts share the existing file lock and authenticated Scope.
use super::*;
pub(super) mod readback;
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
        operation.accepted = Some(
            if attempt
                .snapshot
                .sections
                .iter()
                .any(|row| row.crdt().ok().flatten().is_some_and(|w| w.has(10)))
                || accepted
                    .section()
                    .crdt()
                    .ok()
                    .flatten()
                    .is_some_and(|wire| wire.has(10))
            {
                Accepted::StructureWorker(readback::AcceptedState::new(accepted))
            } else {
                Accepted::Structure(accepted)
            },
        );
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

/// Reserve serial custody before pure owner-delete capture; this grants no core edit authority.
pub(in crate::canvas) fn begin_owner_delete(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    section: &str,
    draft: Arc<super_platinum_core::canvas::edit::text::TextDraft>,
) -> Result<Attempt, &'static str> {
    let mut attempt = begin(shell, visit, read, section, false)?;
    attempt.mutation = Mutation::Structure(draft);
    shell
        .canvas_checklist
        .operations
        .last_mut()
        .unwrap()
        .attempt = attempt.clone();
    Ok(attempt)
}
pub(in crate::canvas) fn preparation_current(shell: &ShellState, attempt: &Attempt) -> bool {
    dispatchable(shell, attempt) && shell.canvas_checklist.operations.iter().any(|op| {
        op.attempt.serial == attempt.serial && op.attempt.scope.current(shell)
        && op.attempt.visit == attempt.visit && op.attempt.read == attempt.read
        && op.attempt.revision == attempt.revision && op.attempt.section == attempt.section
        && Arc::ptr_eq(&op.attempt.snapshot, &attempt.snapshot)
        && op.stage == Stage::Preparing && op.dispatched.is_none() && op.accepted.is_none()
        && op.dispatched_after_read == attempt.read
        && matches!((&op.attempt.mutation,&attempt.mutation),(Mutation::Structure(a),Mutation::Structure(b)) if Arc::ptr_eq(a,b))
    })
}
pub(in crate::canvas) fn fixture_completion_current(
    shell: &ShellState,
    attempt: &Attempt,
    session_id: &str,
    sequence: u64,
) -> bool {
    attempt.scope.current(shell)
        && shell.canvas_checklist.view.as_ref().is_some_and(|view| {
            view_current(shell, view)
                && view.generation == attempt.visit
                && view.read == attempt.read
                && view.revision == attempt.revision
                && !view.loading
                && view.file == attempt.snapshot.file_id()
                && view
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| Arc::ptr_eq(snapshot, &attempt.snapshot))
        })
        && shell.canvas_checklist.operations.iter().any(|op| {
            op.attempt.serial == attempt.serial
                && op.attempt.visit == attempt.visit
                && op.attempt.read == attempt.read
                && op.attempt.revision == attempt.revision
                && op.attempt.section == attempt.section
                && op.attempt.scope.current(shell)
                && op.stage == Stage::Saving
                && op.accepted.is_none()
                && op.dispatched_after_read == attempt.read
                && op
                    .dispatched
                    .as_ref()
                    .is_some_and(|(id, serial)| id == session_id && *serial == sequence)
                && Arc::ptr_eq(&op.attempt.snapshot, &attempt.snapshot)
                && matches!((&op.attempt.mutation, &attempt.mutation),
                    (Mutation::Structure(a), Mutation::Structure(b)) if Arc::ptr_eq(a, b))
        })
}

/// The exact source/draft and prepared request stay inseparable through publication.
/// Only the pure worker can construct this result; the renderer cannot retarget it.
pub(in crate::canvas) struct PreparedTarget {
    origin: Attempt,
    prepared: PreparedStructureEdit,
    checkpoint: super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
}
impl PreparedTarget {
    pub(in crate::canvas) fn capture(
        origin: Attempt,
        fresh: &CanvasCollabSnapshot,
        action: Action,
        now: u64,
    ) -> Result<Self, super_platinum_core::canvas::edit::Error> {
        let Mutation::Structure(draft) = &origin.mutation else {
            return Err(super_platinum_core::canvas::edit::Error::Unsupported);
        };
        let intent = action.capture(&origin.snapshot, &origin.section, draft, now)?;
        let prepared = intent.prepare(fresh, now)?;
        let checkpoint = prepared.checkpoint()?;
        Ok(Self {
            origin,
            prepared,
            checkpoint,
        })
    }
    pub(in crate::canvas) fn publish(
        self,
        shell: &mut ShellState,
    ) -> Option<(
        Attempt,
        PreparedStructureEdit,
        super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
    )> {
        let Self {
            origin,
            prepared,
            checkpoint,
        } = self;
        if !preparation_current(shell, &origin)
            || prepared.workspace_id() != origin.snapshot.init.workspace_id()
            || prepared.user_id()
                != origin
                    .snapshot
                    .init
                    .options()
                    .string(33)
                    .ok()
                    .flatten()
                    .unwrap_or("")
            || prepared.file_id() != origin.snapshot.file_id()
            || prepared.thread_id() != origin.snapshot.thread_id()
            || prepared.document_id() != origin.snapshot.document_id()
            || checkpoint.target_section() != prepared.target_section()
            || (prepared.target_section() != origin.section
                && origin.snapshot.section(prepared.target_section()).is_some())
        {
            return None;
        }
        let op = shell
            .canvas_checklist
            .operations
            .iter_mut()
            .find(|o| o.attempt.serial == origin.serial)?;
        if op.durable.is_some() || op.structural_request.is_some() {
            return None;
        }
        let mut bound = origin;
        bound.section = prepared.target_section().to_owned();
        op.attempt = bound.clone();
        Some((bound, prepared, checkpoint))
    }
}
#[cfg(test)]
#[path = "structure_ops/prepare_tests.rs"]
mod prepare_tests;
