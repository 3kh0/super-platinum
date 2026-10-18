//! Native-only reconciliation after a fresh read confirms retained source rows.
//! ScqY same-version skipping, source order and raw records remain unchanged.
use super::*;
use crate::slack::models::User;
use std::collections::{BTreeMap, HashMap};

#[cfg(test)]
mod tests;

pub struct NativeRepair {
    stamp: Stamp,
    before: Vec<Current>,
    plan: Plan,
    native_before: BTreeMap<String, Value>,
    witnesses: BTreeMap<String, Value>,
    ids: BTreeSet<String>,
}
impl NativeRepair {
    pub fn fields(&self) -> &[String] {
        self.plan.fields()
    }
    pub fn generation(&self) -> &str {
        self.plan.generation()
    }
}
/// Install with the staged Store synchronously under the parent owner fence.
/// No externally supplied native projection can create this receipt.
pub struct NativeRepairCommit {
    native: HashMap<String, User>,
}
impl NativeRepairCommit {
    pub fn into_native(self) -> HashMap<String, User> {
        self.native
    }
}

impl Store {
    /// `fresh` is the actual freshly acquired, interaction-projected known-row
    /// info response. Parent binds its request/native-before/source stamp before
    /// invoking this method; it must not replay an old wire row as a fresh read.
    pub fn prepare_native_repair(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        flags: Flags,
        fresh: Vec<Value>,
        native: &HashMap<String, User>,
    ) -> Result<NativeRepair, Error> {
        let saved = self.identity(account, owner)?;
        if saved.overflowed
            || !saved.pending.is_empty()
            || fresh.is_empty()
            || fresh.len() > MAX_PENDING_ROWS
        {
            return Err(unavailable());
        }
        let witnesses: BTreeMap<_, _> = saved
            .records
            .iter()
            .map(|record| {
                Ok((
                    member_id(&record.source)?.to_owned(),
                    record.native_witness.clone(),
                ))
            })
            .collect::<Result<_, Error>>()?;
        let mut ids = BTreeSet::new();
        for row in &fresh {
            let id = member_id(row)?;
            if !witnesses.contains_key(id)
                || !ids.insert(id.to_owned())
                || !row.get("is_stranger").is_some_and(Value::is_boolean)
            {
                return Err(unavailable());
            }
        }
        let before: Vec<_> = saved.records.iter().map(Record::current).collect();
        // Reuse the pure XbX7 projection math only to validate a native repair.
        // Nothing from this trial replaces the authoritative source collection.
        let plan = Plan::capture_direct(
            Scope::capture(account, owner, epoch, generation)?,
            before.clone(),
            fresh,
            flags,
        )?;
        Ok(NativeRepair {
            stamp: self.stamp(account, owner, epoch, generation)?,
            before,
            plan,
            native_before: native_snapshot(native)?,
            witnesses,
            ids,
        })
    }
    pub fn adopt_native_repair(
        &mut self,
        pending: NativeRepair,
        reply: RendererReply,
        flags: &Flags,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &HashMap<String, User>,
    ) -> Result<NativeRepairCommit, Error> {
        if !self.current(&pending.stamp, account, owner, epoch, generation)
            || native_snapshot(native)? != pending.native_before
        {
            return Err(unavailable());
        }
        let saved = self.identity(account, owner)?;
        if saved.overflowed
            || !saved.pending.is_empty()
            || saved
                .records
                .iter()
                .map(Record::current)
                .collect::<Vec<_>>()
                != pending.before
        {
            return Err(unavailable());
        }
        let applied = pending.plan.finish(
            account,
            owner,
            epoch,
            generation,
            &pending.before,
            flags,
            reply,
        )?;
        if applied.rows().len() != pending.before.len()
            || applied
                .rows()
                .iter()
                .zip(&pending.before)
                .any(|(after, before)| {
                    after.source != before.source || after.interaction != before.interaction
                })
        {
            return Err(unavailable());
        }
        let decode = |values: Vec<Value>| -> Result<BTreeMap<String, User>, Error> {
            let mut decoded = BTreeMap::new();
            for value in values {
                let user: User = serde_json::from_value(value).map_err(|_| unavailable())?;
                if !pending.ids.contains(&user.id)
                    || decoded.insert(user.id.clone(), user).is_some()
                {
                    return Err(unavailable());
                }
            }
            if decoded.keys().cloned().collect::<BTreeSet<_>>() != pending.ids {
                return Err(unavailable());
            }
            Ok(decoded)
        };
        let expected = decode(applied.native_updates(&pending.witnesses)?)?;
        let actual = decode(applied.native_updates(&pending.native_before)?)?;
        let expected_values = expected
            .iter()
            .map(|(id, user)| {
                Ok((
                    id.clone(),
                    serde_json::to_value(user).map_err(|_| unavailable())?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let actual_values = actual
            .iter()
            .map(|(id, user)| {
                Ok((
                    id.clone(),
                    serde_json::to_value(user).map_err(|_| unavailable())?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        // A field absent from the fresh wire cannot silently bless native drift.
        // Both prior-certified and actual native projections must converge.
        if actual_values != expected_values {
            return Err(unavailable());
        }
        let mut staged = native.clone();
        for (id, user) in actual {
            staged.insert(id, user);
        }
        // All fallible checks precede the mutation. Source/raw/order/interaction
        // and disconnected metadata are deliberately untouched.
        let saved = self.saved.as_mut().ok_or_else(unavailable)?;
        for record in &mut saved.records {
            if let Some(value) =
                expected_values.get(record.source["id"].as_str().unwrap_or_default())
            {
                record.native_witness = value.clone();
            }
        }
        saved.invalidated.retain(|id| !pending.ids.contains(id));
        self.changed();
        Ok(NativeRepairCommit { native: staged })
    }
}
