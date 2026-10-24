//! Worker-produced three-row split readback proof, bound to immutable objects and one read.
use super::*;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};

pub(in crate::canvas::checklist) struct AcceptedState {
    value: Arc<AcceptedSplit>,
    proof: Option<Proof>,
}
impl AcceptedState {
    pub(super) fn new(value: AcceptedSplit) -> Self {
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
        self.proof.as_ref().is_some_and(|proof| {
            proof.job.read.current(shell)
                && proof.job.attempt.scope.stable_current(shell)
                && proof
                    .job
                    .matches(&self.value, snapshot, attempt, read, operation)
        })
    }
}

struct Job {
    accepted: Arc<AcceptedSplit>,
    snapshot: Arc<CanvasCollabSnapshot>,
    attempt: Attempt,
    read: Read,
    dispatched: Option<(String, u64)>,
    dispatched_after_read: u64,
}

// This type has no public constructor or boolean acceptance entry point.
// Only executing the core all-member proof can construct it.
struct Proof {
    job: Job,
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
        let snapshot = view.snapshot.as_ref()?;
        let operation = shell.canvas_checklist.operations.iter().find(|op| {
            op.attempt.snapshot.file_id() == read.file
                && op.attempt.scope.stable_current(shell)
                && op.stage == Stage::AwaitingRead
                && read.serial > op.dispatched_after_read
        })?;
        let Accepted::SplitSequence(accepted) = operation.accepted.as_ref()? else {
            return None;
        };
        let Mutation::Split {
            child,
            second_child: Some(second_child),
            ..
        } = &operation.attempt.mutation
        else {
            return None;
        };
        if child.as_deref() != Some(accepted.value.child_id())
            || Some(second_child.as_str()) != accepted.value.second_child_id()
        {
            return None;
        }
        if snapshot.thread_id() != operation.attempt.snapshot.thread_id()
            || snapshot.document_id() != operation.attempt.snapshot.document_id()
        {
            return None;
        }
        Some(Self {
            accepted: accepted.value.clone(),
            snapshot: snapshot.clone(),
            attempt: operation.attempt.clone(),
            read: read.clone(),
            dispatched: operation.dispatched.clone(),
            dispatched_after_read: operation.dispatched_after_read,
        })
    }

    fn matches(
        &self,
        accepted: &Arc<AcceptedSplit>,
        snapshot: &Arc<CanvasCollabSnapshot>,
        attempt: &Attempt,
        read: &Read,
        operation: &Operation,
    ) -> bool {
        Arc::ptr_eq(&self.accepted, accepted)
            && Arc::ptr_eq(&self.snapshot, snapshot)
            && self.same_attempt(attempt)
            && self.same_attempt(&operation.attempt)
            && matches!(&operation.attempt.mutation,
                Mutation::Split { child, second_child, .. }
                if child.as_deref() == Some(self.accepted.child_id())
                    && second_child.as_deref() == self.accepted.second_child_id())
            && self.read.file == read.file
            && self.read.visit == read.visit
            && self.read.serial == read.serial
            && self.read.revision == read.revision
            && operation.stage == Stage::AwaitingRead
            && operation.attempt.serial == self.attempt.serial
            && Arc::ptr_eq(&operation.attempt.snapshot, &self.attempt.snapshot)
            && operation.dispatched == self.dispatched
            && operation.dispatched_after_read == self.dispatched_after_read
    }

    fn same_attempt(&self, attempt: &Attempt) -> bool {
        self.attempt.serial == attempt.serial
            && self.attempt.visit == attempt.visit
            && self.attempt.read == attempt.read
            && self.attempt.revision == attempt.revision
            && self.attempt.section == attempt.section
            && Arc::ptr_eq(&self.attempt.snapshot, &attempt.snapshot)
            && match (&self.attempt.mutation, &attempt.mutation) {
                (Mutation::Split { witness: a, .. }, Mutation::Split { witness: b, .. }) => {
                    Arc::ptr_eq(a, b)
                }
                _ => false,
            }
    }

    fn run(self) -> Option<Proof> {
        self.accepted
            .confirms_readback(&self.snapshot)
            .unwrap_or(false)
            .then_some(Proof { job: self })
    }
}

impl Proof {
    fn publish(self, shell: &mut ShellState) -> bool {
        // Capture repeats all current owner/read/operation checks after await.
        let Some(current) = Job::capture(shell, &self.job.read) else {
            return false;
        };
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
            &current.snapshot,
            &current.attempt,
            &current.read,
            operation,
        ) {
            return false;
        }
        let Some(Accepted::SplitSequence(accepted)) = operation.accepted.as_mut() else {
            return false;
        };
        accepted.proof = Some(self);
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
        let Some(Accepted::SplitSequence(accepted)) = operation.accepted.as_ref() else {
            return;
        };
        if accepted.confirms(&shell, &job.snapshot, &job.attempt, read, operation) {
            drop(shell);
            confirm_readback(&mut state.write(), &job.attempt, read);
            return;
        }
        job
    };
    let Ok(Some(proof)) = tokio::task::spawn_blocking(move || job.run()).await else {
        return; // Failed validation or worker leaves the operation locked.
    };
    let attempt = proof.job.attempt.clone();
    let mut shell = state.write();
    if proof.publish(&mut shell) {
        confirm_readback(&mut shell, &attempt, read);
    }
}

#[cfg(test)]
pub(in crate::canvas::checklist) mod tests;
