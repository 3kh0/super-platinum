//! Complete predecessor guard and paired rich Schedule consumption; no network.
use super::*;
use super_platinum_core::drafts::store::sync::{LocalReview, ReviewedScheduleCommit};

pub(super) fn schedule_view(review: &LocalReview) -> View {
    View {
        local: review.local.clone(),
        stored: review.stored.clone(),
        blocked: review.blocked.clone(),
        uncommitted: None,
    }
}

// Schedule leases require an entirely clean inventory; these helpers do not
// discard blocked/alternate fields to make an otherwise changed view compare.
pub(super) fn clean_schedule_review(review: &LocalReview) -> bool {
    review.blocked.is_none() && review.alternates.is_empty() && review.stored.is_some()
}
pub(super) fn same_schedule_view(a: &View, b: &View) -> bool {
    a.blocked.is_none()
        && b.blocked.is_none()
        && a.uncommitted.is_none()
        && b.uncommitted.is_none()
        && same_local(&a.local, &b.local)
        && match (&a.stored, &b.stored) {
            (Some(a), Some(b)) => a.revision == b.revision && a.state == b.state,
            _ => false,
        }
}

impl Owner {
    /// `admit` is synchronous and is invoked AFTER the final owner read. It
    /// rechecks the retained editor serial/visit before the paired commit is
    /// queued. Later input is intentionally separate and must remain queued.
    /// The storage adapter repeats the whole inventory comparison IN SQLite.
    pub(super) async fn consume_reviewed_schedule_with<A, C, Fut>(
        &self,
        expected: LocalReview,
        reviewed: Snapshot,
        post_at: i64,
        admit: A,
        commit: C,
    ) -> Result<Stored, Fault>
    where
        A: FnOnce() -> Result<(), Fault>,
        C: FnOnce(Backend, LocalReview, Snapshot, i64) -> Fut,
        Fut: Future<Output = Result<ReviewedScheduleCommit, Fault>>,
    {
        if expected.scope != self.scope || !clean_schedule_review(&expected) {
            return Err(Fault::Stale);
        }
        let _owner = self.gate.lock().await;
        self.account.initialize().await?;
        let current = self
            .account
            .backend
            .read_local_review(self.scope.clone())
            .await?;
        if !clean_schedule_review(&current)
            || !same_schedule_view(&schedule_view(&current), &schedule_view(&expected))
        {
            return Err(Fault::Stale);
        }
        let row = current.stored.as_ref().unwrap();
        // The existing composer predecessor must still match its supported raw
        // desired value. Only the DIALOG snapshot is allowed to exceed codec.
        if current.local.composer.text.trim().is_empty()
            || retained_snapshot(&self.scope.key, &current.local, row.state.desired())?.as_ref()
                != row.state.desired()
        {
            return Err(Fault::Stale);
        }
        let mut next = row.state.clone();
        next.request_reviewed_schedule(&row.state, reviewed.clone(), post_at)
            .map_err(|_| Fault::Unsupported)?;
        // No await between this serial/visit check and handing the immutable
        // expected inventory to the async storage boundary. The narrow commit
        // closure is also the deterministic test boundary for during-I/O input.
        admit()?;
        match commit(self.account.backend.clone(), expected, reviewed, post_at).await? {
            ReviewedScheduleCommit::Applied(stored) => Ok(stored),
            ReviewedScheduleCommit::Stale => Err(Fault::Stale),
            ReviewedScheduleCommit::Blocked => Err(Fault::AlternateEdits),
        }
    }
}
