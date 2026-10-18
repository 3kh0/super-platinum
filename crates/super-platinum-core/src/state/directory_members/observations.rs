//! Partial source observations are not Ready or permission evidence.
use super::*;
use std::collections::BTreeMap;

/// Read-only initialization diagnostic, never a selection or reset permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryState {
    Unknown,
    Overflowed,
    CurrentIdentity,
}

pub struct Observations {
    stamp: Stamp,
    interactions: BTreeMap<String, Option<bool>>,
    disconnected: BTreeMap<String, bool>,
}
impl Observations {
    /// Retained source and interaction preimages for conditional reacquisition.
    /// These may be invalidated or disagree with the typed native projection.
    /// They grant neither selection availability nor permission. The caller
    /// must also check `current` with its original account/session/runtime scope.
    /// Only the requested 1..=100 rows are cloned, in request encounter order.
    pub fn rows(&self, store: &Store, requested: &[String]) -> Result<Vec<Current>, Error> {
        if !Arc::ptr_eq(&self.stamp.epoch, &store.epoch)
            || self.stamp.revision != store.revision
            || requested.is_empty()
            || requested.len() > 100
        {
            return Err(unavailable());
        }
        let saved = store.saved.as_ref().ok_or_else(unavailable)?;
        if saved.overflowed || !saved.pending.is_empty() {
            return Err(unavailable());
        }
        let mut wanted = BTreeSet::new();
        for id in requested {
            member_id(&serde_json::json!({"id":id}))?;
            if !wanted.insert(id.as_str()) {
                return Err(unavailable());
            }
        }
        let found: BTreeMap<_, _> = saved
            .records
            .iter()
            .filter_map(|record| {
                let id = record.source["id"].as_str()?;
                wanted.contains(id).then_some((id, record))
            })
            .collect();
        requested
            .iter()
            .map(|id| {
                found
                    .get(id.as_str())
                    .map(|r| r.current())
                    .ok_or_else(unavailable)
            })
            .collect()
    }
    /// Existing key + None is observed absence. Missing key is unknown.
    pub fn interactions(&self) -> &BTreeMap<String, Option<bool>> {
        &self.interactions
    }
    /// Retained raw-source identities only, never native cache membership.
    /// The snapshot's currentness and completeness limits still apply.
    pub fn source_ids(&self) -> impl Iterator<Item = &str> {
        self.interactions.keys().map(String::as_str)
    }
    /// true is an observed mark; false is an observed reset/removal.
    /// Missing keys are unknown, including legacy-cache absence.
    pub fn disconnected(&self) -> &BTreeMap<String, bool> {
        &self.disconnected
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> bool {
        store.current(&self.stamp, account, owner, epoch, generation)
    }
}
impl Store {
    /// Unknown/overflow may trigger an explicit owner-guarded acquisition reset.
    /// A different retained identity is an error and must not be auto-rebound.
    pub fn recovery_state(
        &self,
        account: &str,
        owner: &WorkspaceSession,
    ) -> Result<RecoveryState, Error> {
        let identity = Identity::capture(account, owner)?;
        match &self.saved {
            None => Ok(RecoveryState::Unknown),
            Some(saved) if saved.identity != identity => Err(unavailable()),
            Some(saved) if saved.overflowed => Ok(RecoveryState::Overflowed),
            Some(_) => Ok(RecoveryState::CurrentIdentity),
        }
    }

    /// Retained source pre-state for a new read, even when typed users need
    /// reacquisition. It cannot establish missing rows, native consistency,
    /// availability, permissions or source collection coverage.
    pub fn observations(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> Result<Observations, Error> {
        let saved = self.identity(account, owner)?;
        if saved.overflowed || !saved.pending.is_empty() {
            return Err(unavailable());
        }
        let mut interactions = BTreeMap::new();
        for record in &saved.records {
            interactions.insert(member_id(&record.source)?.to_owned(), record.interaction);
        }
        Ok(Observations {
            stamp: self.stamp(account, owner, epoch, generation)?,
            interactions,
            disconnected: saved.disconnected.clone(),
        })
    }
}
