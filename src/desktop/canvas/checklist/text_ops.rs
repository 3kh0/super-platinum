//! Text operations reuse the canonical read, owner, session and receipt boundary.
use super::*;
pub(super) mod readback;
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
    if let Some(reason) = super::session_worker::reason(shell) {
        return Err(reason);
    }
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
    finish(shell, attempt, accepted, false)
}
pub(in crate::canvas) fn finish_text_with_memo(
    shell: &mut ShellState,
    attempt: &Attempt,
    memo: super::session_worker::Memo,
) -> bool {
    let Some(accepted) = memo.into_accepted(attempt) else {
        return false;
    };
    finish(shell, attempt, accepted, true)
}
fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    accepted: AcceptedText,
    memo_done: bool,
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
    if matches && !memo_done && attempt.scope.account_current(shell) {
        if let Some(owned) = shell.canvas_checklist.sessions.iter().position(|owned| {
            owned.scope.team == attempt.scope.team && owned.scope.account_current(shell)
        }) {
            let _ = shell.canvas_checklist.sessions[owned]
                .session
                .as_mut()
                .map(|session| session.remember_text_accepted(&accepted));
        }
    }
    let operation = &mut shell.canvas_checklist.operations[index];
    operation.stage = if matches {
        Stage::AwaitingRead
    } else {
        Stage::Uncertain
    };
    if matches {
        operation.accepted = Some(if accepted.associated_peer_count() > 0 {
            Accepted::TextSequence(readback::AcceptedState::new(accepted))
        } else {
            Accepted::Text(accepted)
        });
    }
    true
}

/// Only the exact saving operation may adopt a disconnected worker completion.
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
                    (Mutation::Text(a), Mutation::Text(b)) if Arc::ptr_eq(a, b))
        })
}
