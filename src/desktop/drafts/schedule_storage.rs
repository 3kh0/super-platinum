//! Route production through the existing FIFO worker; isolated tests use SQLite.
use super::*;
use super_platinum_core::drafts::store::sync::{LocalReview, ReviewedScheduleCommit};
impl Backend {
    pub async fn commit_reviewed_schedule(
        &self,
        expected: LocalReview,
        reviewed: Snapshot,
        post_at: i64,
    ) -> Result<ReviewedScheduleCommit, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::commit_reviewed_schedule(expected, reviewed, post_at),
            Self::Isolated { path, .. } => {
                DraftStore::open(path)?.commit_reviewed_schedule(&expected, &reviewed, post_at)
            }
        })
        .await
    }
}
