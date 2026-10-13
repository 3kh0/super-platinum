use super::*;
use super_platinum_core::drafts::store::sync::{LocalChoice, LocalReview, LocalSettlement};

impl Backend {
    pub async fn read_local_review(&self, scope: Scope) -> Result<LocalReview, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::read_local_review(scope),
            Self::Isolated { path, .. } => DraftStore::open(path)?.read_local_review(&scope),
        })
        .await
    }
    pub async fn settle_local_versions(
        &self,
        review: LocalReview,
        choice: LocalChoice,
    ) -> Result<LocalSettlement, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::settle_local_versions(review, choice),
            Self::Isolated { path, .. } => {
                DraftStore::open(path)?.settle_local_versions(&review, &choice)
            }
        })
        .await
    }
}
