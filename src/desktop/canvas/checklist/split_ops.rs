//! One parent/child split shares the existing file lock and readback custody.
use super::*;
use super_platinum_core::canvas::edit::structure::{
    AcceptedSplit, PreparedSplitEdit, SplitIntent, SplitWitness,
};

pub(in crate::canvas) fn begin_split(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    section: &str,
    witness: SplitWitness,
    now: u64,
) -> Result<(Attempt, SplitIntent), &'static str> {
    let view = presentation(shell)
        .filter(|view| view.visit == visit && view.read == read)
        .ok_or("This canvas view changed. Cancel and refresh.")?;
    let snapshot = view.snapshot.ok_or("Refresh this canvas before editing.")?;
    let intent = SplitIntent::capture(&snapshot, section, now)
        .map_err(|_| "This paragraph cannot be split here. Cancel and refresh.")?;
    let mut attempt = begin(shell, visit, read, section, false)?;
    attempt.mutation = Mutation::Split {
        witness: Arc::new(witness),
        child: None,
    };
    shell
        .canvas_checklist
        .operations
        .last_mut()
        .unwrap()
        .attempt = attempt.clone();
    Ok((attempt, intent))
}

pub(in crate::canvas) fn witness(attempt: &Attempt) -> Option<&SplitWitness> {
    match &attempt.mutation {
        Mutation::Split { witness, .. } => Some(witness),
        _ => None,
    }
}

pub(in crate::canvas) fn mark_dispatched(
    shell: &mut ShellState,
    attempt: &Attempt,
    prepared: &PreparedSplitEdit,
) -> bool {
    if witness(attempt).is_none()
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
    let Mutation::Split { child, .. } = &mut operation.attempt.mutation else {
        return false;
    };
    *child = Some(prepared.child_section().to_owned());
    operation.structural_request = Some(prepared.document_data().clone());
    operation.stage = Stage::Saving;
    operation.dispatched = Some((prepared.session_id().into(), prepared.sequence()));
    if let Some(view) = shell.canvas_checklist.view.as_mut() {
        view.stale = true;
    }
    true
}

pub(in crate::canvas) fn finish_split(
    shell: &mut ShellState,
    attempt: &Attempt,
    accepted: AcceptedSplit,
) -> bool {
    let Some(operation) = shell.canvas_checklist.operations.iter_mut().find(|op| {
        op.attempt.serial == attempt.serial
            && op.stage == Stage::Saving
            && matches!(op.attempt.mutation, Mutation::Split { .. })
    }) else {
        return false;
    };
    let Mutation::Split { child, .. } = &operation.attempt.mutation else {
        return false;
    };
    let matches = operation
        .dispatched
        .as_ref()
        .is_some_and(|(session, sequence)| {
            session == accepted.session_id() && *sequence == accepted.sequence()
        })
        && accepted.target_section() == attempt.section
        && child.as_deref() == Some(accepted.child_id())
        && [accepted.parent_section(), accepted.child_section()]
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
        operation.accepted = Some(Accepted::Split(accepted));
    }
    true
}
