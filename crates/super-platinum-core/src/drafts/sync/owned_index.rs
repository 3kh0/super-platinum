//! Pure routing index for already durable draft owners.
//!
//! This is not authenticated-source admission, enrollment, event reduction, or
//! permission to dispatch. Build only from a complete successful durable load;
//! invalidate/rebuild on an account visit, release, re-enrollment, or inventory
//! replacement. A fresh index has a fresh process-local lineage even if every
//! loaded byte/revision is equal. Stored rows have no durable enrollment nonce:
//! exact row recreation without host invalidation cannot be detected here.
//!
//! Build once per loaded inventory, not per frame. Lookup uses identity maps;
//! the final candidate check compares one full row under its storage owner gate
//! before the existing reducer/CAS. No visible channel or local-only draft can
//! contribute routing authority. No realtime hook or autosave is enabled.
use super::{
    DraftKey, DraftSync, OperationKind, Scope, Workflow, identifier, revision, valid_account_id,
};
use crate::drafts::store::sync::{Loaded, Stored};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityKind {
    Active,
    Deleted,
}

/// Identity already extracted from a validated event envelope. The caller
/// independently authenticates account, user, credentials and socket generation.
pub struct Identity<'a> {
    pub draft_id: &'a str,
    pub client_msg_id: Option<&'a str>,
    pub kind: IdentityKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    #[error("the source account or team is invalid")]
    Source,
    #[error("the loaded inventory contains an invalid durable owner")]
    Inventory,
    #[error("the event draft identity is malformed or incomplete")]
    Identity,
    #[error("more than one durable destination claims this identity")]
    Ambiguous,
    #[error("the server and client identities do not name the same owner")]
    Mismatch,
    #[error("the server draft identity has already been retired")]
    Retired,
    #[error("the inventory instance has been replaced")]
    StaleInventory,
    #[error("the durable owner row has changed or disappeared")]
    StaleOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authority {
    Acknowledged,
    PendingCreate,
}

type IdentityMap = BTreeMap<(String, String), BTreeSet<DraftKey>>;

struct Inventory {
    account: String,
    rows: BTreeMap<DraftKey, Stored>,
    acknowledged_ids: IdentityMap,
    client_ids: IdentityMap,
    client_claims: IdentityMap,
    // Recorded/retired identities are collision barriers, never new authority.
    id_claims: IdentityMap,
    retired_ids: BTreeSet<(String, String)>,
}

#[derive(Clone)]
pub struct OwnedDraftIndex {
    inventory: Arc<Inventory>,
}

/// An opaque candidate is bound to one immutable inventory instance and row.
/// Its key is not sufficient authority: call `revalidate` under the durable
/// owner gate, then let the existing observation reducer validate the event.
pub struct Candidate {
    inventory: Arc<Inventory>,
    key: DraftKey,
    draft_id: String,
    client_msg_id: String,
    authority: Authority,
}

fn insert(map: &mut IdentityMap, team: &str, id: &str, key: &DraftKey) {
    map.entry((team.to_owned(), id.to_owned()))
        .or_default()
        .insert(key.clone());
}

fn unique<'a>(
    map: &'a IdentityMap,
    team: &str,
    id: &str,
) -> Result<Option<&'a DraftKey>, IndexError> {
    let Some(keys) = map.get(&(team.to_owned(), id.to_owned())) else {
        return Ok(None);
    };
    if keys.len() != 1 {
        return Err(IndexError::Ambiguous);
    }
    Ok(keys.first())
}

impl OwnedDraftIndex {
    pub fn from_loaded(account: &str, loaded: &Loaded) -> Result<Self, IndexError> {
        if !valid_account_id(account) {
            return Err(IndexError::Source);
        }
        let mut inventory = Inventory {
            account: account.to_owned(),
            rows: BTreeMap::new(),
            acknowledged_ids: BTreeMap::new(),
            client_ids: BTreeMap::new(),
            client_claims: BTreeMap::new(),
            id_claims: BTreeMap::new(),
            retired_ids: BTreeSet::new(),
        };
        for (key, stored) in &loaded.synced {
            let state = &stored.state;
            let scope = state.scope();
            if scope.account_id != account
                || &scope.key != key
                || stored.revision == 0
                || stored.revision > i64::MAX as u64
                || !identifier(&key.team)
                || !identifier(&key.channel)
                || key.thread.as_deref().is_some_and(|ts| !revision(ts))
                || !valid_account_id(state.client_msg_id())
                || !state.observation_state_valid()
                || !state.recovery_state_valid()
                || !state.consumption_valid()
                || state.pending().is_some_and(|op| {
                    &op.scope != scope || op.base.as_ref().is_some_and(|base| &base.scope != scope)
                })
                || matches!(state.workflow(), Workflow::Conflict {remote:Some(remote),..} if &remote.scope != scope)
            {
                return Err(IndexError::Inventory);
            }
            if let Some(base) = state.acknowledged() {
                if &base.scope != scope
                    || !identifier(&base.id)
                    || !valid_account_id(&base.client_msg_id)
                    || base.client_msg_id != state.client_msg_id()
                {
                    return Err(IndexError::Inventory);
                }
                insert(&mut inventory.acknowledged_ids, &key.team, &base.id, key);
                insert(&mut inventory.id_claims, &key.team, &base.id, key);
                insert(
                    &mut inventory.client_ids,
                    &key.team,
                    &base.client_msg_id,
                    key,
                );
                insert(
                    &mut inventory.client_claims,
                    &key.team,
                    &base.client_msg_id,
                    key,
                );
            } else if let Some(create) = pending_create(state) {
                if !valid_account_id(&create.client_msg_id)
                    || create.client_msg_id != state.client_msg_id()
                {
                    return Err(IndexError::Inventory);
                }
                insert(
                    &mut inventory.client_ids,
                    &key.team,
                    &create.client_msg_id,
                    key,
                );
                insert(
                    &mut inventory.client_claims,
                    &key.team,
                    &create.client_msg_id,
                    key,
                );
            }
            for id in &state.retired_ids {
                if !identifier(id) {
                    return Err(IndexError::Inventory);
                }
                insert(&mut inventory.id_claims, &key.team, id, key);
                inventory.retired_ids.insert((key.team.clone(), id.clone()));
            }
            for (id, event) in &state.observed_events {
                insert(&mut inventory.id_claims, &key.team, id, key);
                insert(
                    &mut inventory.client_claims,
                    &key.team,
                    &event.client_msg_id,
                    key,
                );
            }
            inventory.rows.insert(key.clone(), stored.clone());
        }
        Ok(Self {
            inventory: Arc::new(inventory),
        })
    }

    /// Unknown identities return None. Known-but-conflicting identities fail
    /// closed; UUID lookup never rotates an acknowledged server draft ID.
    pub fn resolve(
        &self,
        account: &str,
        team: &str,
        identity: Identity<'_>,
    ) -> Result<Option<Candidate>, IndexError> {
        if account != self.inventory.account || !identifier(team) {
            return Err(IndexError::Source);
        }
        if !identifier(identity.draft_id)
            || identity
                .client_msg_id
                .is_some_and(|id| !valid_account_id(id))
            || (identity.kind == IdentityKind::Active && identity.client_msg_id.is_none())
        {
            return Err(IndexError::Identity);
        }
        let server = unique(&self.inventory.acknowledged_ids, team, identity.draft_id)?;
        let claim = unique(&self.inventory.id_claims, team, identity.draft_id)?;
        let client = match identity.client_msg_id {
            Some(id) => {
                // Historical evidence may block a duplicate claim, but cannot
                // promote an otherwise unowned UUID into a routing candidate.
                unique(&self.inventory.client_claims, team, id)?;
                unique(&self.inventory.client_ids, team, id)?
            }
            None => None,
        };
        if self
            .inventory
            .retired_ids
            .contains(&(team.to_owned(), identity.draft_id.to_owned()))
        {
            return Err(IndexError::Retired);
        }
        let (key, authority) = match (server, client, identity.client_msg_id) {
            (Some(server), Some(client), _) if server == client => {
                (server, Authority::Acknowledged)
            }
            (Some(server), None, None) if identity.kind == IdentityKind::Deleted => {
                (server, Authority::Acknowledged)
            }
            (Some(_), _, _) => return Err(IndexError::Mismatch),
            (None, Some(client), _) => {
                let state = &self.inventory.rows[client].state;
                if state.acknowledged().is_some()
                    || pending_create(state).is_none()
                    || claim.is_some_and(|owner| owner != client)
                    || state.observed_events.values().any(|event| {
                        (Some(event.client_msg_id.as_str()) == identity.client_msg_id
                            && event.draft_id != identity.draft_id)
                            || (event.draft_id == identity.draft_id
                                && Some(event.client_msg_id.as_str()) != identity.client_msg_id)
                    })
                {
                    return Err(IndexError::Mismatch);
                }
                (client, Authority::PendingCreate)
            }
            (None, None, _) => return Ok(None),
        };
        let state = &self.inventory.rows[key].state;
        let client_msg_id = match authority {
            Authority::Acknowledged => state.acknowledged().unwrap().client_msg_id.clone(),
            Authority::PendingCreate => pending_create(state).unwrap().client_msg_id.clone(),
        };
        // A UUID-less delete may borrow only an unambiguous durable UUID from
        // the exact acknowledged ID, never bypass a known duplicate claim.
        if unique(&self.inventory.client_claims, team, &client_msg_id)?
            .is_some_and(|owner| owner != key)
        {
            return Err(IndexError::Mismatch);
        }
        Ok(Some(Candidate {
            inventory: self.inventory.clone(),
            key: key.clone(),
            draft_id: identity.draft_id.into(),
            client_msg_id,
            authority,
        }))
    }
}

fn pending_create(state: &DraftSync) -> Option<&super::Operation> {
    state
        .pending()
        .filter(|op| op.kind == OperationKind::Create && op.base.is_none())
}

impl Candidate {
    pub fn scope(&self) -> &Scope {
        self.inventory.rows[&self.key].state.scope()
    }
    pub fn revision(&self) -> u64 {
        self.inventory.rows[&self.key].revision
    }
    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }
    pub fn client_msg_id(&self) -> &str {
        &self.client_msg_id
    }
    pub fn authority(&self) -> Authority {
        self.authority
    }

    /// A storage revision is not an enrollment nonce. Instance lineage also
    /// invalidates an old candidate after rebuilding a byte-identical inventory.
    /// Same-instance revalidation still requires the current full durable row.
    pub fn revalidate(
        &self,
        current: &OwnedDraftIndex,
        stored: Option<&Stored>,
    ) -> Result<(), IndexError> {
        if !Arc::ptr_eq(&self.inventory, &current.inventory) {
            return Err(IndexError::StaleInventory);
        }
        let expected = &self.inventory.rows[&self.key];
        if stored.is_none_or(|row| row.revision != expected.revision || row.state != expected.state)
        {
            return Err(IndexError::StaleOwner);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
