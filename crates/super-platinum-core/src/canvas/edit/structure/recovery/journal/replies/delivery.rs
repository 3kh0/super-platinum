use super::*;
impl Journal {
    /// Only this transaction's caller may continue its current deliberate send.
    /// An identical restored row always returns None and cannot be re-admitted.
    pub fn begin_reply(
        &mut self,
        expected: &ReplyInventory,
        record: ReplyRecord,
    ) -> Result<Option<ReplyReceipt>, AppError> {
        record.validate().map_err(|_| invalid())?;
        let [draft] = expected.drafts.as_slice() else {
            return Err(invalid());
        };
        let value = ReplyReceipt {
            id: record.client_msg_id().into(),
            record,
            draft: draft.clone(),
        };
        receipt_valid(&value, &expected.scope)?;
        if value.record.root_ts() != expected.root || expected.attempt.is_some() {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if read(&tx, &expected.scope, &expected.root)? != *expected
            || super::super::read(&tx, &expected.scope)?.is_some()
            || super::super::comments::present(&tx, &expected.scope)?
        {
            return Ok(None);
        }
        let total:i64=tx.query_row("SELECT (SELECT count(*) FROM receipts)+(SELECT count(*) FROM comment_receipts)+(SELECT count(*) FROM reply_attempts)",[],|r|r.get(0))?;
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM reply_history WHERE id=?1)",
            [&value.id],
            |r| r.get(0),
        )?;
        if total >= MAX_RECEIPTS || used {
            return Err(invalid());
        }
        tx.execute(
            "INSERT INTO reply_attempts VALUES(?1,?2,?3,?4,?5)",
            params![
                expected.scope.workspace,
                expected.scope.user,
                expected.scope.file,
                value.id,
                encoded(&value)?
            ],
        )?;
        advance(&tx)?;
        tx.commit()?;
        Ok(Some(value))
    }
    /// Definitely no transport handoff. Draft content is retained, including
    /// any newer local version. The old attempt is still permanently recorded.
    pub fn reply_not_sent(&mut self, receipt: &ReplyReceipt) -> Result<bool, AppError> {
        self.finish_reply(receipt, "not_sent", None, false)
    }
    pub fn reply_acknowledged(
        &mut self,
        receipt: &ReplyReceipt,
        accepted: &ReplyAccepted,
    ) -> Result<bool, AppError> {
        if accepted.record() != receipt.record() {
            return Err(invalid());
        }
        self.finish_reply(
            receipt,
            "acknowledged",
            Some(serde_json::to_value(accepted.message())?),
            true,
        )
    }
    /// Explicit review never classifies delivery. The full exact inventory must
    /// still match, and the original attempt/text is retained in history.
    pub fn archive_unconfirmed_reply(
        &mut self,
        expected: &ReplyInventory,
        observed: &crate::slack::models::Message,
    ) -> Result<bool, AppError> {
        let receipt = expected.attempt.as_ref().ok_or_else(invalid)?;
        if receipt.record.root_ts() != expected.root || !receipt.record.same_root(observed) {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if read(&tx, &expected.scope, &expected.root)? != *expected {
            return Ok(false);
        }
        finish(
            &tx,
            &expected.scope,
            receipt,
            "reviewed_outcome_unknown",
            Some(serde_json::to_value(observed)?),
            true,
        )?;
        advance(&tx)?;
        tx.commit()?;
        Ok(true)
    }
    fn finish_reply(
        &mut self,
        receipt: &ReplyReceipt,
        status: &str,
        observed: Option<serde_json::Value>,
        clear_exact: bool,
    ) -> Result<bool, AppError> {
        let scope = Scope {
            workspace: receipt.record.workspace().into(),
            user: receipt.record.user().into(),
            file: receipt.record.file().into(),
        };
        receipt_valid(receipt, &scope)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if read(&tx, &scope, receipt.record.root_ts())?
            .attempt
            .as_ref()
            != Some(receipt)
        {
            let row: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT body,observed,status FROM reply_history WHERE id=?1",
                    [&receipt.id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((body, prior, old_status)) = row else {
                return Ok(false);
            };
            if body != encoded(receipt)? {
                return Err(invalid());
            }
            let encoded_observed = encoded(&observed)?;
            if old_status == status && prior == encoded_observed {
                return Ok(true);
            }
            if old_status == "reviewed_outcome_unknown" && status == "acknowledged" {
                let previous: Option<String> = tx
                    .query_row(
                        "SELECT observed FROM reply_late_results WHERE id=?1",
                        [&receipt.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if previous.as_ref().is_some_and(|p| p != &encoded_observed) {
                    return Err(invalid());
                }
                tx.execute(
                    "INSERT OR IGNORE INTO reply_late_results VALUES(?1,?2)",
                    params![receipt.id, encoded_observed],
                )?;
                advance(&tx)?;
                tx.commit()?;
                return Ok(true);
            }
            return Ok(false);
        }
        finish(&tx, &scope, receipt, status, observed, clear_exact)?;
        advance(&tx)?;
        tx.commit()?;
        Ok(true)
    }
}
fn finish(
    tx: &Connection,
    scope: &Scope,
    receipt: &ReplyReceipt,
    status: &str,
    observed: Option<serde_json::Value>,
    clear_exact: bool,
) -> Result<(), AppError> {
    let body = encoded(receipt)?;
    let observed = encoded(&observed)?;
    // Per-row bounded immutable evidence; indexed paged readers never scan all history.
    tx.execute(
        "INSERT INTO reply_history VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            scope.workspace,
            scope.user,
            scope.file,
            receipt.id,
            body,
            observed,
            status
        ],
    )?;
    if clear_exact {
        tx.execute("DELETE FROM reply_drafts WHERE workspace=?1 AND user=?2 AND file=?3 AND root=?4 AND id=?5 AND body=?6",params![scope.workspace,scope.user,scope.file,receipt.record.root_ts(),receipt.draft.id,encoded(&receipt.draft)?])?;
    }
    if tx.execute("DELETE FROM reply_attempts WHERE workspace=?1 AND user=?2 AND file=?3 AND id=?4 AND body=?5",params![scope.workspace,scope.user,scope.file,receipt.id,body])?!=1{return Err(invalid())}
    Ok(())
}
