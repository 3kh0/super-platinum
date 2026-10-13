//! Explicit local version choice. Never authorizes a Slack write or replay.
use super::*;
use super_platinum_core::drafts::store::sync::{LocalChoice, LocalReview, LocalSettlement};
mod controller;
pub(crate) use controller::{Prepared, apply, open};

impl Owner {
    pub async fn review_local(&self) -> Result<LocalReview, Fault> {
        let _guard = self.gate.lock().await;
        self.account.initialize().await?;
        self.account
            .backend
            .read_local_review(self.scope.clone())
            .await
    }
    pub async fn settle_local(
        &self,
        review: LocalReview,
        choice: LocalChoice,
    ) -> Result<View, Fault> {
        if review.scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        let _guard = self.gate.lock().await;
        self.account.initialize().await?;
        match self
            .account
            .backend
            .settle_local_versions(review, choice)
            .await?
        {
            LocalSettlement::Applied { local, stored } => Ok(View {
                local,
                stored,
                blocked: None,
                uncommitted: None,
            }),
            LocalSettlement::Stale => Err(Fault::Stale),
            LocalSettlement::Blocked => Err(Fault::AlternateEdits),
        }
    }
}
pub(super) fn account(id: &str) -> Result<Arc<Account>, Fault> {
    if crate::fixture::is_fixture() {
        super::settlement_fixture::account(id)
    } else {
        Account::production(id)
    }
}
