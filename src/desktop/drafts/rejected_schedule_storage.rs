use super::*;
use super_platinum_core::drafts::{
    store::sync::{LocalReview, RejectedScheduleCommit},
    sync::RejectedScheduleChoice,
};
impl Backend {
    pub async fn review_rejected_schedule(
        &self,
        expected: LocalReview,
        choice: RejectedScheduleChoice,
        now_secs: i64,
    ) -> Result<RejectedScheduleCommit, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::review_rejected_schedule(expected, choice, now_secs),
            Self::Isolated { path, .. } => {
                DraftStore::open(path)?.review_rejected_schedule(&expected, &choice, now_secs)
            }
        })
        .await
    }
}
