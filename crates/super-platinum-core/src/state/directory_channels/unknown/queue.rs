//! Source actions join the existing ordered queue; receipt-time classification
//! remains the caller's shadow, never an eagerly replaced canonical Store.
use super::*;
impl Manager {
    #[allow(clippy::too_many_arguments)]
    pub fn queue_joined_memberships(
        &self,
        scope: &Scope,
        flight: &Flight,
        store: &Store,
        observed: &Observation,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        ids: &[String],
    ) -> Result<Commit> {
        self.queue_action(
            scope,
            flight,
            store,
            observed,
            generation,
            native,
            actions::Action::Joined(ids.to_vec()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn queue_unchanged_channels(
        &self,
        scope: &Scope,
        flight: &Flight,
        store: &Store,
        observed: &Observation,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        ids: &[String],
    ) -> Result<Commit> {
        self.queue_action(
            scope,
            flight,
            store,
            observed,
            generation,
            native,
            actions::Action::Unchanged(ids.to_vec()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn queue_action(
        &self,
        scope: &Scope,
        flight: &Flight,
        store: &Store,
        observed: &Observation,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        action: actions::Action,
    ) -> Result<Commit> {
        if !self.flight_current(scope, flight)
            || flight.terminal.load(Ordering::Acquire) != 0
            || !observed.current(
                store,
                &scope.account,
                &scope.owner,
                scope.epoch,
                generation,
                native,
            )
            || self.phase.as_ref().is_none_or(|phase| {
                !store
                    .lifecycle
                    .accepts(phase, &scope.account, &scope.owner, scope.epoch)
            })
            || action.ids().iter().any(|id| !flight.ids.contains(id))
        {
            return Err(Error::MissingObservation);
        }
        for id in action.ids() {
            projection::id(&serde_json::json!({"id":id}))?;
        }
        let mut next = store.clone();
        next.enqueue_batch(Batch {
            origin: Origin::Direct,
            rows: vec![],
            action: Some(action),
            boot_in_flight: None,
            receipt_facts: None,
        })?;
        Ok(Commit {
            store: next,
            native: native.clone(),
        })
    }
}
impl Store {
    /// Only a source-manager settlement can enqueue terminal runtime objects.
    /// Native caller retains exact transport/receipt authority across awaits.
    pub fn enqueue_runtime_missing(
        &mut self,
        observed: &Observation,
        scope: &Scope,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        missing: &Missing,
    ) -> Result<()> {
        if !missing.scope.matches(scope)
            || missing.identity.load(Ordering::Acquire) != missing.attempt
            || missing.terminal.load(Ordering::Acquire) != 1
            || !self
                .lifecycle
                .accepts(&missing.phase, &scope.account, &scope.owner, scope.epoch)
            || !observed.current(
                self,
                &scope.account,
                &scope.owner,
                scope.epoch,
                generation,
                native,
            )
        {
            return Err(Error::MissingObservation);
        }
        let rows = missing
            .ids
            .iter()
            .map(|id| serde_json::json!({"id":id,"isNonExistent":true}))
            .collect();
        self.enqueue_checked(
            &scope.account,
            &scope.owner,
            scope.epoch,
            Origin::RuntimeMissing,
            rows,
        )?;
        missing.terminal.store(2, Ordering::Release);
        Ok(())
    }
}
