//! Credential-free journal barrier shared by all Canvas paragraph/checklist UI.
//! Restored receipts are inert locks; a canonical read never settles uncertainty.
use super::*;
use dioxus::prelude::*;
use super_platinum_core::canvas::edit::structure::{
    PreparedStructureEdit,
    recovery::{
        Scope as DiskScope,
        journal::{Begin, Journal, Receipt},
    },
};

pub(in crate::canvas) mod review;

const WAIT: &str = "Checking saved canvas changes before editing…";
const FAILED: &str = "Saved canvas changes could not be checked. Refresh before editing.";
const UNRESOLVED: &str = "An earlier canvas change is unconfirmed. Check this canvas in Slack; it will not be sent again.";

#[derive(Clone, PartialEq, Eq)]
struct Owner {
    account: String,
    scope: DiskScope,
    path: Option<std::path::PathBuf>,
}
impl Owner {
    fn capture(shell: &ShellState, file: &str) -> Option<Self> {
        let workspace = shell.core.active_team.as_ref()?;
        let user = &shell.core.workspaces.get(workspace)?.self_user_id;
        if user != &shell.self_account.user_id {
            return None;
        }
        let path = if crate::fixture::is_fixture() {
            Some(std::env::var_os("SUPER_PLATINUM_CANVAS_RECOVERY_DB")?.into())
        } else {
            None
        };
        Some(Self {
            account: shell.core.active_account.clone()?,
            scope: DiskScope {
                workspace: workspace.clone(),
                user: user.clone(),
                file: file.into(),
            },
            path,
        })
    }
    async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Journal) -> Result<T, super_platinum_core::error::AppError>
        + Send
        + 'static,
    ) -> Result<T, ()> {
        let owner = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut j = match owner.path {
                Some(path) => Journal::open(path, &owner.account),
                None => Journal::open_default(&owner.account),
            }
            .map_err(|_| ())?;
            work(&mut j).map_err(|_| ())
        })
        .await
        .map_err(|_| ())?
    }
}
#[derive(Default)]
pub(super) struct State {
    generation: u64,
    owner: Option<Owner>,
    loaded: bool,
    error: bool,
    receipt: Option<Receipt>,
}
#[derive(Clone)]
pub(in crate::canvas) struct Ticket {
    owner: Owner,
    receipt: Receipt,
}
fn bypass(shell: &ShellState) -> bool {
    (crate::fixture::is_fixture()
        && std::env::var_os("SUPER_PLATINUM_CANVAS_RECOVERY_DB").is_none())
        || (!crate::fixture::is_fixture()
            && shell.core.session.is_none()
            && shell.core.transport.is_none())
}
pub(super) fn reason(shell: &ShellState) -> Option<&'static str> {
    if bypass(shell) {
        return None;
    }
    let file = &shell.canvas.as_ref()?.id;
    let owner = Owner::capture(shell, file);
    shell.canvas_checklist.recovery.reason_for(owner)
}
impl State {
    fn reason_for(&self, owner: Option<Owner>) -> Option<&'static str> {
        if owner.is_none() || self.owner != owner || !self.loaded {
            Some(WAIT)
        } else if self.error {
            Some(FAILED)
        } else if self.receipt.is_some() {
            Some(UNRESOLVED)
        } else {
            None
        }
    }
}
/// Completion of an old file's disk worker must not replace the current file's
/// loaded slot. Original tickets still return to their owner for cleanup.
fn publish_reservation(shell: &mut ShellState, owner: &Owner, result: Result<Receipt, ()>) {
    if shell
        .canvas
        .as_ref()
        .is_none_or(|canvas| canvas.id != owner.scope.file)
        || shell
            .canvas_checklist
            .view
            .as_ref()
            .is_none_or(|view| !view_current(shell, view) || view.file != owner.scope.file)
        || shell.canvas_checklist.recovery.owner.as_ref() != Some(owner)
    {
        return;
    }
    let generation = shell.canvas_checklist.recovery.generation.wrapping_add(1);
    match result {
        Ok(receipt) => {
            shell.canvas_checklist.recovery = State {
                generation,
                owner: Some(owner.clone()),
                loaded: true,
                error: false,
                receipt: Some(receipt),
            }
        }
        Err(()) => {
            let slot = &mut shell.canvas_checklist.recovery;
            slot.generation = generation;
            slot.loaded = true;
            slot.error = true;
        }
    }
}
fn begin_load(shell: &mut ShellState, owner: &Owner) -> u64 {
    let generation = shell.canvas_checklist.recovery.generation.wrapping_add(1);
    shell.canvas_checklist.recovery = State {
        generation,
        owner: Some(owner.clone()),
        ..Default::default()
    };
    generation
}
fn publish_load(
    shell: &mut ShellState,
    read: &Read,
    owner: Owner,
    generation: u64,
    result: Result<Option<Receipt>, ()>,
) {
    if !read.current(shell)
        || shell.canvas_checklist.recovery.owner.as_ref() != Some(&owner)
        || shell.canvas_checklist.recovery.generation != generation
    {
        return;
    }
    shell.canvas_checklist.recovery = match result {
        Ok(receipt) => State {
            generation,
            owner: Some(owner),
            loaded: true,
            error: false,
            receipt,
        },
        Err(()) => State {
            generation,
            owner: Some(owner),
            loaded: true,
            error: true,
            receipt: None,
        },
    };
}
pub(in crate::canvas) async fn load(mut state: Signal<ShellState>, read: &Read) {
    if bypass(&state.peek()) {
        return;
    }
    let owner = {
        let shell = state.peek();
        if !read.current(&shell) {
            return;
        };
        Owner::capture(&shell, &read.file)
    };
    let Some(owner) = owner else { return };
    let generation = begin_load(&mut state.write(), &owner);
    let scope = owner.scope.clone();
    let result = owner.run(move |j| j.get(&scope)).await;
    let mut shell = state.write();
    publish_load(&mut shell, read, owner, generation, result);
}

/// Must complete before mark_dispatched or the consuming transport call.
pub(in crate::canvas) async fn reserve(
    state: Signal<ShellState>,
    attempt: &Attempt,
    prepared: &PreparedStructureEdit,
) -> Result<Ticket, &'static str> {
    reserve_checkpoint(state, attempt, prepared.checkpoint().map_err(|_| FAILED)?).await
}
pub(in crate::canvas) async fn reserve_replacement(
    state: Signal<ShellState>,
    attempt: &Attempt,
    prepared: &super_platinum_core::canvas::edit::structure::PreparedReplacement,
) -> Result<Ticket, &'static str> {
    reserve_checkpoint(state, attempt, prepared.checkpoint().map_err(|_| FAILED)?).await
}
pub(in crate::canvas) async fn reserve_split(
    state: Signal<ShellState>,
    attempt: &Attempt,
    prepared: &super_platinum_core::canvas::edit::structure::PreparedSplitEdit,
) -> Result<Ticket, &'static str> {
    reserve_checkpoint(state, attempt, prepared.checkpoint().map_err(|_| FAILED)?).await
}
pub(in crate::canvas) async fn reserve_end(
    state: Signal<ShellState>,
    attempt: &Attempt,
    prepared: &super_platinum_core::canvas::edit::structure::PreparedEndEdit,
) -> Result<Ticket, &'static str> {
    reserve_checkpoint(state, attempt, prepared.checkpoint().map_err(|_| FAILED)?).await
}
pub(in crate::canvas) async fn reserve_start_typing(
    state: Signal<ShellState>,
    attempt: &Attempt,
    prepared: &super_platinum_core::canvas::edit::structure::PreparedStartTypingEdit,
) -> Result<Ticket, &'static str> {
    reserve_checkpoint(state, attempt, prepared.checkpoint().map_err(|_| FAILED)?).await
}
/// Exact disconnected fixture only: independently read the committed journal
/// through its normal blocking worker, rather than infer durability from UI state.
pub(in crate::canvas) async fn split_fixture_journal(
    state: Signal<ShellState>,
) -> Result<serde_json::Value, &'static str> {
    let owner = {
        let shell = state.peek();
        if !crate::canvas::text::split_fixture::enabled(&shell) {
            return Err(FAILED);
        }
        let file = shell.canvas.as_ref().ok_or(FAILED)?.id.clone();
        Owner::capture(&shell, &file).ok_or(FAILED)?
    };
    let scope = owner.scope.clone();
    owner
        .run(move |journal| {
            let receipt = journal.get(&scope)?;
            Ok(
                serde_json::json!({"scope":scope,"receipt":receipt.map(|receipt|serde_json::json!({
            "operation":receipt.operation_id(),"checkpoint":receipt.checkpoint()}))}),
            )
        })
        .await
        .map_err(|_| FAILED)
}
async fn reserve_checkpoint(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    checkpoint: super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
) -> Result<Ticket, &'static str> {
    let owner = {
        let shell = state.peek();
        if !dispatchable(&shell, attempt) {
            return Err(FAILED);
        }
        Owner::capture(&shell, attempt.snapshot.file_id()).ok_or(FAILED)?
    };
    if checkpoint.scope() != &owner.scope {
        return Err(FAILED);
    }
    let result = owner.run(move |j| j.begin(checkpoint)).await;
    let (receipt, started) = match result {
        Ok(Begin::Started(r)) => (r, true),
        Ok(Begin::Blocked(r)) => (r, false),
        Err(()) => {
            let mut shell = state.write();
            publish_reservation(&mut shell, &owner, Err(()));
            return Err(FAILED);
        }
    };
    let ticket = Ticket {
        owner: owner.clone(),
        receipt: receipt.clone(),
    };
    {
        let mut shell = state.write();
        publish_reservation(&mut shell, &owner, Ok(receipt));
        if started
            && let Some(op) = shell
                .canvas_checklist
                .operations
                .iter_mut()
                .find(|op| op.attempt.serial == attempt.serial)
        {
            op.durable = Some(ticket.clone());
        }
    }
    if started { Ok(ticket) } else { Err(UNRESOLVED) }
}
/// Only use for definite non-dispatch or the core-sealed readback below.
pub(in crate::canvas) async fn complete(mut state: Signal<ShellState>, ticket: &Ticket) -> bool {
    let fence = retirement_fence(&state.peek(), ticket);
    let receipt = ticket.receipt.clone();
    if ticket.owner.run(move |j| j.complete(&receipt)).await != Ok(true) {
        return false;
    }
    let mut shell = state.write();
    publish_retirement(&mut shell, ticket, fence);
    true
}
fn retirement_fence(shell: &ShellState, ticket: &Ticket) -> Option<u64> {
    let slot = &shell.canvas_checklist.recovery;
    (slot.owner.as_ref() == Some(&ticket.owner) && !slot.error).then_some(slot.generation)
}
fn publish_retirement(shell: &mut ShellState, ticket: &Ticket, fence: Option<u64>) {
    let disk = &mut shell.canvas_checklist.recovery;
    if fence == Some(disk.generation)
        && disk.owner.as_ref() == Some(&ticket.owner)
        && !disk.error
        && (disk
            .receipt
            .as_ref()
            .is_some_and(|r| r.operation_id() == ticket.receipt.operation_id())
            || (disk.receipt.is_none() && !disk.loaded))
    {
        disk.generation = disk.generation.wrapping_add(1);
        disk.loaded = true;
        disk.error = false;
        disk.receipt = None;
    }
}
pub(in crate::canvas) async fn settle(mut state: Signal<ShellState>, read: &Read) {
    let pending = {
        let mut shell = state.write();
        if !read.current(&shell) {
            return;
        }
        let Some(op) = shell
            .canvas_checklist
            .operations
            .iter_mut()
            .find(|op| op.durable_verified == Some(read.serial) && !op.durable_finishing)
        else {
            return;
        };
        let Some(ticket) = op.durable.clone() else {
            return;
        };
        op.durable_finishing = true;
        (op.attempt.clone(), ticket)
    };
    let done = complete(state, &pending.1).await;
    let mut shell = state.write();
    finish_retirement(&mut shell, &pending.0, done);
}
fn finish_retirement(shell: &mut ShellState, attempt: &Attempt, done: bool) {
    let verified = if let Some(op) = shell
        .canvas_checklist
        .operations
        .iter_mut()
        .find(|op| op.attempt.serial == attempt.serial)
    {
        op.durable_finishing = false;
        if done {
            op.durable = None;
            op.durable_verified.take()
        } else {
            None
        }
    } else {
        None
    };
    // A newer canonical read can finish while the disk worker is retiring R1.
    // Use only a read already independently verified by confirm_readback, and
    // re-run that full proof under its current visit/revision before unlocking.
    // An unverified read, stale event, or replacement visit grants no authority.
    let current = shell
        .canvas_checklist
        .view
        .as_ref()
        .filter(|view| verified == Some(view.read) && !view.stale && !view.loading)
        .map(|view| Read {
            scope: view.scope.clone(),
            file: view.file.clone(),
            visit: view.generation,
            serial: view.read,
            revision: view.revision,
        });
    if let Some(read) = current {
        confirm_readback(shell, attempt, &read);
    }
}

#[cfg(test)]
pub(super) mod tests;
