//! One parent/child end shares the existing file lock and readback custody.
use super::*;
use super_platinum_core::canvas::edit::structure::{
    AcceptedEnd, EndIntent, EndWitness, PreparedEndEdit,
};

pub(in crate::canvas) fn begin_end(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    section: &str,
    witness: EndWitness,
    now: u64,
) -> Result<(Attempt, EndIntent), &'static str> {
    let view = presentation(shell)
        .filter(|view| view.visit == visit && view.read == read)
        .ok_or("This canvas view changed. Cancel and refresh.")?;
    let snapshot = view.snapshot.ok_or("Refresh this canvas before editing.")?;
    let intent = if witness.is_sequence() {
        EndIntent::capture_sequence(&snapshot, section, witness.append_count(), now)
    } else if witness.is_repeat() {
        EndIntent::capture_repeat(&snapshot, section, now)
    } else if witness.is_empty_enter() {
        EndIntent::capture_empty(&snapshot, section, now)
    } else {
        EndIntent::capture(&snapshot, section, now)
    }
    .map_err(|_| "An empty paragraph cannot be added here. Cancel and refresh.")?;
    let mut attempt = begin(shell, visit, read, section, false)?;
    attempt.mutation = Mutation::End {
        witness: Arc::new(witness),
        child: None,
        second_child: None,
        appended: Vec::new(),
    };
    shell
        .canvas_checklist
        .operations
        .last_mut()
        .unwrap()
        .attempt = attempt.clone();
    Ok((attempt, intent))
}

pub(in crate::canvas) fn witness(attempt: &Attempt) -> Option<&EndWitness> {
    match &attempt.mutation {
        Mutation::End { witness, .. } => Some(witness),
        _ => None,
    }
}

pub(in crate::canvas) fn mark_dispatched(
    shell: &mut ShellState,
    attempt: &Attempt,
    prepared: &PreparedEndEdit,
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
    let Mutation::End {
        child,
        second_child,
        appended,
        ..
    } = &mut operation.attempt.mutation
    else {
        return false;
    };
    *appended = if prepared.is_sequence() {
        prepared
            .appended_members()
            .map(|m| m.id().to_owned())
            .collect()
    } else {
        Vec::new()
    };
    *child = Some(prepared.child_section().to_owned());
    *second_child = prepared.second_child_section().map(str::to_owned);
    operation.structural_request = Some(prepared.document_data().clone());
    operation.stage = Stage::Saving;
    operation.dispatched = Some((prepared.session_id().into(), prepared.sequence()));
    if let Some(view) = shell.canvas_checklist.view.as_mut() {
        view.stale = true;
    }
    true
}

pub(in crate::canvas) fn finish_end(
    shell: &mut ShellState,
    attempt: &Attempt,
    accepted: AcceptedEnd,
) -> bool {
    let Some(operation) = shell.canvas_checklist.operations.iter_mut().find(|op| {
        op.attempt.serial == attempt.serial
            && op.stage == Stage::Saving
            && matches!(op.attempt.mutation, Mutation::End { .. })
    }) else {
        return false;
    };
    let Mutation::End {
        child,
        second_child,
        appended,
        ..
    } = &operation.attempt.mutation
    else {
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
        && second_child.as_deref() == accepted.second_child_section().map(|s| s.id())
        && (!accepted.is_sequence()
            || (appended
                .iter()
                .map(String::as_str)
                .eq(accepted.appended_sections().map(|s| s.id()))
                && accepted.appended_sections().all(|s| {
                    s.thread_id().ok().flatten() == Some(attempt.snapshot.thread_id())
                        && s.document_id().ok().flatten() == Some(attempt.snapshot.document_id())
                })))
        && [
            Some(accepted.parent_section()),
            Some(accepted.child_section()),
            accepted.second_child_section(),
        ]
        .into_iter()
        .flatten()
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
        operation.accepted = Some(Accepted::End(accepted));
    }
    true
}
