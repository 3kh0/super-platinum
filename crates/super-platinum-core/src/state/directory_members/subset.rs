//! Existing canonical members needed by another source projection.
//! This is a bounded read, never a complete collection or absence certificate.
use super::*;
use crate::slack::models::User;
use std::collections::HashMap;

pub struct Subset {
    history: Arc<()>,
    native: Vec<Value>,
    stamp: Stamp,
    ids: BTreeSet<String>,
    rows: Vec<Current>,
}
impl Subset {
    pub fn rows(&self) -> &[Current] {
        &self.rows
    }
    /// Bounded self/permission observation: unrelated intake may change the
    /// collection revision, but source lineage, original scope and every
    /// requested source/native field must remain exactly the same.
    /// Pending/invalidation of a requested row still refuses immediately.
    pub fn current_rows(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &HashMap<String, User>,
    ) -> bool {
        Arc::ptr_eq(&self.history, &store.history)
            && self.stamp.scope.current(account, owner, epoch, generation)
            && store
                .subset_rows(account, owner, &self.ids, native)
                .is_ok_and(|rows| rows == self.rows)
            && subset_native(&self.ids, native).is_ok_and(|rows| rows == self.native)
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
            && store.subset_rows(account, owner, &self.ids, native).is_ok()
    }
}
impl Store {
    /// Certify only requested existing rows, retaining canonical encounter order.
    /// Missing rows remain unavailable; a typed native row is not source evidence.
    /// Unrelated pending/stale rows do not prevent this bounded observation.
    pub fn subset(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        ids: &BTreeSet<String>,
        native: &HashMap<String, User>,
    ) -> Result<Subset, Error> {
        let rows = self.subset_rows(account, owner, ids, native)?;
        Ok(Subset {
            history: self.history.clone(),
            native: subset_native(ids, native)?,
            stamp: self.stamp(account, owner, epoch, generation)?,
            ids: ids.clone(),
            rows,
        })
    }
    fn subset_rows(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        ids: &BTreeSet<String>,
        native: &HashMap<String, User>,
    ) -> Result<Vec<Current>, Error> {
        let saved = self.identity(account, owner)?;
        for id in ids {
            member_id(&serde_json::json!({"id": id}))?;
        }
        if saved.overflowed
            || !saved.invalidated.is_disjoint(ids)
            || saved.pending.iter().any(|batch| {
                batch
                    .rows
                    .iter()
                    .any(|row| row["id"].as_str().is_some_and(|id| ids.contains(id)))
                    || batch
                        .interactions
                        .as_object()
                        .is_some_and(|map| map.keys().any(|id| ids.contains(id)))
                    || batch.disconnection.as_ref().is_some_and(|effect| {
                        effect
                            .requested
                            .iter()
                            .chain(&effect.marked)
                            .any(|id| ids.contains(id))
                    })
            })
        {
            return Err(unavailable());
        }
        let mut rows = Vec::with_capacity(ids.len());
        for record in &saved.records {
            let id = member_id(&record.source)?;
            if !ids.contains(id) {
                continue;
            }
            if native
                .get(id)
                .and_then(|user| serde_json::to_value(user).ok())
                .as_ref()
                != Some(&record.native_witness)
            {
                return Err(unavailable());
            }
            rows.push(record.current());
        }
        if rows.len() != ids.len() {
            return Err(unavailable());
        }
        Ok(rows)
    }
}
fn subset_native(
    ids: &BTreeSet<String>,
    native: &HashMap<String, User>,
) -> Result<Vec<Value>, Error> {
    ids.iter()
        .map(|id| {
            serde_json::to_value(native.get(id).ok_or_else(unavailable)?).map_err(|_| unavailable())
        })
        .collect()
}
#[cfg(test)]
mod tests;
