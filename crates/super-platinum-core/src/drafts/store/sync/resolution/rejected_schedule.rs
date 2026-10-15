//! Retry/replacement never clears, decodes or replaces the separate local draft.
use super::*;
use crate::drafts::sync::{RejectedScheduleChoice, RejectedScheduleError};

#[derive(Debug)]
pub enum RejectedScheduleCommit {
    Applied { local: LocalDraft, stored: Stored },
    Stale,
    Blocked,
    Invalid(RejectedScheduleError),
}
impl DraftStore {
    pub fn review_rejected_schedule(
        &mut self,
        expected: &LocalReview,
        choice: &RejectedScheduleChoice,
        now_secs: i64,
    ) -> Result<RejectedScheduleCommit, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &expected.scope.account_id)?;
        let current = review(&tx, &expected.scope)?;
        if !expected.matches(&current) {
            return Ok(RejectedScheduleCommit::Stale);
        }
        if current.blocked.is_some() || !current.alternates.is_empty() {
            return Ok(RejectedScheduleCommit::Blocked);
        }
        let Some(previous) = current.stored.as_ref() else {
            return Ok(RejectedScheduleCommit::Stale);
        };
        let mut state = previous.state.clone();
        if let Err(problem) = state.review_rejected_schedule(&previous.state, choice, now_secs) {
            return Ok(RejectedScheduleCommit::Invalid(problem));
        }
        validate(&expected.scope, &state)?;
        let stored = Stored {
            revision: next_revision(Some(previous))?,
            state,
        };
        // Leave composer_drafts bytes untouched. Newer local content, selection,
        // files and unsupported shapes are independent of the frozen schedule.
        write(&tx, &stored)?;
        tx.commit()?;
        Ok(RejectedScheduleCommit::Applied {
            local: current.local,
            stored,
        })
    }
}
