//! Sealed successful readback for one exact durable reservation. No Deserialize.
use super::*;
use recovery::journal::Receipt;
use std::sync::Arc;

pub struct ConfirmedReplacement {
    checkpoint: recovery::Checkpoint,
    operation: String,
    snapshot: Arc<CanvasCollabSnapshot>,
}
impl ConfirmedReplacement {
    /// Borrow on a worker, or wrap the moved snapshot in Arc for publication.
    pub fn snapshot(&self) -> &CanvasCollabSnapshot {
        &self.snapshot
    }
    pub fn snapshot_arc(&self) -> Arc<CanvasCollabSnapshot> {
        self.snapshot.clone()
    }
    pub fn into_snapshot(self) -> Arc<CanvasCollabSnapshot> {
        self.snapshot
    }
    pub(in crate::canvas::edit::structure) fn checkpoint(&self) -> &recovery::Checkpoint {
        &self.checkpoint
    }
    pub(in crate::canvas::edit::structure) fn operation_id(&self) -> &str {
        &self.operation
    }
}
impl AcceptedReplacement {
    /// The caller's receipt must describe this exact source, request, session and
    /// pair. A later reservation of the same checkpoint gets a different UUID.
    pub fn confirmed_readback(
        &self,
        fresh: &CanvasCollabSnapshot,
        receipt: &Receipt,
    ) -> Result<ConfirmedReplacement, Error> {
        let checkpoint = self.checkpoint.as_ref().map_err(|e| *e)?;
        if checkpoint != receipt.checkpoint() {
            return Err(Error::Changed);
        }
        let snapshot = self.certify_readback(fresh)?;
        if self
            .confirmed_receipt
            .get_or_init(|| receipt.operation_id().to_owned())
            != receipt.operation_id()
        {
            return Err(Error::Changed);
        }
        Ok(ConfirmedReplacement {
            checkpoint: checkpoint.clone(),
            operation: receipt.operation_id().to_owned(),
            snapshot: Arc::new(snapshot),
        })
    }
}
