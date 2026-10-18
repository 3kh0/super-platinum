//! Local edit custody is separate from remote write admission. Every mutation
//! still needs reserve's complete fresh baseline and exact current revision.
use super::*;

impl Journal {
    /// Save user input before any preflight await. This does not authorize a write.
    /// Keep the last reviewed server version alongside the new requested payload.
    pub fn retain_edit(
        &mut self,
        expected: &Record,
        note: Vec<Value>,
        post_at: i64,
        now: u64,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = exact(&tx, expected)?;
        if !matches!(
            record.phase,
            Phase::Ready | Phase::Rejected | Phase::Confirmed
        ) || (post_at == 0 && record.reviewed.is_none())
        {
            return Err(invalid(
                "Review this retained forward before changing its input",
            ));
        }
        if post_at != 0 {
            crate::scheduled::validate_time(
                post_at,
                i64::try_from(now).map_err(|_| invalid("Invalid clock"))?,
            )
            .map_err(invalid)?;
        }
        record.intent.blocks = append_reference(&note, &record.intent.source)?;
        record.intent.note = note;
        record.intent.post_at = post_at;
        record.action = if post_at == 0 {
            Action::SaveAsDraft
        } else {
            Action::Schedule
        };
        record.phase = Phase::Ready;
        record.observed = None;
        record.outcome="Your updated note and time are saved locally. Fresh draft review is required before a write.".into();
        advance(&mut record, now)?;
        put(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }

    /// Capture a complete conflicting preflight without discarding local input.
    /// Equality with the baseline grants no capability; reserve rechecks it.
    pub fn observe_preflight(
        &mut self,
        expected: &Record,
        response: &Value,
        now: u64,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = exact(&tx, expected)?;
        if !matches!(
            record.phase,
            Phase::Ready | Phase::Rejected | Phase::Confirmed
        ) || record.reviewed.is_none()
            || !request::complete_for(&record, response)
        {
            return Err(invalid("No complete current scheduled-forward preflight"));
        }
        if record.reviewed.as_ref().and_then(|v| v.get("draft")) != response.get("draft")
            || response["draft"]["is_deleted"] != false
            || response["draft"]["is_sent"] != false
        {
            record.phase = Phase::Conflict;
            record.observed = Some(response.clone());
            record.outcome="Slack returned a different draft. Your saved note and time and the observed version are both retained; no write was made.".into();
            advance(&mut record, now)?;
            put(&tx, &record)?;
        }
        tx.commit()?;
        Ok(record)
    }

    /// A preflight/read rate limit survives restart. No expiry dispatches work.
    pub fn defer_read(
        &mut self,
        expected: &Record,
        seconds: u64,
        now: u64,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = exact(&tx, expected)?;
        if record.phase == Phase::Deleted {
            return Err(invalid("The scheduled forward is deleted"));
        }
        record.retry_at = record.retry_at.max(now.saturating_add(seconds));
        advance(&mut record, now)?;
        put(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }

    /// The immutable proof is retained by the caller on all local failures.
    /// It cannot prepare a request, and cannot release a prepared request.
    pub fn save_unsent(&mut self, proof: &Unsent) -> Result<Record, AppError> {
        if proof.attempt.request_taken {
            return Err(invalid("A prepared request cannot be declared unsent"));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = load(&tx, proof.attempt.record.id())?
            .ok_or_else(|| invalid("Missing retained forward"))?;
        if record.attempt == proof.attempt.record.attempt
            && record.phase == Phase::Ready
            && record.response_recorded
        {
            tx.commit()?;
            return Ok(record);
        }
        record = exact(&tx, &proof.attempt.record)?;
        record.phase = Phase::Ready;
        record.response_recorded = true;
        record.outcome="No request was prepared; your complete input is retained for an explicit schedule action.".into();
        advance(&mut record, proof.completed_at)?;
        put(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }
}
