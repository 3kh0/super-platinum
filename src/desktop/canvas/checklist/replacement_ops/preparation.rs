//! Capture the file-wide lock before off-thread history/Inventory validation.
use super::*;
const CHANGED: &str = "This canvas view changed. Cancel and refresh.";
const INVALID: &str = "This paragraph cannot be replaced here. Cancel and refresh.";
pub(in crate::canvas) struct Job {
    attempt: Attempt,
    now: u64,
}
pub(in crate::canvas) struct Ready {
    attempt: Attempt,
    intent: ReplacementIntent,
}
pub(in crate::canvas) fn begin(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    target: &str,
    now: u64,
) -> Result<Job, &'static str> {
    let mut attempt = super::super::begin(shell, visit, read, target, false)?;
    attempt.mutation = Mutation::ReplacementPreparing;
    shell
        .canvas_checklist
        .operations
        .last_mut()
        .expect("begin owns operation")
        .attempt = attempt.clone();
    Ok(Job { attempt, now })
}
impl Job {
    pub(in crate::canvas) fn attempt(&self) -> &Attempt {
        &self.attempt
    }
    pub(in crate::canvas) fn run(self) -> Result<Ready, &'static str> {
        let intent =
            ReplacementIntent::main_canvas(&self.attempt.snapshot, &self.attempt.section, self.now)
                .map_err(|_| INVALID)?;
        Ok(Ready {
            attempt: self.attempt,
            intent,
        })
    }
}
fn current(shell: &ShellState, attempt: &Attempt) -> bool {
    preparation_owner_current(shell, attempt)
        && matches!(attempt.mutation, Mutation::ReplacementPreparing)
        && shell.canvas_checklist.operations.iter().any(|op| {
            op.attempt.serial == attempt.serial
                && op.attempt.visit == attempt.visit
                && op.attempt.read == attempt.read
                && op.attempt.revision == attempt.revision
                && op.attempt.section == attempt.section
                && Arc::ptr_eq(&op.attempt.snapshot, &attempt.snapshot)
                && matches!(op.attempt.mutation, Mutation::ReplacementPreparing)
                && op.stage == Stage::Preparing
                && op.dispatched.is_none()
                && op.accepted.is_none()
                && op.durable.is_none()
                && op.dispatched_after_read == attempt.read
        })
}
impl Ready {
    pub(in crate::canvas) fn publish(
        self,
        shell: &mut ShellState,
    ) -> Result<(Attempt, ReplacementIntent), &'static str> {
        if !current(shell, &self.attempt) {
            return Err(CHANGED);
        }
        let mut attempt = self.attempt;
        attempt.mutation = Mutation::Replacement {
            replacement: self.intent.replacement_section().into(),
        };
        shell
            .canvas_checklist
            .operations
            .iter_mut()
            .find(|op| op.attempt.serial == attempt.serial)
            .expect("current exact operation")
            .attempt = attempt.clone();
        Ok((attempt, self.intent))
    }
}
#[cfg(test)]
mod tests;
