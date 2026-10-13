//! Text operations reuse the canonical read, owner, session and receipt boundary.
use super::*;
use super_platinum_core::canvas::edit::text::{AcceptedText, PreparedTextEdit, TextDraft};

pub(in crate::canvas) fn settled(shell: &ShellState, attempt: &Attempt) -> bool {
    attempt.scope.stable_current(shell)
        && shell.canvas_checklist.view.as_ref().is_some_and(|view| {
            view_current(shell, view)
                && view.generation == attempt.visit
                && view.settled_text == Some(attempt.serial)
        })
}

pub(in crate::canvas) fn begin_text(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    section: &str,
    draft: Arc<TextDraft>,
) -> Result<Attempt, &'static str> {
    let mut attempt = begin(shell, visit, read, section, false)?;
    attempt.mutation = Mutation::Text(draft);
    let operation = shell
        .canvas_checklist
        .operations
        .last_mut()
        .expect("begin retained operation");
    operation.attempt = attempt.clone();
    Ok(attempt)
}

pub(in crate::canvas) fn draft(attempt: &Attempt) -> Option<&TextDraft> {
    match &attempt.mutation {
        Mutation::Text(draft) => Some(draft),
        _ => None,
    }
}

pub(in crate::canvas) fn mark_dispatched(
    shell: &mut ShellState,
    attempt: &Attempt,
    prepared: &PreparedTextEdit,
) -> bool {
    if draft(attempt).is_none()
        || !dispatchable(shell, attempt)
        || prepared.file_id() != attempt.snapshot.file_id()
        || prepared.thread_id() != attempt.snapshot.thread_id()
        || prepared.document_id() != attempt.snapshot.document_id()
        || prepared.section_id() != attempt.section
    {
        return false;
    }
    let operation = shell
        .canvas_checklist
        .operations
        .iter_mut()
        .find(|operation| operation.attempt.serial == attempt.serial)
        .unwrap();
    operation.stage = Stage::Saving;
    operation.dispatched = Some((prepared.session_id().into(), prepared.sequence()));
    if let Some(view) = shell.canvas_checklist.view.as_mut() {
        view.stale = true;
    }
    true
}

pub(in crate::canvas) fn finish_text(
    shell: &mut ShellState,
    attempt: &Attempt,
    accepted: AcceptedText,
) -> bool {
    let Some(index) = shell
        .canvas_checklist
        .operations
        .iter()
        .position(|operation| {
            operation.attempt.serial == attempt.serial
                && operation.stage == Stage::Saving
                && matches!(operation.attempt.mutation, Mutation::Text(_))
        })
    else {
        return false;
    };
    let operation = &shell.canvas_checklist.operations[index];
    let section = accepted.section();
    let matches = operation
        .dispatched
        .as_ref()
        .is_some_and(|(session, sequence)| {
            session == accepted.session_id() && *sequence == accepted.sequence()
        })
        && section.id() == attempt.section
        && section.thread_id().ok().flatten() == Some(attempt.snapshot.thread_id())
        && section.document_id().ok().flatten() == Some(attempt.snapshot.document_id());
    if matches && attempt.scope.account_current(shell) {
        if let Some(owned) = shell.canvas_checklist.sessions.iter().position(|owned| {
            owned.scope.team == attempt.scope.team && owned.scope.account_current(shell)
        }) {
            let _ = shell.canvas_checklist.sessions[owned]
                .session
                .remember_text_accepted(&accepted);
        }
    }
    let operation = &mut shell.canvas_checklist.operations[index];
    operation.stage = if matches {
        Stage::AwaitingRead
    } else {
        Stage::Uncertain
    };
    if matches {
        operation.accepted = Some(Accepted::Text(accepted));
    }
    true
}
