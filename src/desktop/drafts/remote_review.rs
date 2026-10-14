//! Explicit positive review transaction. No HTTP or renderer installation.
//! Pending module hooks; production storage stays on its existing FIFO worker.
use super::*;
use super_platinum_core::drafts::store::sync::{RemoteChoice, RemoteReview, RemoteSettlement};
impl Owner {
    pub(super) async fn reconcile_remote_review(
        &self,
        review: RemoteReview,
        choice: RemoteChoice,
    ) -> Result<View, Fault> {
        if review.inventory().scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        let _guard = self.gate.lock().await;
        self.account.initialize().await?;
        match self
            .account
            .backend
            .reconcile_remote_review(review, choice)
            .await?
        {
            RemoteSettlement::Applied { local, stored } => Ok(View {
                local,
                stored: Some(stored),
                blocked: None,
                uncommitted: None,
            }),
            RemoteSettlement::Stale => Err(Fault::Stale),
            RemoteSettlement::Blocked => Err(Fault::Unsupported),
        }
    }
}
