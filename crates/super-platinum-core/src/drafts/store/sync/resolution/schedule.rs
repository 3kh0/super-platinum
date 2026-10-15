//! Exact inventory CAS and paired consumption; no new schema or network action.
use super::*;
use crate::drafts::sync::Snapshot;

#[derive(Debug)]
pub enum ReviewedScheduleCommit {
    Applied(Stored),
    Stale,
    Blocked,
}
impl DraftStore {
    pub fn commit_reviewed_schedule(
        &mut self,
        expected: &LocalReview,
        reviewed: &Snapshot,
        post_at: i64,
    ) -> Result<ReviewedScheduleCommit, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scope = &expected.scope;
        owner(&tx, &scope.account_id)?;
        let current = review(&tx, scope)?;
        if !expected.matches(&current) {
            return Ok(ReviewedScheduleCommit::Stale);
        }
        if current.blocked.is_some()
            || !current.alternates.is_empty()
            || !current.local.attachments.is_empty()
        {
            return Ok(ReviewedScheduleCommit::Blocked);
        }
        let Some(previous) = current.stored.as_ref() else {
            return Ok(ReviewedScheduleCommit::Stale);
        };
        // Validate the original paired local representation, never roundtrip the
        // independent reviewed rich payload through the ordinary composer codec.
        let Some(desired) = previous.state.desired() else {
            return Ok(ReviewedScheduleCommit::Blocked);
        };
        let Ok(projection) = codec::decode(&scope.key, desired) else {
            return Ok(ReviewedScheduleCommit::Blocked);
        };
        if current.local.composer.text.trim().is_empty()
            || projection
                .snapshot_for(&current.local.composer)
                .as_ref()
                .ok()
                != Some(desired)
        {
            return Ok(ReviewedScheduleCommit::Blocked);
        }
        let mut state = previous.state.clone();
        if state
            .request_reviewed_schedule(&previous.state, reviewed.clone(), post_at)
            .is_err()
        {
            return Ok(ReviewedScheduleCommit::Blocked);
        }
        validate(scope, &state)?;
        let stored = Stored {
            revision: next_revision(Some(previous))?,
            state,
        };
        write_local(&tx, &scope.key, &LocalDraft::default())?;
        write(&tx, &stored)?;
        tx.commit()?;
        Ok(ReviewedScheduleCommit::Applied(stored))
    }
}
