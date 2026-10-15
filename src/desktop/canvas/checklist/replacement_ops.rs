//! Main Canvas replacement shares the exact account/view/file-wide edit lock.
use super::*;
use super_platinum_core::canvas::edit::structure::{
    AcceptedReplacement, PreparedReplacement, ReplacementIntent,
};

pub(in crate::canvas) fn begin_replacement(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    target: &str,
    now: u64,
) -> Result<(Attempt, ReplacementIntent), &'static str> {
    // presentation is owned only by Overlay::Canvas, never preview/flexpane.
    let view = presentation(shell)
        .filter(|view| view.visit == visit && view.read == read)
        .ok_or("This canvas view changed. Cancel and refresh.")?;
    let snapshot = view.snapshot.ok_or("Refresh this canvas before editing.")?;
    let intent = ReplacementIntent::main_canvas(&snapshot, target, now)
        .map_err(|_| "This paragraph cannot be replaced here. Cancel and refresh.")?;
    let mut attempt = begin(shell, visit, read, target, false)?;
    attempt.mutation = Mutation::Replacement {
        replacement: intent.replacement_section().into(),
    };
    shell
        .canvas_checklist
        .operations
        .last_mut()
        .expect("begin owns operation")
        .attempt = attempt.clone();
    Ok((attempt, intent))
}
pub(in crate::canvas) fn replacement(attempt: &Attempt) -> Option<&str> {
    match &attempt.mutation {
        Mutation::Replacement { replacement } => Some(replacement),
        _ => None,
    }
}
pub(in crate::canvas) fn mark_dispatched(
    shell: &mut ShellState,
    attempt: &Attempt,
    prepared: &PreparedReplacement,
) -> bool {
    if replacement(attempt) != Some(prepared.replacement_section())
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
    operation.structural_request = Some(prepared.document_data().clone());
    operation.stage = Stage::Saving;
    operation.dispatched = Some((prepared.session_id().into(), prepared.sequence()));
    if let Some(view) = shell.canvas_checklist.view.as_mut() {
        view.stale = true;
    }
    true
}
pub(in crate::canvas) fn finish_replacement(
    shell: &mut ShellState,
    attempt: &Attempt,
    accepted: AcceptedReplacement,
) -> bool {
    let Some(operation) = shell.canvas_checklist.operations.iter_mut().find(|op| {
        op.attempt.serial == attempt.serial
            && op.stage == Stage::Saving
            && matches!(op.attempt.mutation, Mutation::Replacement { .. })
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
        && replacement(attempt) == Some(accepted.replacement_section())
        && [accepted.deleted_section(), accepted.inserted_section()]
            .iter()
            .all(|section| {
                section.thread_id().ok().flatten() == Some(attempt.snapshot.thread_id())
                    && section.document_id().ok().flatten() == Some(attempt.snapshot.document_id())
            });
    operation.stage = if matches {
        Stage::AwaitingRead
    } else {
        Stage::Uncertain
    };
    if matches {
        operation.accepted = Some(Accepted::Replacement(accepted));
    }
    true
}
