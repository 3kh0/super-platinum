//! Exclusive workspace session custody for expensive multi-peer text saves.
use super::*;
use super_platinum_core::canvas::edit::text::{
    AcceptedText, PreparedTextEdit, TextDraft, TextIntent,
};
const BUSY: &str = "Wait for the other canvas change to finish before saving.";
const POISONED: &str =
    "This canvas edit session is unavailable. Sign out and sign in again before editing.";
pub(in crate::canvas) fn reason(shell: &ShellState) -> Option<&'static str> {
    let owned = shell
        .canvas_checklist
        .sessions
        .iter()
        .find(|s| s.scope.current(shell))?;
    owned.session.is_none().then_some(if owned.claim.is_some() {
        BUSY
    } else {
        POISONED
    })
}
#[derive(Clone)]
pub(in crate::canvas) struct Claim {
    token: Arc<()>,
    attempt: Attempt,
}
pub(in crate::canvas) struct Lease {
    claim: Claim,
    session: EditSession,
}
pub(in crate::canvas) struct Memo {
    claim: Claim,
    accepted: AcceptedText,
}
impl Claim {
    fn index(&self, shell: &ShellState) -> Option<usize> {
        shell.canvas_checklist.sessions.iter().position(|owned| {
            owned
                .claim
                .as_ref()
                .is_some_and(|token| Arc::ptr_eq(token, &self.token))
        })
    }
    pub(in crate::canvas) fn owns(&self, shell: &ShellState) -> bool {
        self.attempt.scope.account_current(shell) && self.index(shell).is_some()
    }
    fn same_attempt(&self, attempt: &Attempt) -> bool {
        self.attempt.serial == attempt.serial
            && self.attempt.visit == attempt.visit
            && self.attempt.read == attempt.read
            && self.attempt.revision == attempt.revision
            && self.attempt.section == attempt.section
            && Arc::ptr_eq(&self.attempt.snapshot, &attempt.snapshot)
            && matches!((&self.attempt.mutation,&attempt.mutation),
                (Mutation::Text(a),Mutation::Text(b)) if Arc::ptr_eq(a,b))
    }
    pub(in crate::canvas) fn preparing(&self, shell: &ShellState, attempt: &Attempt) -> bool {
        self.owns(shell)
            && self.same_attempt(attempt)
            && dispatchable(shell, attempt)
            && shell.canvas_checklist.operations.iter().any(|op| {
                self.same_attempt(&op.attempt)
                    && op.stage == Stage::Preparing
                    && op.dispatched.is_none()
                    && op.accepted.is_none()
                    && op.dispatched_after_read == attempt.read
            })
    }
    pub(in crate::canvas) fn poison(&self, shell: &mut ShellState) {
        if let Some(index) = self.index(shell) {
            let owned = &mut shell.canvas_checklist.sessions[index];
            owned.session = None;
            owned.claim = None; // The existing slot remains, so it cannot create another actor.
        }
    }
}
impl Lease {
    pub(in crate::canvas) fn claim(&self) -> Claim {
        self.claim.clone()
    }
    pub(in crate::canvas) fn prepare(
        &mut self,
        intent: &TextIntent,
        fresh: &CanvasCollabSnapshot,
        draft: &TextDraft,
        now: u64,
    ) -> Result<PreparedTextEdit, super_platinum_core::canvas::edit::Error> {
        intent.prepare(fresh, &mut self.session, draft, now)
    }
    /// Execute only on a worker. The sealed wrapper records that adoption ran;
    /// a failure keeps core's unusable retained entry, as the legacy path does.
    pub(in crate::canvas) fn remember(&mut self, accepted: AcceptedText) -> Memo {
        let _ = self.session.remember_text_accepted(&accepted);
        Memo {
            claim: self.claim.clone(),
            accepted,
        }
    }
    /// Navigation may be stale while the authenticated session remains valid.
    /// Preserve consumed counters; never restore after credential replacement.
    pub(in crate::canvas) fn restore(self, shell: &mut ShellState) -> bool {
        if !self.claim.owns(shell) {
            return false;
        }
        let index = self.claim.index(shell).unwrap();
        shell.canvas_checklist.sessions[index].session = Some(self.session);
        shell.canvas_checklist.sessions[index].claim = None;
        true
    }
}
impl Memo {
    pub(super) fn into_accepted(self, attempt: &Attempt) -> Option<AcceptedText> {
        self.claim.same_attempt(attempt).then_some(self.accepted)
    }
}
pub(in crate::canvas) fn acquire(
    shell: &mut ShellState,
    attempt: &Attempt,
) -> Result<Lease, &'static str> {
    if !matches!(attempt.mutation, Mutation::Text(_)) || !dispatchable(shell, attempt) {
        return Err("This canvas changed before saving.");
    }
    if let Some(reason) = reason(shell) {
        return Err(reason);
    }
    // Existing same-scope requests may still adopt an ACK. Do not take their
    // session away, and do not require a deferred-memo queue.
    if shell.canvas_checklist.operations.iter().any(|op| {
        op.stage == Stage::Saving
            && op.attempt.scope.team == attempt.scope.team
            && op.attempt.scope.account_current(shell)
    }) {
        return Err(BUSY);
    }
    with_edit_session(shell, attempt, |_| ()).ok_or(BUSY)?;
    let index = shell
        .canvas_checklist
        .sessions
        .iter()
        .position(|s| s.scope.current(shell))
        .ok_or(BUSY)?;
    let owned = &mut shell.canvas_checklist.sessions[index];
    let session = owned.session.take().ok_or(BUSY)?;
    let token = Arc::new(());
    owned.claim = Some(token.clone());
    Ok(Lease {
        claim: Claim {
            token,
            attempt: attempt.clone(),
        },
        session,
    })
}
#[cfg(test)]
mod tests;
