use super::*;

impl Store {
    /// Explicit archive decision + distinct EMPTY draft, atomically. No network retry.
    /// Caller supplies a stable operation UUID so a lost commit reply is recoverable.
    pub fn archive_and_new(
        &mut self,
        expected_draft: &Draft,
        expected_attempt: &Attempt,
        reason: ArchiveReason,
        operation: &str,
        new_writer: &str,
        now: u64,
    ) -> Result<ArchiveReceipt> {
        self.owner(&expected_draft.owner)?;
        self.owner(&expected_attempt.owner)?;
        expected_draft.validate()?;
        expected_attempt.validate()?;
        uuid(operation)?;
        uuid(new_writer)?;
        clock(now)?;
        if expected_draft.owner != expected_attempt.owner
            || expected_draft.id != expected_attempt.draft_id
            || expected_draft.revision != expected_attempt.draft_revision
            || expected_draft.content != expected_attempt.frozen
            || expected_draft.attempt.as_deref() != Some(expected_attempt.id.as_str())
            || expected_draft.archived
        {
            return Err(Fault::Invalid);
        }
        let allowed = match reason {
            ArchiveReason::AbandonedOutcomeUnknown => matches!(
                expected_attempt.delivery,
                Delivery::Started | Delivery::Uncertain
            ),
            ArchiveReason::CreatedReviewed => {
                matches!(expected_attempt.delivery, Delivery::Created { .. })
            }
        };
        if !allowed {
            return Err(Fault::Blocked);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        if let Some(receipt) = read::<ArchiveReceipt>(&tx, &expected_draft.owner, operation)? {
            if receipt.old_draft == expected_draft.id
                && receipt.expected_draft_revision == expected_draft.revision
                && receipt.expected_decision_revision == expected_draft.decision_revision
                && receipt.old_attempt == expected_attempt.id
                && receipt.expected_result_revision == expected_attempt.result_revision
                && receipt.expected_delivery == expected_attempt.delivery
                && receipt.reason == reason
                && receipt.new_writer == new_writer
            {
                // Recovery returns the immutable decision, not a replacement current UI snapshot.
                let mut old = read::<Draft>(&tx, &receipt.owner, &receipt.old_draft)?
                    .ok_or(Fault::Invalid)?;
                if !old.archived
                    || old.decision_revision != next(receipt.expected_decision_revision)?
                {
                    return Err(Fault::Invalid);
                }
                old.archived = false;
                old.decision_revision = receipt.expected_decision_revision;
                if old != *expected_draft {
                    return Err(Fault::Stale);
                }
                return Ok(receipt);
            }
            return Err(Fault::Stale);
        }
        reject_retired_key(&tx, "decisions", operation)?;
        exact_active(&tx, expected_draft)?;
        if association(&tx, expected_draft)?.as_ref() != Some(expected_attempt) {
            return Err(Fault::Stale);
        }
        let mut scope = scope(&tx, &expected_draft.owner)?.ok_or(Fault::Invalid)?;
        if now < scope.retry_at {
            return Err(Fault::Cooldown);
        }
        if count(&tx, "drafts")? >= MAX_SCOPES + MAX_ATTEMPTS
            || count(&tx, "decisions")? >= MAX_ATTEMPTS
        {
            return Err(Fault::Capacity);
        }
        if active_scope_count(&tx)? > MAX_SCOPES {
            return Err(Fault::Capacity);
        }
        let new = Draft::fresh(expected_draft.owner.clone(), new_writer, Content::empty())?;
        let receipt = ArchiveReceipt {
            operation: operation.into(),
            owner: expected_draft.owner.clone(),
            old_draft: expected_draft.id.clone(),
            expected_draft_revision: expected_draft.revision,
            expected_decision_revision: expected_draft.decision_revision,
            old_attempt: expected_attempt.id.clone(),
            expected_result_revision: expected_attempt.result_revision,
            expected_delivery: expected_attempt.delivery.clone(),
            reason,
            new_draft: new.id.clone(),
            new_writer: new_writer.into(),
        };
        let mut archived = expected_draft.clone();
        archived.archived = true;
        archived.decision_revision = next(archived.decision_revision)?;
        scope.active = Some(new.id.clone());
        put(&tx, &archived)?;
        insert(&tx, &new)?;
        insert(&tx, &receipt)?;
        put(&tx, &scope)?;
        tx.commit()?;
        Ok(receipt)
    }
    pub fn archive_receipt(
        &self,
        owner: &Owner,
        operation: &str,
    ) -> Result<Option<ArchiveReceipt>> {
        self.owner(owner)?;
        uuid(operation)?;
        self.read_snapshot(|conn| read(conn, owner, operation))
    }
    pub fn archived(&self, owner: &Owner) -> Result<Vec<Draft>> {
        self.owner(owner)?;
        self.read_snapshot(|conn| {
            let selected: Vec<_> = all::<Draft>(conn, &self.account)?
                .into_iter()
                .filter(|d| d.owner == *owner && d.archived)
                .collect();
            for draft in &selected {
                association(conn, draft)?;
            }
            Ok(selected)
        })
    }
}
