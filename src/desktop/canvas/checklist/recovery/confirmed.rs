//! Confirmed replacement storage shares the existing journal worker and fences.
use super::*;
use super_platinum_core::canvas::edit::structure::{
    ConfirmedReplacement, recovery::journal::ReplacementRestore,
};

/// A component-owned read may disappear during SQLite commit. Retain this
/// already-authorized transaction, not its view authority; publication still
/// requires the exact ticket and independently current verified read.
pub(super) async fn settle(
    mut state: Signal<ShellState>,
    attempt: Attempt,
    ticket: Ticket,
    token: Arc<ConfirmedReplacement>,
) {
    let (send, receive) = tokio::sync::oneshot::channel();
    dioxus::core::spawn_forever(async move {
        let done = complete(state, &ticket, token).await;
        finish(&mut state.write(), &attempt, &ticket, done);
        let _ = send.send(());
    });
    let _ = receive.await;
}

pub(super) async fn complete(
    mut state: Signal<ShellState>,
    ticket: &Ticket,
    token: Arc<ConfirmedReplacement>,
) -> bool {
    cancellation_fixture::pause(state, ticket, "precommit").await;
    let fence = retirement_fence(&state.peek(), ticket);
    let receipt = ticket.receipt.clone();
    if ticket
        .owner
        .run(move |j| j.complete_replacement(&receipt, &token))
        .await
        != Ok(true)
    {
        return false;
    }
    cancellation_fixture::pause(state, ticket, "postcommit").await;
    publish_retirement(&mut state.write(), ticket, fence);
    true
}

/// Atomic completion retires this exact operation even if its Reader was closed.
/// Removing finished custody is not permission to publish an old view snapshot.
pub(in crate::canvas::checklist) fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    ticket: &Ticket,
    done: bool,
) -> bool {
    let exact = shell
        .canvas_checklist
        .operations
        .iter()
        .find(|op| op.attempt.serial == attempt.serial)
        .and_then(|op| op.durable.as_ref())
        .is_some_and(|old| old.same(ticket));
    if !exact || (done && !mark_retired(shell, attempt, ticket)) {
        return false;
    }
    finish_retirement(shell, attempt, done);
    if done {
        // No await intervenes after the exact ticket and sealed proof checks.
        shell
            .canvas_checklist
            .operations
            .retain(|op| op.attempt.serial != attempt.serial);
    }
    true
}

pub(in crate::canvas::checklist) fn mark_retired(
    shell: &mut ShellState,
    attempt: &Attempt,
    ticket: &Ticket,
) -> bool {
    let Some(op) = shell
        .canvas_checklist
        .operations
        .iter_mut()
        .find(|op| op.attempt.serial == attempt.serial)
    else {
        return false;
    };
    if !op.durable.as_ref().is_some_and(|t| t.same(ticket)) {
        return false;
    }
    if let Some(Accepted::Replacement(accepted)) = op.accepted.as_mut() {
        accepted.retired(ticket)
    } else {
        false
    }
}

struct RestoreRead {
    owner: Owner,
    generation: u64,
    read: Read,
    previous: Option<Arc<CanvasCollabSnapshot>>,
}
impl RestoreRead {
    fn capture(shell: &ShellState, read: &Read) -> Option<Self> {
        let owner = Owner::capture(shell, &read.file)?;
        let slot = &shell.canvas_checklist.recovery;
        if !read.current(shell) || slot.owner.as_ref() != Some(&owner) || !slot.loaded || slot.error
        {
            return None;
        }
        Some(Self {
            owner,
            generation: slot.generation,
            read: read.clone(),
            previous: shell.canvas_checklist.view.as_ref()?.snapshot.clone(),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.read.current(shell)
            && Owner::capture(shell, &self.read.file).as_ref() == Some(&self.owner)
            && shell.canvas_checklist.recovery.owner.as_ref() == Some(&self.owner)
            && shell.canvas_checklist.recovery.generation == self.generation
            && shell.canvas_checklist.recovery.loaded
            && !shell.canvas_checklist.recovery.error
            && shell.canvas_checklist.view.as_ref().is_some_and(|v| {
                match (&v.snapshot, &self.previous) {
                    (None, None) => true,
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    _ => false,
                }
            })
    }
}

/// Every canonical read first carries any live proof, then consults durable
/// authority. Blocked/Absent preserves that candidate, including pending pairs.
pub(in crate::canvas::checklist) async fn restore(
    state: Signal<ShellState>,
    read: &Read,
    fresh: Arc<CanvasCollabSnapshot>,
) -> Result<Arc<CanvasCollabSnapshot>, &'static str> {
    let captured = {
        let shell = state.peek();
        if bypass(&shell) {
            return Ok(fresh);
        }
        RestoreRead::capture(&shell, read).ok_or(FAILED)?
    };
    let raw = fresh.clone();
    let scope = captured.owner.scope.clone();
    let restored = captured
        .owner
        .run(move |j| {
            j.restore_replacement(&scope, &raw)
                .map(|result| match result {
                    ReplacementRestore::Restored(snapshot) => Arc::new(snapshot),
                    // Pending receipts remain visible inert locks; they never mint
                    // authority or turn a readable unknown result into a read failure.
                    ReplacementRestore::Blocked | ReplacementRestore::Absent => raw,
                })
        })
        .await
        .map_err(|_| "Saved canvas history changed. Check it in Slack before editing.")?;
    if !captured.current(&state.peek()) {
        return Err("This canvas read changed.");
    }
    Ok(restored)
}

#[cfg(test)]
pub(in crate::canvas::checklist) mod tests;
