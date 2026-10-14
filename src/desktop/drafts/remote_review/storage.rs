//! Serialized storage adapter; no direct production database bypass.
use super::*;
use super_platinum_core::drafts::store::sync::{RemoteChoice, RemoteReview, RemoteSettlement};
impl Backend {
    pub async fn reconcile_remote_review(
        &self,
        review: RemoteReview,
        choice: RemoteChoice,
    ) -> Result<RemoteSettlement, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::reconcile_remote_review(review, choice),
            Self::Isolated { path, .. } => {
                DraftStore::open(path)?.reconcile_remote_review(&review, &choice)
            }
        })
        .await
    }
}
