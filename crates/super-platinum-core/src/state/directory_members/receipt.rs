//! Synchronous member facts at a channel response, before optional unknown IO.
//! This is a bounded source witness, not a whole member inventory or Ready.
use super::*;
use crate::block_actions::directory_endpoint::member_info::unknown::{Capture, PlaceholderAsset};
use crate::slack::models::User;
use std::collections::{BTreeMap, HashMap};

pub struct ReceiptMembers {
    stamp: Stamp,
    known: Subset,
    absent: BTreeSet<String>,
    rows: BTreeMap<String, Value>,
}
impl ReceiptMembers {
    /// Exact canonical sources plus transient PdIx/0mwJ unknown projections.
    /// These rows must never be enqueued as fetched members.
    pub fn rows(&self) -> &BTreeMap<String, Value> {
        &self.rows
    }
    /// Only these IDs may schedule unknown recovery after receipt settlement.
    pub fn absent_ids(&self) -> &BTreeSet<String> {
        &self.absent
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &HashMap<String, User>,
    ) -> bool {
        store.current(&self.stamp, account, owner, epoch, generation)
            && self
                .known
                .current(store, account, owner, epoch, generation, native)
            && store
                .receipt_absence(account, owner, &self.absent, native)
                .is_ok()
    }
}
impl Store {
    pub fn receipt_members(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        requested: &BTreeSet<String>,
        native: &HashMap<String, User>,
        asset: &PlaceholderAsset,
    ) -> Result<ReceiptMembers, Error> {
        let saved = self.identity(account, owner)?;
        if saved.overflowed {
            return Err(unavailable());
        }
        for id in requested {
            member_id(&serde_json::json!({"id":id}))?;
        }
        let existing = saved
            .records
            .iter()
            .map(|r| member_id(&r.source))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let known_ids = requested
            .iter()
            .filter(|id| existing.contains(id.as_str()))
            .cloned()
            .collect::<BTreeSet<_>>();
        let absent = requested
            .difference(&known_ids)
            .cloned()
            .collect::<BTreeSet<_>>();
        self.receipt_absence(account, owner, &absent, native)?;
        let known = self.subset(account, owner, epoch, generation, &known_ids, native)?;
        let mut rows = known
            .rows()
            .iter()
            .map(|row| Ok((member_id(&row.source)?.to_owned(), row.source.clone())))
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        if !absent.is_empty() {
            // This existing producer establishes source absence under the exact
            // initialized Store stamp. The extra guards above exclude legacy
            // native-only or explicitly invalidated identities.
            let observed = self.observations(account, owner, epoch, generation)?;
            let requested = absent.iter().cloned().collect::<Vec<_>>();
            let captured = Capture::network_only(observed, &requested)?;
            if captured.ids() != requested {
                return Err(unavailable());
            }
            for row in captured.unknown_rows(asset) {
                rows.insert(member_id(&row)?.to_owned(), row);
            }
        }
        Ok(ReceiptMembers {
            stamp: self.stamp(account, owner, epoch, generation)?,
            known,
            absent,
            rows,
        })
    }
    fn receipt_absence(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        ids: &BTreeSet<String>,
        native: &HashMap<String, User>,
    ) -> Result<(), Error> {
        let saved = self.identity(account, owner)?;
        if saved.overflowed
            || !saved.invalidated.is_disjoint(ids)
            || ids.iter().any(|id| native.contains_key(id))
            || saved
                .records
                .iter()
                .any(|row| row.source["id"].as_str().is_some_and(|id| ids.contains(id)))
            || (!ids.is_empty() && !saved.pending.is_empty())
        {
            return Err(unavailable());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
