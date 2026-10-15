//! Explicit human removal of a local edit lock. This never certifies server
//! success and exposes no path to an actor, intent, request replay or Accepted.
use super::*;
use crate::canvas::edit::structure::inventory::Inventory;
use crate::canvas::edit::text::TextDraft;
use crate::slack::api::CanvasCollabSnapshot;
use serde::Serialize;

const MAX_REVIEWED: i64 = 32;
const MAX_REVIEWED_BYTES: usize = 64 * 1024 * 1024;

/// Caller must obtain a deliberate human decision after displaying the retained
/// request and unknown-outcome warning. Canonical content cannot create this.
pub enum LocalDecision {
    RemoveEditLockOutcomeUnknown,
}

pub struct LocalReview {
    account: String,
    receipt: Receipt,
    inventory: Inventory,
    observed: String,
}
impl LocalReview {
    pub fn operation_id(&self) -> &str {
        self.receipt.operation_id()
    }
    pub fn is_replacement(&self) -> bool {
        self.receipt.checkpoint.is_replacement()
    }
    pub fn is_delete(&self) -> bool {
        self.receipt.checkpoint.is_delete()
    }
    pub fn draft_text(&self) -> String {
        // Checkpoint validation already admitted this exact encoded-text grammar.
        TextDraft::from_rtml(self.receipt.checkpoint.draft_rtml())
            .unwrap()
            .runs
            .into_iter()
            .map(|run| run.text)
            .collect()
    }
    pub fn revalidate(&self, snapshot: &CanvasCollabSnapshot) -> Result<(), AppError> {
        self.inventory.revalidate(snapshot).map_err(|_| invalid())
    }
}

#[derive(Serialize)]
struct Observed {
    // No controller initializer, auth/session/Transport or credentials persisted.
    thread_sequence: u64,
    document_sequence: u64,
    sections: Vec<Vec<u8>>,
}

impl Journal {
    /// Count archived local decisions for an exact scope, without reconstructing
    /// an operation or treating its remote outcome as successful.
    pub fn local_review_count(&self, scope: &Scope) -> Result<usize, AppError> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM local_reviews WHERE workspace=?1 AND user=?2 AND file=?3 AND decision='local_unlock_outcome_unknown'",
            params![scope.workspace, scope.user, scope.file], |row| row.get(0))?;
        usize::try_from(count).map_err(|_| invalid())
    }
    /// Read-only audit lookup. A local review remains an unknown remote outcome.
    pub fn has_local_review(&self, receipt: &Receipt) -> Result<bool, AppError> {
        let scope = receipt.checkpoint.scope();
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM local_reviews WHERE workspace=?1 AND user=?2 AND file=?3 AND operation=?4 AND body=?5 AND decision='local_unlock_outcome_unknown'",
            params![scope.workspace,scope.user,scope.file,receipt.id,receipt.body], |row| row.get(0))?;
        Ok(count == 1)
    }
    /// Read-only review capture binds the actual account, exact on-disk row and
    /// complete fresh canonical inventory. It does not compare text for success.
    pub fn local_review(
        &self,
        receipt: &Receipt,
        snapshot: &CanvasCollabSnapshot,
    ) -> Result<LocalReview, AppError> {
        if read(&self.conn, receipt.checkpoint.scope())?.as_ref() != Some(receipt) {
            return Err(invalid());
        }
        let inventory = if receipt.checkpoint.is_replacement() {
            Inventory::capture_for_local_review(snapshot)
        } else {
            Inventory::capture(snapshot)
        }
        .map_err(|_| invalid())?;
        let c = &receipt.checkpoint;
        let owner = &inventory.owner;
        if owner.workspace != c.scope.workspace
            || owner.user != c.scope.user
            || owner.file != c.scope.file
            || owner.thread != c.thread
            || owner.document != c.document
        {
            return Err(invalid());
        }
        let observed = serde_json::to_string(&Observed {
            thread_sequence: snapshot
                .thread
                .uint(2)
                .map_err(|_| invalid())?
                .filter(|v| *v > 0)
                .ok_or_else(invalid)?,
            document_sequence: snapshot
                .document
                .uint(2)
                .map_err(|_| invalid())?
                .filter(|v| *v > 0)
                .ok_or_else(invalid)?,
            sections: inventory
                .sections
                .values()
                .map(|s| s.raw().raw().to_vec())
                .collect(),
        })?;
        if observed.len() > MAX_BODY {
            return Err(invalid());
        }
        Ok(LocalReview {
            account: self.account.clone(),
            receipt: receipt.clone(),
            inventory,
            observed,
        })
    }

    /// The explicit local decision is archived atomically before the exact active
    /// row is removed. Failure/old row/account/canonical changes preserve the lock.
    /// This method is separate from complete(), which requires non-dispatch or a
    /// sealed ACK/readback. Here the remote outcome intentionally stays unknown.
    pub fn remove_local_lock(
        &mut self,
        review: &LocalReview,
        snapshot: &CanvasCollabSnapshot,
        _decision: LocalDecision,
        reviewed_at_usec: u64,
    ) -> Result<bool, AppError> {
        if self.account != review.account
            || reviewed_at_usec == 0
            || reviewed_at_usec > i64::MAX as u64
        {
            return Err(invalid());
        }
        review.revalidate(snapshot)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let receipt = &review.receipt;
        let scope = receipt.checkpoint.scope();
        if read(&tx, scope)?.as_ref() != Some(receipt) {
            return Ok(false);
        }
        let (count, bytes): (i64, i64) = tx.query_row(
            "SELECT count(*),coalesce(sum(length(CAST(body AS BLOB))+length(CAST(observed AS BLOB))),0) FROM local_reviews",
            [], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let new_bytes = receipt
            .body
            .len()
            .checked_add(review.observed.len())
            .ok_or_else(invalid)?;
        if !archive_admits(count, bytes, new_bytes) {
            return Err(invalid());
        }
        tx.execute(
            "INSERT INTO local_reviews VALUES(?1,?2,?3,?4,?5,?6,?7,'local_unlock_outcome_unknown')",
            params![
                scope.workspace,
                scope.user,
                scope.file,
                receipt.id,
                receipt.body,
                review.observed,
                reviewed_at_usec as i64
            ],
        )?;
        let changed = tx.execute("DELETE FROM receipts WHERE workspace=?1 AND user=?2 AND file=?3 AND operation=?4 AND body=?5",
            params![scope.workspace,scope.user,scope.file,receipt.id,receipt.body])?;
        if changed != 1 {
            return Err(invalid());
        }
        tx.commit()?;
        Ok(true)
    }
}

fn archive_admits(count: i64, bytes: i64, new_bytes: usize) -> bool {
    count >= 0
        && count < MAX_REVIEWED
        && bytes >= 0
        && (bytes as usize)
            .checked_add(new_bytes)
            .is_some_and(|v| v <= MAX_REVIEWED_BYTES)
}

#[cfg(test)]
mod tests;
