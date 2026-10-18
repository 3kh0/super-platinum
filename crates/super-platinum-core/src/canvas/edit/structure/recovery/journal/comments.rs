//! Comment receipts share the existing account journal and file-wide lock.
//! Restoring a row never reconstructs ArchiveIntent or dispatch permission.
use super::*;
use crate::canvas::comments::archive::ArchiveRecord;

const MAX_COMMENT_BODY: usize = 96 * 1024;
#[derive(Clone, PartialEq, Eq)]
pub struct CommentReceipt {
    id: String,
    record: ArchiveRecord,
    body: String,
}
impl CommentReceipt {
    pub fn operation_id(&self) -> &str {
        &self.id
    }
    pub fn record(&self) -> &ArchiveRecord {
        &self.record
    }
}
pub enum CommentBegin {
    Started(CommentReceipt),
    Blocked(CommentReceipt),
}
pub struct CommentReview {
    account: String,
    receipt: CommentReceipt,
    observed: String,
}
pub enum CommentDecision {
    RemoveLocalLockOutcomeUnknown,
}
fn scope(record: &ArchiveRecord) -> Scope {
    Scope {
        workspace: record.workspace().into(),
        user: record.user().into(),
        file: record.file().into(),
    }
}
pub(super) fn present(conn: &Connection, owner: &Scope) -> Result<bool, AppError> {
    scope_valid(owner)?;
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM comment_receipts WHERE workspace=?1 AND user=?2 AND file=?3",
        params![owner.workspace, owner.user, owner.file],
        |r| r.get(0),
    )?;
    Ok(count != 0)
}
fn read_comment(conn: &Connection, owner: &Scope) -> Result<Option<CommentReceipt>, AppError> {
    scope_valid(owner)?;
    let row: Option<(String, String)> = conn.query_row(
        "SELECT operation,CASE WHEN length(CAST(body AS BLOB))<=?4 THEN body ELSE NULL END FROM comment_receipts WHERE workspace=?1 AND user=?2 AND file=?3",
        params![owner.workspace, owner.user, owner.file, MAX_COMMENT_BODY as i64],
        |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let Some((id, body)) = row else {
        return Ok(None);
    };
    uuid::Uuid::parse_str(&id).map_err(|_| invalid())?;
    let record: ArchiveRecord = serde_json::from_str(&body)?;
    record.validate().map_err(|_| invalid())?;
    if scope(&record) != *owner {
        return Err(invalid());
    }
    Ok(Some(CommentReceipt { id, record, body }))
}
impl Journal {
    pub fn review_comment(
        &self,
        receipt: &CommentReceipt,
        file: &crate::slack::models::File,
        root: &crate::slack::models::Message,
    ) -> Result<CommentReview, AppError> {
        if read_comment(&self.conn, &scope(receipt.record()))?.as_ref() != Some(receipt) {
            return Err(invalid());
        }
        let observed = receipt
            .record
            .review_current(file, root)
            .map_err(|_| invalid())?;
        Ok(CommentReview {
            account: self.account.clone(),
            receipt: receipt.clone(),
            observed: serde_json::to_string(&observed)?,
        })
    }
    /// Explicit local disposition retains the original request and observed root.
    /// It does not certify the remote outcome or resend the original operation.
    pub fn resolve_comment_locally(
        &mut self,
        review: CommentReview,
        _decision: CommentDecision,
    ) -> Result<bool, AppError> {
        if review.account != self.account {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scope = scope(review.receipt.record());
        if read_comment(&tx, &scope)?.as_ref() != Some(&review.receipt) {
            return Ok(false);
        }
        let bytes:i64=tx.query_row("SELECT coalesce(sum(length(CAST(body AS BLOB))+length(CAST(observed AS BLOB))),0) FROM comment_reviews",[],|row|row.get(0))?;
        if bytes < 0
            || bytes as usize + review.receipt.body.len() + review.observed.len() > 64 * 1024 * 1024
        {
            return Err(invalid());
        }
        tx.execute("INSERT INTO comment_reviews(workspace,user,file,operation,body,observed,decision) VALUES(?1,?2,?3,?4,?5,?6,'local_unlock_outcome_unknown')",
            params![scope.workspace,scope.user,scope.file,review.receipt.id,review.receipt.body,review.observed])?;
        let count=tx.execute("DELETE FROM comment_receipts WHERE workspace=?1 AND user=?2 AND file=?3 AND operation=?4 AND body=?5",
            params![scope.workspace,scope.user,scope.file,review.receipt.id,review.receipt.body])?;
        if count != 1 {
            return Err(invalid());
        }
        tx.commit()?;
        Ok(true)
    }
    pub fn comment_review_count(&self, owner: &Scope) -> Result<usize, AppError> {
        scope_valid(owner)?;
        let count:i64=self.conn.query_row("SELECT count(*) FROM comment_reviews WHERE workspace=?1 AND user=?2 AND file=?3 AND decision='local_unlock_outcome_unknown'",
            params![owner.workspace,owner.user,owner.file],|row|row.get(0))?;
        usize::try_from(count).map_err(|_| invalid())
    }
    pub fn comment(&self, owner: &Scope) -> Result<Option<CommentReceipt>, AppError> {
        let tx = self.conn.unchecked_transaction()?;
        // A structural receipt is a whole-file write lock, not comment authority.
        if super::replies::present(&tx, owner)? {
            return Err(invalid());
        }
        let occupied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM receipts WHERE workspace=?1 AND user=?2 AND file=?3)",
            params![owner.workspace, owner.user, owner.file],
            |r| r.get(0),
        )?;
        if occupied {
            return Err(invalid());
        }
        let result = read_comment(&tx, owner)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn begin_comment(&mut self, record: ArchiveRecord) -> Result<CommentBegin, AppError> {
        record.validate().map_err(|_| invalid())?;
        let owner = scope(&record);
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = read_comment(&tx, &owner)? {
            return Ok(CommentBegin::Blocked(existing));
        }
        if read(&tx, &owner)?.is_some() {
            return Err(invalid());
        }
        let count: i64 = tx.query_row(
            "SELECT (SELECT count(*) FROM receipts)+(SELECT count(*) FROM comment_receipts)+(SELECT count(*) FROM reply_attempts)",
            [],
            |r| r.get(0),
        )?;
        if count >= MAX_RECEIPTS {
            return Err(invalid());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let body = serde_json::to_string(&record)?;
        if body.len() > MAX_COMMENT_BODY {
            return Err(invalid());
        }
        tx.execute(
            "INSERT INTO comment_receipts VALUES(?1,?2,?3,?4,?5)",
            params![owner.workspace, owner.user, owner.file, id, body],
        )?;
        tx.commit()?;
        Ok(CommentBegin::Started(CommentReceipt { id, record, body }))
    }
    /// Exact owner can retire only after definite non-dispatch or an acknowledged
    /// mutation followed by independently checked canonical readback.
    pub fn complete_comment(&mut self, receipt: &CommentReceipt) -> Result<bool, AppError> {
        let owner = scope(&receipt.record);
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "DELETE FROM comment_receipts WHERE workspace=?1 AND user=?2 AND file=?3 AND operation=?4 AND body=?5",
            params![owner.workspace, owner.user, owner.file, receipt.id, receipt.body])?;
        tx.commit()?;
        Ok(changed == 1)
    }
}

#[cfg(test)]
mod tests;
