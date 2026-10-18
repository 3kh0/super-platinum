//! Resolve/reopen shares Canvas shell custody, Scope and its account journal.
//! Unknown delivery never becomes a new request merely because a view reopens.
use super::*;
use std::path::PathBuf;
use super_platinum_core::{
    canvas::{
        comments::archive::{ArchiveAccepted, ArchiveIntent},
        edit::structure::recovery::{
            Scope as DiskScope,
            journal::{CommentBegin, CommentReceipt, Journal},
        },
    },
    slack::{models::File, transport::CanvasEditFailure},
};
pub(super) mod check;
pub(in crate::canvas) mod fixture;
pub(super) mod review;
mod run;
#[cfg(test)]
mod tests;
const LOCKED: &str = "This thread change is unconfirmed. It will not be sent again. Review the current thread before removing its local lock.";
const STORAGE: &str =
    "The local comment receipt could not be checked. Refresh comments before changing a thread.";
#[derive(Default)]
pub(in crate::canvas) struct State {
    serial: u64,
    operations: Vec<Operation>,
}
pub(super) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    let old = &mut previous.canvas_checklist.comment_operations;
    let next = &mut loaded.canvas_checklist.comment_operations;
    // Fresh loaded roots start at zero. Keep the original allocation counter so
    // a late completion cannot alias a newly allocated attempt after replacement.
    next.serial = next.serial.max(old.serial);
    next.operations.append(&mut old.operations);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Preparing,
    Sending,
    AwaitingRead,
    Uncertain,
    NotSent,
}
struct Operation {
    attempt: Attempt,
    stage: Stage,
    receipt: Option<CommentReceipt>,
    accepted: Option<std::sync::Arc<ArchiveAccepted>>,
}
#[derive(Clone)]
struct Attempt {
    owner: Owner,
    serial: u64,
    read: u64,
    intent: ArchiveIntent,
    disk: Disk,
}
#[derive(Clone, PartialEq, Eq)]
struct Disk {
    account: String,
    scope: DiskScope,
    path: Option<PathBuf>,
}
impl Disk {
    fn capture(shell: &ShellState, owner: &Owner) -> Option<Self> {
        if !owner.current(shell) {
            return None;
        }
        let account = shell.core.active_account.clone()?;
        uuid::Uuid::parse_str(&account).ok()?;
        let user = shell
            .core
            .workspaces
            .get(&owner.scope.team)?
            .self_user_id
            .clone();
        let path = if crate::fixture::is_fixture() {
            Some(fixture::path(shell)?)
        } else {
            None
        };
        Some(Self {
            account,
            scope: DiskScope {
                workspace: owner.scope.team.clone(),
                user,
                file: owner.file.clone(),
            },
            path,
        })
    }
    async fn work<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Journal) -> Result<T, super_platinum_core::error::AppError>
        + Send
        + 'static,
    ) -> Result<T, ()> {
        let owner = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut journal = match owner.path {
                Some(path) => Journal::open(path, &owner.account),
                None => Journal::open_default(&owner.account),
            }
            .map_err(|_| ())?;
            work(&mut journal).map_err(|_| ())
        })
        .await
        .map_err(|_| ())?
    }
}
fn index(shell: &ShellState, owner: &Owner) -> Option<usize> {
    shell
        .canvas_checklist
        .comment_operations
        .operations
        .iter()
        .position(|operation| {
            operation.attempt.owner.file == owner.file
                && operation.attempt.owner.scope.stable_current(shell)
        })
}
pub(in crate::canvas) fn pending_for_current_file(shell: &ShellState) -> bool {
    Owner::capture(shell).is_some_and(|owner| index(shell, &owner).is_some())
}
pub(in crate::canvas) fn retained_count(shell: &ShellState) -> (usize, usize) {
    let operations = &shell.canvas_checklist.comment_operations.operations;
    (
        operations
            .iter()
            .filter(|operation| operation.attempt.owner.scope.stable_current(shell))
            .count(),
        operations.len(),
    )
}
pub(super) async fn load(
    state: Signal<ShellState>,
    mut model: Signal<super::State>,
    ticket: &Ticket,
) {
    let owner = &ticket.owner;
    let disk = Disk::capture(&state.peek(), owner);
    let Some(disk) = disk else {
        // Ordinary static fixtures never acquire a database or mutation authority.
        if model.peek().accepts(&state.peek(), ticket) {
            model.write().mutation_ready = false;
        }
        return;
    };
    let scope = disk.scope.clone();
    let result = disk.work(move |journal| journal.comment(&scope)).await;
    if !model.peek().accepts(&state.peek(), ticket) {
        return;
    }
    let mut current = model.write();
    match result {
        Ok(receipt) => {
            current.mutation_receipt = receipt;
            current.mutation_ready = true;
            current.mutation_error = None;
        }
        Err(()) => {
            current.mutation_ready = false;
            current.mutation_error = Some(STORAGE);
        }
    }
}
pub(super) fn reason(shell: &ShellState, model: &super::State) -> Option<&'static str> {
    if !model.mutation_ready {
        return Some(STORAGE);
    }
    if model.mutation_receipt.is_some() {
        return Some(LOCKED);
    }
    if model.error.is_some() {
        return Some("Refresh comments before changing a thread.");
    }
    let (local, total) = retained_count(shell);
    let (documents, document_total) = super::super::checklist::document_operation_count(shell);
    if local + documents >= 8 || total + document_total >= 32 {
        return Some("Earlier canvas changes must be reviewed before starting another change.");
    }
    let owner = model.owner.as_ref()?;
    if index(shell, owner).is_some() {
        return Some("The thread change is being checked.");
    }
    if super::super::checklist::has_pending_write(shell)
        || super::reply::pending_for_current_file(shell)
    {
        return Some("A canvas change must be checked before changing a thread.");
    }
    None
}
/// A fresh, already adopted read may finish an ACK-backed operation or retry
/// local removal of a definitely unsent receipt. It never retries a mutation.
pub(super) async fn after_read(mut state: Signal<ShellState>, mut model: Signal<super::State>) {
    let pending = {
        let shell = state.peek();
        let value = model.peek();
        let Some(owner) = value.owner.as_ref().filter(|owner| owner.current(&shell)) else {
            return;
        };
        if value.loading || value.error.is_some() {
            return;
        }
        let Some(index) = index(&shell, owner) else {
            return;
        };
        let operation = &shell.canvas_checklist.comment_operations.operations[index];
        let Some(receipt) = operation.receipt.clone() else {
            return;
        };
        let proven = operation.stage == Stage::NotSent
            || (operation.stage == Stage::AwaitingRead
                && operation.accepted.as_ref().is_some_and(|accepted| {
                    value.file.as_ref().is_some_and(|file| {
                        value
                            .rows
                            .iter()
                            .any(|row| accepted.readback(file, &row.message).is_ok())
                    })
                }));
        if !proven {
            return;
        }
        (operation.attempt.clone(), receipt, value.serial)
    };
    let receipt = pending.1.clone();
    let done = pending
        .0
        .disk
        .work(move |journal| journal.complete_comment(&receipt))
        .await;
    if done == Ok(true) {
        retire(&mut state.write(), &pending.0);
        if pending.0.owner.scope.current(&state.peek()) && model.peek().serial == pending.2 {
            let mut value = model.write();
            if value.mutation_receipt.as_ref() == Some(&pending.1) {
                value.mutation_receipt = None;
            }
        }
    }
}
pub(super) fn choose(
    mut state: Signal<ShellState>,
    model: Signal<super::State>,
    root: Message,
    expected_read: u64,
) {
    let attempt =
        {
            let shell = state.peek();
            let value = model.peek();
            if value.serial != expected_read
                || value.loading
                || value.cooling()
                || reason(&shell, &value).is_some()
            {
                return;
            }
            let Some(owner) = value
                .owner
                .as_ref()
                .filter(|owner| owner.current(&shell))
                .cloned()
            else {
                return;
            };
            // The exact root must still be displayed, not an old retained DOM event.
            if !value.rows.iter().any(|row| {
                serde_json::to_value(&row.message).ok() == serde_json::to_value(&root).ok()
            }) {
                return;
            }
            let Some(file) = value.file.as_ref() else {
                return;
            };
            let Ok(intent) = ArchiveIntent::capture(file, &root) else {
                return;
            };
            let Some(disk) = Disk::capture(&shell, &owner) else {
                return;
            };
            let (local, total) = retained_count(&shell);
            let (document_local, document_total) =
                super::super::checklist::document_operation_count(&shell);
            if local + document_local >= 8 || total + document_total >= 32 {
                return;
            }
            Attempt {
                owner,
                serial: 0,
                read: expected_read,
                intent,
                disk,
            }
        };
    let attempt = {
        let mut shell = state.write();
        let custody = &mut shell.canvas_checklist.comment_operations;
        custody.serial = custody.serial.wrapping_add(1);
        let attempt = Attempt {
            serial: custody.serial,
            ..attempt
        };
        custody.operations.push(Operation {
            attempt: attempt.clone(),
            stage: Stage::Preparing,
            receipt: None,
            accepted: None,
        });
        attempt
    };
    dioxus::core::spawn_forever(run::execute(state, model, attempt));
}
fn owns(shell: &ShellState, attempt: &Attempt, stage: Stage) -> bool {
    shell
        .canvas_checklist
        .comment_operations
        .operations
        .iter()
        .any(|operation| {
            operation.attempt.serial == attempt.serial
                && operation.attempt.disk == attempt.disk
                && operation.stage == stage
        })
}
fn current(shell: &ShellState, model: &super::State, attempt: &Attempt) -> bool {
    attempt.owner.current(shell)
        && model.serial == attempt.read
        && model.owner.as_ref().is_some_and(|owner| {
            owner.visit == attempt.owner.visit && owner.file == attempt.owner.file
        })
        && owns(shell, attempt, Stage::Preparing)
}
fn stage(shell: &mut ShellState, attempt: &Attempt, from: Stage, to: Stage) -> bool {
    let Some(operation) = shell
        .canvas_checklist
        .comment_operations
        .operations
        .iter_mut()
        .find(|operation| {
            operation.attempt.serial == attempt.serial
                && operation.attempt.disk == attempt.disk
                && operation.stage == from
        })
    else {
        return false;
    };
    operation.stage = to;
    true
}
fn retire(shell: &mut ShellState, attempt: &Attempt) {
    shell
        .canvas_checklist
        .comment_operations
        .operations
        .retain(|operation| {
            operation.attempt.serial != attempt.serial || operation.attempt.disk != attempt.disk
        });
}
fn feedback(
    state: Signal<ShellState>,
    mut model: Signal<super::State>,
    attempt: &Attempt,
    error: Option<&'static str>,
) {
    if attempt.owner.current(&state.peek()) && model.peek().serial == attempt.read {
        let mut current = model.write();
        current.error = error;
        if error == Some(STORAGE) {
            current.mutation_ready = false;
            current.mutation_error = Some(STORAGE);
        }
    }
}
