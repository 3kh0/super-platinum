//! Explicit local conflict decision. It prepares an editable retained record,
//! never a request. The next write still needs fresh full preflight and reserve.
use super::*;

impl Journal {
    pub fn resolve_conflict(
        &mut self,
        expected: &Record,
        fresh: &Value,
        now: u64,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = exact(&tx, expected)?;
        if record.phase != Phase::Conflict
            || now < record.retry_at
            || record
                .observed
                .as_ref()
                .and_then(|value| value.get("draft"))
                != fresh.get("draft")
            || !request::complete_for(&record, fresh)
            || fresh["draft"]["is_deleted"] != false
            || fresh["draft"]["is_sent"] != false
        {
            return Err(invalid(
                "Read the same complete active conflicting draft before choosing a version",
            ));
        }
        // Preserve the user's note/time and both prior versions in history. The
        // explicitly reviewed Slack row supplies only the next preflight basis.
        record.reviewed = Some(fresh.clone());
        record.phase = Phase::Ready;
        record.action = if record.intent.post_at == 0 {
            Action::SaveAsDraft
        } else {
            Action::Schedule
        };
        record.outcome = "Your requested note and time are retained against the reviewed Slack version. Edit them or explicitly save; nothing has been sent.".into();
        advance(&mut record, now)?;
        put(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }
}
