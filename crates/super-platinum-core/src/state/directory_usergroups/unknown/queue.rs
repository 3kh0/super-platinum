use super::*;
impl Store {
    /// The only manager-terminal constructor. The unchanged JJAw Metadata path
    /// merges effective state; the raw row remains exactly {id,isNonExistent}.
    pub fn enqueue_missing(
        &mut self,
        missing: &Missing,
        native: &Native,
        generation: &str,
    ) -> Result<Ticket> {
        let a = &missing.authority;
        if missing.ids.is_empty()
            || a.alive.load(Ordering::Acquire) != 1
            || a.identity.load(Ordering::Acquire) != a.attempt
            || a.phase.load(Ordering::Acquire) != 1
            || !missing.observed.current(
                self,
                &missing.scope.account,
                &missing.scope.owner,
                missing.scope.epoch,
                native,
            )
        {
            return Err(Error::Stale);
        }
        let mut completed = missing.completed.lock().map_err(|_| Error::Stale)?;
        if completed.is_some() {
            return Err(Error::Stale);
        }
        let rows = missing
            .ids
            .iter()
            .map(|id| serde_json::json!({"id":id,"isNonExistent":true}))
            .collect();
        let ticket = self.enqueue(
            &missing.scope.account,
            &missing.scope.owner,
            missing.scope.epoch,
            generation,
            native,
            Operation::Metadata {
                origin: Origin::Upsert,
                rows,
                native_members: NativeMembers::Preserve,
            },
        )?;
        self.queue.last_mut().expect("enqueued").terminal = Some(a.clone());
        *completed = Some(ticket.completed.clone());
        a.phase.store(2, Ordering::Release);
        Ok(ticket)
    }
}
