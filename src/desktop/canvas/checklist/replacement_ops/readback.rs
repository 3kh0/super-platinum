//! Private replacement provenance is produced on a worker, then published only
//! into the exact canonical read that supplied the proof. No raw-row shortcut.
use super::*;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::canvas::edit::structure::ConfirmedReplacement;

pub(in crate::canvas::checklist) struct AcceptedState {
    value: Arc<AcceptedReplacement>,
    proof: Option<Proof>,
}
impl AcceptedState {
    pub(super) fn new(value: AcceptedReplacement) -> Self {
        Self {
            value: Arc::new(value),
            proof: None,
        }
    }
    pub(in crate::canvas::checklist) fn confirms(
        &self,
        shell: &ShellState,
        snapshot: &Arc<CanvasCollabSnapshot>,
        attempt: &Attempt,
        read: &Read,
        operation: &Operation,
    ) -> bool {
        self.proof.as_ref().is_some_and(|p| {
            p.job.read.current(shell)
                && p.job.attempt.scope.stable_current(shell)
                && Arc::ptr_eq(&p.certified, snapshot)
                && p.job
                    .matches(&self.value, attempt, read, operation, p.retired)
        })
    }
}
fn ticket_matches(
    captured: Option<&recovery::Ticket>,
    current: Option<&recovery::Ticket>,
    retired: bool,
) -> bool {
    match (captured, current) {
        (Some(a), Some(b)) => a.same(b),
        (None, None) => true,
        (Some(_), None) => retired,
        _ => false,
    }
}
impl AcceptedState {
    pub(in crate::canvas::checklist) fn confirmed(
        &self,
        shell: &ShellState,
        read: &Read,
        operation: &Operation,
    ) -> Option<Arc<ConfirmedReplacement>> {
        let proof = self.proof.as_ref()?;
        let snapshot = shell.canvas_checklist.view.as_ref()?.snapshot.as_ref()?;
        if !self.confirms(shell, snapshot, &operation.attempt, read, operation) {
            return None;
        }
        proof.confirmed.clone()
    }
    pub(in crate::canvas::checklist) fn retired(&mut self, ticket: &recovery::Ticket) -> bool {
        let Some(proof) = self.proof.as_mut() else {
            return false;
        };
        if !proof
            .job
            .ticket
            .as_ref()
            .is_some_and(|old| old.same(ticket))
            || proof.confirmed.is_none()
        {
            return false;
        }
        proof.retired = true;
        true
    }
}
struct Job {
    accepted: Arc<AcceptedReplacement>,
    source: Arc<CanvasCollabSnapshot>,
    attempt: Attempt,
    read: Read,
    dispatched: Option<(String, u64)>,
    dispatched_after_read: u64,
    ticket: Option<recovery::Ticket>,
}
struct Proof {
    job: Job,
    certified: Arc<CanvasCollabSnapshot>,
    confirmed: Option<Arc<ConfirmedReplacement>>,
    retired: bool,
}
impl Job {
    fn capture(shell: &ShellState, read: &Read) -> Option<Self> {
        if !read.current(shell) {
            return None;
        }
        let view = shell.canvas_checklist.view.as_ref()?;
        if view.stale || view.loading {
            return None;
        }
        let source = view.snapshot.as_ref()?;
        let operation = shell.canvas_checklist.operations.iter().find(|op| {
            op.attempt.snapshot.file_id() == read.file
                && op.attempt.scope.stable_current(shell)
                && op.stage == Stage::AwaitingRead
                && read.serial > op.dispatched_after_read
        })?;
        let Accepted::Replacement(accepted) = operation.accepted.as_ref()? else {
            return None;
        };
        if source.thread_id() != operation.attempt.snapshot.thread_id()
            || source.document_id() != operation.attempt.snapshot.document_id()
        {
            return None;
        }
        if operation.durable.is_none() && !recovery::bypass(shell) {
            return None;
        }
        Some(Self {
            accepted: accepted.value.clone(),
            source: source.clone(),
            attempt: operation.attempt.clone(),
            read: read.clone(),
            dispatched: operation.dispatched.clone(),
            dispatched_after_read: operation.dispatched_after_read,
            ticket: operation.durable.clone(),
        })
    }
    fn matches(
        &self,
        accepted: &Arc<AcceptedReplacement>,
        attempt: &Attempt,
        read: &Read,
        operation: &Operation,
        retired: bool,
    ) -> bool {
        ticket_matches(self.ticket.as_ref(), operation.durable.as_ref(), retired)
            && Arc::ptr_eq(&self.accepted, accepted)
            && self.attempt.serial == attempt.serial
            && self.attempt.visit == attempt.visit
            && self.attempt.read == attempt.read
            && self.attempt.revision == attempt.revision
            && self.attempt.section == attempt.section
            && Arc::ptr_eq(&self.attempt.snapshot, &attempt.snapshot)
            && replacement(&self.attempt) == replacement(attempt)
            && self.read.file == read.file
            && self.read.visit == read.visit
            && self.read.serial == read.serial
            && self.read.revision == read.revision
            && operation.stage == Stage::AwaitingRead
            && operation.attempt.serial == self.attempt.serial
            && operation.attempt.visit == self.attempt.visit
            && Arc::ptr_eq(&operation.attempt.snapshot, &self.attempt.snapshot)
            && replacement(&operation.attempt) == replacement(&self.attempt)
            && operation.dispatched == self.dispatched
            && operation.dispatched_after_read == self.dispatched_after_read
    }
    fn run(self) -> Option<Proof> {
        let confirmed = self
            .ticket
            .as_ref()
            .map(|ticket| {
                self.accepted
                    .confirmed_readback(&self.source, ticket.receipt())
                    .map(Arc::new)
            })
            .transpose()
            .ok()?;
        let certified = match &confirmed {
            Some(token) => token.snapshot_arc(),
            None => Arc::new(self.accepted.certify_readback(&self.source).ok()?),
        };
        Some(Proof {
            job: self,
            certified,
            confirmed,
            retired: false,
        })
    }
}
impl Proof {
    fn publish(self, shell: &mut ShellState) -> bool {
        let Some(current) = Job::capture(shell, &self.job.read) else {
            return false;
        };
        if !Arc::ptr_eq(&self.job.source, &current.source) {
            return false;
        }
        let Some(operation) = shell
            .canvas_checklist
            .operations
            .iter_mut()
            .find(|op| op.attempt.serial == self.job.attempt.serial)
        else {
            return false;
        };
        if !self.job.matches(
            &current.accepted,
            &current.attempt,
            &current.read,
            operation,
            false,
        ) {
            return false;
        }
        let Some(Accepted::Replacement(accepted)) = operation.accepted.as_mut() else {
            return false;
        };
        let certified = self.certified.clone();
        accepted.proof = Some(self);
        shell.canvas_checklist.view.as_mut().unwrap().snapshot = Some(certified);
        true
    }
}
pub(in crate::canvas::checklist) async fn verify(mut state: Signal<ShellState>, read: &Read) {
    let job = {
        let shell = state.peek();
        let Some(job) = Job::capture(&shell, read) else {
            return;
        };
        let operation = shell
            .canvas_checklist
            .operations
            .iter()
            .find(|op| op.attempt.serial == job.attempt.serial)
            .unwrap();
        let Some(Accepted::Replacement(accepted)) = operation.accepted.as_ref() else {
            return;
        };
        if accepted.confirms(&shell, &job.source, &job.attempt, read, operation) {
            drop(shell);
            confirm_readback(&mut state.write(), &job.attempt, read);
            return;
        }
        job
    };
    let Ok(Some(proof)) = tokio::task::spawn_blocking(move || job.run()).await else {
        return;
    };
    let attempt = proof.job.attempt.clone();
    let mut shell = state.write();
    if proof.publish(&mut shell) {
        confirm_readback(&mut shell, &attempt, read);
    }
}
/// Fresh network rows cannot erase already-known provenance by losing one of
/// its retained endpoints. Failure leaves the read non-authoritative.
pub(in crate::canvas) async fn carry_for_read(
    state: Signal<ShellState>,
    read: &Read,
    fresh: Arc<CanvasCollabSnapshot>,
) -> Result<Arc<CanvasCollabSnapshot>, &'static str> {
    let previous = {
        let shell = state.peek();
        if !read.current(&shell) {
            return Err("This canvas read changed.");
        }
        shell
            .canvas_checklist
            .view
            .as_ref()
            .and_then(|v| v.snapshot.clone())
    };
    let Some(previous) = previous else {
        return recovery::confirmed::restore(state, read, fresh).await;
    };
    let source = previous.clone();
    let result = tokio::task::spawn_blocking(move || {
        super_platinum_core::canvas::edit::structure::carry_replacement_provenance(&source, &fresh)
            .map(Arc::new)
    })
    .await
    .map_err(|_| "Canvas retained history could not be checked.")?
    .map_err(|_| "Canvas retained history changed. Reopen it to check before editing.")?;
    let shell = state.peek();
    if !read.current(&shell)
        || shell
            .canvas_checklist
            .view
            .as_ref()
            .and_then(|v| v.snapshot.as_ref())
            .is_none_or(|current| !Arc::ptr_eq(current, &previous))
    {
        return Err("This canvas read changed.");
    }
    drop(shell);
    recovery::confirmed::restore(state, read, result).await
}
#[cfg(test)]
pub(in crate::canvas) fn verify_now(shell: &mut ShellState, read: &Read) -> bool {
    let Some(proof) = Job::capture(shell, read).and_then(Job::run) else {
        return false;
    };
    let attempt = proof.job.attempt.clone();
    proof.publish(shell) && confirm_readback(shell, &attempt, read)
}
#[cfg(test)]
mod tests;
