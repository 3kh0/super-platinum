//! One explicit rejected-Schedule decision; no dispatch or composer replacement.
use super::dispatch_schedule::{clean_schedule_review, same_schedule_view, schedule_view};
use super::*;
use super_platinum_core::drafts::{
    store::sync::{LocalReview, RejectedScheduleCommit},
    sync::RejectedScheduleChoice,
};
impl Owner {
    pub(super) async fn review_rejected_schedule_with<A, C, Fut>(
        &self,
        expected: LocalReview,
        choice: RejectedScheduleChoice,
        now_secs: i64,
        admit: A,
        commit: C,
    ) -> Result<View, Fault>
    where
        A: FnOnce() -> Result<(), Fault>,
        C: FnOnce(Backend, LocalReview, RejectedScheduleChoice, i64) -> Fut,
        Fut: Future<Output = Result<RejectedScheduleCommit, Fault>>,
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
        let mut next = row.state.clone();
        next.review_rejected_schedule(&row.state, &choice, now_secs)
            .map_err(|_| Fault::Unsupported)?;
        admit()?;
        match commit(self.account.backend.clone(), expected, choice, now_secs).await? {
            RejectedScheduleCommit::Applied { local, stored } => Ok(View {
                local,
                stored: Some(stored),
                blocked: None,
                uncommitted: None,
            }),
            RejectedScheduleCommit::Stale => Err(Fault::Stale),
            RejectedScheduleCommit::Blocked => Err(Fault::AlternateEdits),
            RejectedScheduleCommit::Invalid(_) => Err(Fault::Unsupported),
        }
    }
}
