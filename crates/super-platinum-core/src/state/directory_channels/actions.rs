//! Ordered direct source actions following accepted raw processor batches.
//! Action IDs carry no fetched row or renderer output. The existing canonical
//! Store drains them against the preceding committed source, never later rows.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum Action {
    Joined(Vec<String>),
    Unchanged(Vec<String>),
}
impl Action {
    pub(super) fn ids(&self) -> &[String] {
        match self {
            Self::Joined(ids) | Self::Unchanged(ids) => ids,
        }
    }
}
impl Batch {
    pub(super) fn units(&self) -> usize {
        self.rows.len() + self.action.as_ref().map_or(0, |a| a.ids().len())
    }
    pub(super) fn valid_action(&self) -> bool {
        self.action.as_ref().is_none_or(|action| {
            self.origin == Origin::Direct
                && self.rows.is_empty()
                && self.boot_in_flight.is_none()
                && self.receipt_facts.is_none()
                && !action.ids().is_empty()
                && action
                    .ids()
                    .iter()
                    .all(|id| projection::id(&serde_json::json!({"id":id})).is_ok())
        })
    }
}
impl freshness::Manager {
    /// Closed eq/eV/eH(true) action under an original conditional batch. The
    /// caller applies the same action synchronously to its receipt shadow.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_joined_memberships(
        &self,
        scope: &freshness::Scope,
        batch: &freshness::Batch,
        store: &Store,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        joined: &[String],
    ) -> Result<Commit> {
        self.queue_action(
            scope,
            batch,
            store,
            observed,
            account,
            owner,
            epoch,
            generation,
            native,
            Action::Joined(joined.to_vec()),
        )
    }
    /// HP6x action: callers remove returned and actually missing selector IDs
    /// using their original post-action predecessor before enqueueing.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_unchanged_channels(
        &self,
        scope: &freshness::Scope,
        batch: &freshness::Batch,
        store: &Store,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        ids: &[String],
    ) -> Result<Commit> {
        self.queue_action(
            scope,
            batch,
            store,
            observed,
            account,
            owner,
            epoch,
            generation,
            native,
            Action::Unchanged(ids.to_vec()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn queue_action(
        &self,
        scope: &freshness::Scope,
        batch: &freshness::Batch,
        store: &Store,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        action: Action,
    ) -> Result<Commit> {
        let expected = freshness::Scope::new(account, owner, epoch)?;
        if !self.batch_current(&expected, batch)
            || !self.batch_current(scope, batch)
            || !observed.current(store, account, owner, epoch, generation, native)
            || action.ids().iter().any(|id| !batch.ids().contains(id))
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
    /// HP6x from an admitted client.channels response, following that response's
    /// Channels action. The native caller retains its original split publication
    /// permit; this observation does not create a conditional-fetch capability.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_resync_unchanged(
        &self,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        ids: &[String],
    ) -> Result<Commit> {
        if !observed.current(self, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        for id in ids {
            projection::id(&serde_json::json!({"id":id}))?;
        }
        let mut next = self.clone();
        if !ids.is_empty() {
            next.enqueue_batch(Batch {
                origin: Origin::Direct,
                rows: vec![],
                action: Some(Action::Unchanged(ids.to_vec())),
                boot_in_flight: None,
                receipt_facts: None,
            })?;
        }
        Ok(Commit {
            store: next,
            native: native.clone(),
        })
    }
    /// Drain only a front action, after all earlier raw batches have committed.
    /// Later batches remain queued and cannot change this action's predecessor.
    /// The caller retains original publication authority and installs the pair
    /// synchronously. No new request, renderer or complete inventory is minted.
    #[allow(clippy::too_many_arguments)]
    pub fn drain_source_action(
        &self,
        observed: &Observation,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<Option<Commit>> {
        if !observed.current(self, account, owner, epoch, generation, native) {
            return Err(Error::MissingObservation);
        }
        let saved = self.identity(account, owner)?;
        let Some(action) = saved
            .pending
            .first()
            .and_then(|batch| batch.action.as_ref())
        else {
            return Ok(None);
        };
        let later = saved.pending[1..].to_vec();
        // Reuse the exact reviewed eH/HP6x direct reducers against the action's
        // current predecessor. Clearing this PRIVATE copy's queue does not
        // discard canonical work; the remaining suffix is restored below.
        let mut predecessor = self.clone();
        predecessor.saved.as_mut().unwrap().pending.clear();
        let mut commit = match action {
            Action::Joined(ids) => joined::apply_joined(&predecessor, native, ids)?,
            Action::Unchanged(ids) => {
                let captured =
                    predecessor.observation(account, owner, epoch, generation, native)?;
                let (commit, missing) = predecessor
                    .process_unchanged_channels(
                        &captured,
                        account,
                        owner,
                        epoch,
                        generation,
                        native,
                        ids,
                        &[],
                    )?
                    .into_parts();
                // Absence was classified at the original receipt, not invented
                // at renderer completion. A changed predecessor refuses.
                if !missing.is_empty() {
                    return Err(Error::MissingObservation);
                }
                commit
            }
        };
        commit.store.saved.as_mut().unwrap().pending = later;
        commit.store.changed();
        Ok(Some(commit))
    }
}

#[cfg(test)]
mod tests;
