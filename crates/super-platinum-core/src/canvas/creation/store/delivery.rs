use super::*;

impl Store {
    pub fn reserve(&mut self, expected: &Draft, now: u64) -> Result<Reservation> {
        self.owner(&expected.owner)?;
        expected.validate()?;
        clock(now)?;
        if expected.content.title().trim().is_empty() {
            return Err(Fault::Invalid);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        exact_active(&tx, expected)?;
        editable(&tx, expected)?;
        let scope = scope(&tx, &expected.owner)?.ok_or(Fault::Invalid)?;
        if now < scope.retry_at {
            return Err(Fault::Cooldown);
        }
        if integrity::has_conflicts(&tx, &expected.owner)? {
            return Err(Fault::Blocked);
        }
        // Outcome capacity is reserved by inserting this row before any admission.
        if count(&tx, "attempts")? >= MAX_ATTEMPTS {
            return Err(Fault::Capacity);
        }
        let attempt = Attempt {
            id: uuid::Uuid::new_v4().to_string(),
            owner: expected.owner.clone(),
            draft_id: expected.id.clone(),
            draft_revision: expected.revision,
            frozen: expected.content.clone(),
            delivery: Delivery::Reserved,
            result_revision: 1,
        };
        let mut draft = expected.clone();
        draft.attempt = Some(attempt.id.clone());
        insert(&tx, &attempt)?;
        put(&tx, &draft)?;
        tx.commit()?;
        Ok(Reservation { draft, attempt })
    }
    /// Restore the exact Reserved address for cancellation/admission, not a permit.
    pub fn reservation(&self, owner: &Owner, id: &str) -> Result<Reservation> {
        self.owner(owner)?;
        uuid(id)?;
        self.read_snapshot(|conn| {
            let attempt = read_attempt(conn, owner, id)?.ok_or(Fault::Stale)?;
            if !matches!(attempt.delivery, Delivery::Reserved) {
                return Err(Fault::Blocked);
            }
            let draft = active(conn, owner)?.ok_or(Fault::Stale)?;
            if draft.attempt.as_deref() != Some(id) {
                return Err(Fault::Stale);
            }
            Ok(Reservation { draft, attempt })
        })
    }
    pub fn admit(&mut self, reservation: &Reservation, now: u64) -> Result<DispatchPermit> {
        self.owner(&reservation.attempt.owner)?;
        clock(now)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        verify_reserved(&tx, reservation)?;
        let scope = scope(&tx, &reservation.attempt.owner)?.ok_or(Fault::Invalid)?;
        if now < scope.retry_at {
            return Err(Fault::Cooldown);
        }
        if integrity::has_conflicts(&tx, &reservation.attempt.owner)? {
            return Err(Fault::Blocked);
        }
        let mut started = reservation.attempt.clone();
        started.delivery = Delivery::Started;
        started.result_revision = next(started.result_revision)?;
        put(&tx, &started)?;
        // A lost commit reply cannot be recovered as another permit: row is Started.
        tx.commit()?;
        Ok(DispatchPermit(started))
    }
    pub fn cancel_reserved(&mut self, reservation: &Reservation) -> Result<Attempt> {
        self.owner(&reservation.attempt.owner)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        verify_reserved(&tx, reservation)?;
        let mut canceled = reservation.attempt.clone();
        canceled.delivery = Delivery::CanceledBeforeAdmission;
        canceled.result_revision = next(canceled.result_revision)?;
        put(&tx, &canceled)?;
        tx.commit()?;
        Ok(canceled)
    }
    /// Exact original delivery CAS. Archival/new active draft is intentionally irrelevant.
    /// An equal already-committed result is idempotently recoverable, never dispatchable.
    pub fn settle(&mut self, expected: &Attempt, outcome: Outcome) -> Result<Attempt> {
        self.owner(&expected.owner)?;
        expected.validate()?;
        if !matches!(expected.delivery, Delivery::Started | Delivery::Uncertain) {
            return Err(Fault::Blocked);
        }
        let delivery = outcome.delivery()?;
        let mut after = expected.clone();
        after.delivery = delivery;
        after.result_revision = next(expected.result_revision)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        let actual = read::<Attempt>(&tx, &expected.owner, &expected.id)?.ok_or(Fault::Stale)?;
        if actual == after {
            let draft =
                read::<Draft>(&tx, &expected.owner, &expected.draft_id)?.ok_or(Fault::Invalid)?;
            let scope = scope(&tx, &expected.owner)?.ok_or(Fault::Invalid)?;
            if draft.revision < expected.draft_revision
                || (draft.revision == expected.draft_revision && draft.content != expected.frozen)
                || matches!(after.delivery, Delivery::RateLimited { retry_at } if scope.retry_at < retry_at)
            {
                return Err(Fault::Invalid);
            }
            return Ok(actual);
        }
        if actual != *expected {
            return Err(Fault::Stale);
        }
        let draft =
            read::<Draft>(&tx, &expected.owner, &expected.draft_id)?.ok_or(Fault::Invalid)?;
        if draft.revision != expected.draft_revision
            || draft.content != expected.frozen
            || draft.attempt.as_deref() != Some(expected.id.as_str())
        {
            return Err(Fault::Invalid);
        }
        let mut scope = scope(&tx, &expected.owner)?.ok_or(Fault::Invalid)?;
        if let Delivery::RateLimited { retry_at } = after.delivery {
            scope.retry_at = scope.retry_at.max(retry_at);
            put(&tx, &scope)?;
        }
        put(&tx, &after)?;
        tx.commit()?;
        Ok(after)
    }
}
fn verify_reserved(conn: &Connection, reservation: &Reservation) -> Result<()> {
    reservation.draft.validate()?;
    reservation.attempt.validate()?;
    if !matches!(reservation.attempt.delivery, Delivery::Reserved)
        || reservation.draft.owner != reservation.attempt.owner
    {
        return Err(Fault::Invalid);
    }
    exact_active(conn, &reservation.draft)?;
    if association(conn, &reservation.draft)?.as_ref() != Some(&reservation.attempt) {
        return Err(Fault::Stale);
    }
    Ok(())
}
