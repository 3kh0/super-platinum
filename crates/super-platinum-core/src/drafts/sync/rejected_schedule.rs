//! Explicit disposition of a definitively rejected Schedule; no dispatch.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RejectedScheduleChoice {
    Retry,
    Replace { snapshot: Snapshot, post_at: i64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RejectedScheduleError {
    #[error("the reviewed rejected schedule changed")]
    Stale,
    #[error("this is not a supported definitively rejected schedule")]
    Ineligible,
    #[error("choose a future time within the scheduling window")]
    Time,
    #[error("the replacement snapshot or destination is unsupported")]
    Snapshot,
}

impl DraftSync {
    /// This guard accepts only persisted definitive rejection, never uncertain
    /// outcomes, absence, missing receipts, or an ordinary prerequisite update.
    /// The adapter's existing strict rejection classifier establishes that state.
    pub fn rejected_schedule(&self) -> Option<(&ConsumeIntent, &Operation)> {
        let Workflow::Rejected(operation) = &self.workflow else {
            return None;
        };
        let intent = self.consumption.as_ref()?;
        (operation.known_schedule_check()
            && self.accepts_direct_local(intent)
            && self.recovery_state_valid()
            && self.pending_check(operation).is_none())
        .then_some((intent, operation))
    }

    /// Compare the complete reviewed state before changing anything. Pair this
    /// state with the UNCHANGED LocalDraft in the whole-inventory transaction.
    /// New local input is already separate from the old frozen intent; it must
    /// not be decoded, cleared or consumed again, even when unsupported by codec.
    pub fn review_rejected_schedule(
        &mut self,
        predecessor: &DraftSync,
        choice: &RejectedScheduleChoice,
        now_secs: i64,
    ) -> Result<ConsumeIntent, RejectedScheduleError> {
        if self != predecessor {
            return Err(RejectedScheduleError::Stale);
        }
        let (old, operation) = self
            .rejected_schedule()
            .ok_or(RejectedScheduleError::Ineligible)?;
        let mut intent = old.clone();
        match choice {
            RejectedScheduleChoice::Retry => {}
            RejectedScheduleChoice::Replace { snapshot, post_at } => {
                // New intent identity only after explicit choice, while retaining
                // the verified existing server owner/client UUID and the original
                // detachment revision. Current local_revision/desired stay intact.
                intent.id = uuid::Uuid::new_v4().to_string();
                intent.kind = ConsumeKind::Schedule { post_at: *post_at };
                intent.snapshot = snapshot.clone();
                let candidate = Operation {
                    consume_id: Some(intent.id.clone()),
                    kind: OperationKind::Schedule { post_at: *post_at },
                    snapshot: Some(snapshot.clone()),
                    ..operation.clone()
                };
                // Shared strict request/readback routing policy; no broadening
                // merely because the raw Snapshot model can retain extra fields.
                if !candidate.known_schedule_check() {
                    return Err(RejectedScheduleError::Snapshot);
                }
            }
        }
        let ConsumeKind::Schedule { post_at } = intent.kind else {
            unreachable!()
        };
        if now_secs <= 0 || crate::scheduled::validate_time(post_at, now_secs).is_err() {
            return Err(RejectedScheduleError::Time);
        }
        // Existing explicit retry semantics: a known failed attempt may end;
        // its immutable intent stays exact for Retry, or the human explicitly
        // replaces that intent. No operation or HTTP attempt is allocated here.
        self.consumption = Some(intent.clone());
        self.workflow = Workflow::Idle;
        self.local_completion = None;
        Ok(intent)
    }
}

#[cfg(test)]
mod tests;
