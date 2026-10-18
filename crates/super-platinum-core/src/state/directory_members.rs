//! Account-bound source encounter order and pending raw member observations.
//! Native typed users are a separate projection and never establish this order.
use crate::{
    block_actions::workflow_management::member_hydration::{
        Applied, Current, Flags, IncomingInteractions, Plan, RendererReply, Scope,
    },
    config::WorkspaceSession,
    slack::Error,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, fmt, sync::Arc};

pub mod fetch_claim;
mod native_repair;
mod observations;
mod receipt;
pub mod subscriptions;
pub use receipt::ReceiptMembers;
mod subset;
pub use native_repair::{NativeRepair, NativeRepairCommit};
pub use subset::Subset;
mod persistence;
pub use observations::{Observations, RecoveryState};
#[cfg(test)]
mod tests;

const MAX_PENDING_BATCHES: usize = 64;
const MAX_PENDING_ROWS: usize = 100_000;
fn unavailable() -> Error {
    Error::Api("directory_member_order_unavailable".into())
}
fn member_id(row: &Value) -> Result<&str, Error> {
    let id = row
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(unavailable)?;
    // Reuse the hydration leaf's exact supported identity boundary.
    IncomingInteractions::decode(&serde_json::json!({(id): true}))?;
    Ok(id)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    account: String,
    team: String,
    user: String,
}
impl Identity {
    fn capture(account: &str, owner: &WorkspaceSession) -> Result<Self, Error> {
        if account.is_empty() || owner.team_id.is_empty() || owner.user_id.is_empty() {
            return Err(unavailable());
        }
        Ok(Self {
            account: account.into(),
            team: owner.team_id.clone(),
            user: owner.user_id.clone(),
        })
    }
}

/// Reacquired order is explicitly a native acquisition history, not recovery
/// of the web client's lost historical insertion order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    EmptySessionIngress,
    ReacquiredNativeHistory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Boot,
    Directory,
    /// xqiG interaction-projected users/info rows, then ScqY (no An8m).
    MemberInfo,
    /// Caller observed an actual XbX7.Ay route, not a generic socket update.
    DirectUpsert,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    native: Value,
    source: Value,
    interaction: Option<bool>,
    wire: Value,
    native_witness: Value,
}
impl Record {
    fn current(&self) -> Current {
        Current {
            native: self.native.clone(),
            source: self.source.clone(),
            interaction: self.interaction,
        }
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    origin: Origin,
    rows: Vec<Value>,
    interactions: Value,
    #[serde(default)]
    disconnection: Option<Disconnection>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Disconnection {
    requested: Vec<String>,
    marked: Vec<String>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Persisted {
    version: u8,
    identity: Identity,
    provenance: Provenance,
    records: Vec<Record>,
    pending: Vec<Batch>,
    invalidated: BTreeSet<String>,
    overflowed: bool,
    /// false records an observed removal; absent keys remain unknown.
    #[serde(default)]
    disconnected: std::collections::BTreeMap<String, bool>,
}

#[derive(Clone, Default)]
pub struct Store {
    saved: Option<Persisted>,
    // Source lineage survives ordinary intake, but never begin/cache restore.
    history: Arc<()>,
    epoch: Arc<()>,
    revision: u64,
}
impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DirectoryMembers")
            .field("known", &self.saved.is_some())
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
pub struct Stamp {
    epoch: Arc<()>,
    revision: u64,
    scope: Scope,
}
pub struct Pending {
    stamp: Stamp,
    before: Vec<Current>,
    batch: Batch,
    plan: Plan,
    native_before: std::collections::BTreeMap<String, Value>,
}
impl Pending {
    pub fn fields(&self) -> &[String] {
        self.plan.fields()
    }
    pub fn generation(&self) -> &str {
        self.plan.generation()
    }
    pub fn before(&self) -> &[Current] {
        &self.before
    }
    pub fn origin(&self) -> Origin {
        self.batch.origin
    }
}
/// Fully checked native/source pair for synchronous serial installation.
pub struct Commit {
    applied: Applied,
    native: std::collections::HashMap<String, crate::slack::models::User>,
}
impl Commit {
    pub fn into_parts(
        self,
    ) -> (
        Applied,
        std::collections::HashMap<String, crate::slack::models::User>,
    ) {
        (self.applied, self.native)
    }
}
pub struct Ready {
    stamp: Stamp,
    rows: Vec<Current>,
    provenance: Provenance,
}
impl Ready {
    pub fn rows(&self) -> &[Current] {
        &self.rows
    }
    pub fn provenance(&self) -> Provenance {
        self.provenance
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &std::collections::HashMap<String, crate::slack::models::User>,
    ) -> bool {
        store.current(&self.stamp, account, owner, epoch, generation)
            && store.native_current(native)
    }
}

impl Store {
    fn changed(&mut self) {
        self.epoch = Arc::new(());
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
        } else {
            self.epoch = Arc::new(());
            self.revision = 0;
        }
    }
    fn identity(&self, account: &str, owner: &WorkspaceSession) -> Result<&Persisted, Error> {
        let identity = Identity::capture(account, owner)?;
        self.saved
            .as_ref()
            .filter(|s| s.identity == identity)
            .ok_or_else(unavailable)
    }
    fn stamp(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> Result<Stamp, Error> {
        self.identity(account, owner)?;
        Ok(Stamp {
            epoch: self.epoch.clone(),
            revision: self.revision,
            scope: Scope::capture(account, owner, epoch, generation)?,
        })
    }
    pub fn current(
        &self,
        stamp: &Stamp,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> bool {
        Arc::ptr_eq(&self.epoch, &stamp.epoch)
            && self.revision == stamp.revision
            && stamp.scope.current(account, owner, epoch, generation)
            && self.identity(account, owner).is_ok()
    }
    /// Explicit new source-history boundary. Only an actually empty native
    /// member collection can establish EmptySessionIngress. A legacy cache or
    /// overflow recovery is labelled ReacquiredNativeHistory.
    pub fn begin(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        native_ids: &BTreeSet<String>,
    ) -> Result<(), Error> {
        let identity = Identity::capture(account, owner)?;
        for id in native_ids {
            member_id(&serde_json::json!({"id":id}))?;
        }
        let provenance = if native_ids.is_empty() && self.saved.is_none() {
            Provenance::EmptySessionIngress
        } else {
            Provenance::ReacquiredNativeHistory
        };
        self.saved = Some(Persisted {
            version: 1,
            identity,
            provenance,
            records: vec![],
            pending: vec![],
            invalidated: native_ids.clone(),
            overflowed: false,
            disconnected: std::collections::BTreeMap::new(),
        });
        self.history = Arc::new(());
        self.epoch = Arc::new(());
        self.revision = 0;
        Ok(())
    }
    /// Native client.userBoot raw users are observed before typed decoding.
    /// This starts/extends native encounter history, never a claim that this
    /// response is Slack's entire member store. Missing users leave it unknown.
    pub fn observe_boot(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        native_ids: &BTreeSet<String>,
        raw: &Value,
    ) -> Result<(), Error> {
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(unavailable());
        }
        let rows = raw
            .get("users")
            .and_then(Value::as_array)
            .ok_or_else(unavailable)?;
        for row in rows {
            member_id(row)?;
        }
        if self.identity(account, owner).is_err() {
            self.begin(account, owner, native_ids)?;
        }
        self.enqueue(
            account,
            owner,
            Origin::Boot,
            rows.clone(),
            serde_json::json!({}),
        )
    }
    /// Called only after the owner accepts the actual raw response/event. The
    /// supplied order must be the wire array/Object.keys order, never map values.
    /// Omitted IDs do not remove old members; deleted:true is an ordinary row.
    pub fn enqueue(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        origin: Origin,
        rows: Vec<Value>,
        interactions: Value,
    ) -> Result<(), Error> {
        self.enqueue_batch(account, owner, origin, rows, interactions, None)
    }
    /// Actual nzRy requested-ID resets and returned disconnected marks travel
    /// with the xqiG/ScqY member batch and settle in the same checked Commit.
    pub fn enqueue_member_info(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        rows: Vec<Value>,
        interactions: Value,
        requested: Vec<String>,
        marked: Vec<String>,
    ) -> Result<(), Error> {
        self.enqueue_batch(
            account,
            owner,
            Origin::MemberInfo,
            rows,
            interactions,
            Some(Disconnection { requested, marked }),
        )
    }
    fn enqueue_batch(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        origin: Origin,
        rows: Vec<Value>,
        interactions: Value,
        disconnection: Option<Disconnection>,
    ) -> Result<(), Error> {
        self.identity(account, owner)?;
        validate_disconnection(origin, &rows, &interactions, disconnection.as_ref())?;
        IncomingInteractions::decode(&interactions)?;
        if !matches!(origin, Origin::Directory | Origin::MemberInfo)
            && interactions.as_object().is_none_or(|m| !m.is_empty())
        {
            return Err(unavailable());
        }
        for row in &rows {
            member_id(row)?;
        }
        let saved = self.saved.as_mut().ok_or_else(unavailable)?;
        if saved.overflowed {
            return Err(unavailable());
        }
        let count: usize = saved.pending.iter().map(batch_size).sum();
        if saved.pending.len() >= MAX_PENDING_BATCHES
            || rows.len().saturating_add(
                disconnection
                    .as_ref()
                    .map_or(0, |d| d.requested.len().saturating_add(d.marked.len())),
            ) > MAX_PENDING_ROWS.saturating_sub(count)
        {
            // Retain all accepted state and order, but explicitly require
            // reacquisition after the observation that could not be retained.
            saved.overflowed = true;
            self.changed();
            return Err(unavailable());
        }
        saved.pending.push(Batch {
            origin,
            rows,
            interactions,
            disconnection,
        });
        self.changed();
        Ok(())
    }
    /// A typed-only update carries no raw presence or casing evidence. Preserve
    /// the member's position and invalidate just that row until raw reacquisition.
    pub fn invalidate(&mut self, id: &str) {
        if let Some(saved) = &mut self.saved {
            if saved.invalidated.insert(id.into()) {
                self.changed();
            }
        }
    }
    pub fn invalidated(&self) -> BTreeSet<String> {
        self.saved
            .as_ref()
            .map(|s| s.invalidated.clone())
            .unwrap_or_default()
    }
    /// Acquisition IDs only, never an ordered source inventory. This also finds
    /// typed legacy writers that did not explicitly call invalidate().
    pub fn required_observations(
        &self,
        native: &std::collections::HashMap<String, crate::slack::models::User>,
    ) -> BTreeSet<String> {
        let mut ids = self.invalidated();
        let records: std::collections::HashMap<_, _> = self
            .saved
            .iter()
            .flat_map(|saved| saved.records.iter())
            .filter_map(|row| row.source["id"].as_str().map(|id| (id, row)))
            .collect();
        for (id, user) in native {
            if records
                .get(id.as_str())
                .is_none_or(|r| serde_json::to_value(user).ok().as_ref() != Some(&r.native_witness))
            {
                ids.insert(id.clone());
            }
        }
        if let Some(saved) = &self.saved {
            ids.extend(saved.records.iter().filter_map(|r| {
                let id = r.source["id"].as_str()?;
                (!native.contains_key(id)).then(|| id.to_owned())
            }));
        }
        ids
    }
    /// Physical eviction/removal is separate from deleted:true. Re-insertion
    /// then appends a new slot, like JS delete followed by property insertion.
    /// Pending batches must first settle so removal cannot reorder an older write.
    pub fn remove(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        ids: &BTreeSet<String>,
    ) -> Result<(), Error> {
        let saved = self.identity(account, owner)?;
        if !saved.pending.is_empty() || saved.overflowed {
            return Err(unavailable());
        }
        let saved = self.saved.as_mut().ok_or_else(unavailable)?;
        saved
            .records
            .retain(|r| !ids.contains(r.source["id"].as_str().unwrap_or_default()));
        saved.invalidated.retain(|id| !ids.contains(id));
        self.changed();
        Ok(())
    }
    /// Queue state only: false does not establish ready source coverage.
    /// Unknown identity or lost observations remain errors, not an empty queue.
    /// prepare/adopt still capture and recheck the full runtime scope.
    pub fn has_pending(&self, account: &str, owner: &WorkspaceSession) -> Result<bool, Error> {
        let saved = self.identity(account, owner)?;
        if saved.overflowed {
            return Err(unavailable());
        }
        Ok(!saved.pending.is_empty())
    }
    pub fn prepare(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        flags: Flags,
        native: &std::collections::HashMap<String, crate::slack::models::User>,
    ) -> Result<Pending, Error> {
        let saved = self.identity(account, owner)?;
        if saved.overflowed {
            return Err(unavailable());
        }
        let batch = saved.pending.first().ok_or_else(unavailable)?.clone();
        let before: Vec<_> = saved.records.iter().map(Record::current).collect();
        let scope = Scope::capture(account, owner, epoch, generation)?;
        let plan = if batch.origin == Origin::Directory {
            Plan::capture(
                scope,
                before.clone(),
                batch.rows.clone(),
                IncomingInteractions::decode(&batch.interactions)?,
                flags,
            )?
        } else if batch.origin == Origin::MemberInfo {
            Plan::capture_member_info(
                scope,
                before.clone(),
                batch.rows.clone(),
                IncomingInteractions::decode(&batch.interactions)?,
                flags,
            )?
        } else {
            Plan::capture_direct(scope, before.clone(), batch.rows.clone(), flags)?
        };
        Ok(Pending {
            stamp: self.stamp(account, owner, epoch, generation)?,
            before,
            batch,
            plan,
            native_before: native_snapshot(native)?,
        })
    }
    pub fn adopt(
        &mut self,
        pending: Pending,
        reply: RendererReply,
        flags: &Flags,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native_current: &std::collections::HashMap<String, crate::slack::models::User>,
    ) -> Result<Commit, Error> {
        if !self.current(&pending.stamp, account, owner, epoch, generation) {
            return Err(unavailable());
        }
        if native_snapshot(native_current)? != pending.native_before {
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
        let mut native_after = native_current.clone();
        for raw in applied.native_updates(&pending.native_before)? {
            let user: crate::slack::models::User =
                serde_json::from_value(raw).map_err(|_| unavailable())?;
            native_after.insert(user.id.clone(), user);
        }
        let saved = self.identity(account, owner)?;
        if saved.pending.first() != Some(&pending.batch)
            || saved
                .records
                .iter()
                .map(Record::current)
                .collect::<Vec<_>>()
                != pending.before
            || applied
                .observations()
                .iter()
                .map(|o| &o.wire)
                .ne(pending.batch.rows.iter())
        {
            return Err(unavailable());
        }
        // Indices are lookups only; the Applied vector remains the source order.
        let before_by_id: std::collections::HashMap<_, _> = pending
            .before
            .iter()
            .map(|row| member_id(&row.source).map(|id| (id, row)))
            .collect::<Result<_, _>>()?;
        let saved_by_id: std::collections::HashMap<_, _> = saved
            .records
            .iter()
            .map(|row| member_id(&row.source).map(|id| (id, row)))
            .collect::<Result<_, _>>()?;
        let mut last_unskipped = std::collections::HashMap::new();
        // Applied is minted only by the checked hydration leaf. Confirm its
        // original-state witnesses rather than accepting a different plan.
        for observation in applied.observations() {
            let id = member_id(&observation.wire)?;
            if observation.before.as_ref() != before_by_id.get(id).copied() {
                return Err(unavailable());
            }
            if !observation.skipped_same_version {
                last_unskipped.insert(id, &observation.wire);
            }
        }
        let mut records = Vec::new();
        for row in applied.rows() {
            let id = member_id(&row.source)?;
            let changed_wire = last_unskipped.get(id).copied();
            let previous = saved_by_id.get(id).copied();
            let wire = changed_wire
                .or_else(|| previous.map(|record| &record.wire))
                .ok_or_else(unavailable)?
                .clone();
            records.push(Record {
                native: row.native.clone(),
                source: row.source.clone(),
                interaction: row.interaction,
                wire,
                native_witness: if changed_wire.is_some() {
                    serde_json::to_value(native_after.get(id).ok_or_else(unavailable)?)
                        .map_err(|_| unavailable())?
                } else {
                    previous.ok_or_else(unavailable)?.native_witness.clone()
                },
            });
        }
        let saved = self.saved.as_mut().ok_or_else(unavailable)?;
        saved.records = records;
        if let Some(effect) = &pending.batch.disconnection {
            for id in &effect.requested {
                saved.disconnected.insert(id.clone(), false);
            }
            for id in &effect.marked {
                saved.disconnected.insert(id.clone(), true);
            }
        }
        saved.pending.remove(0);
        for row in &pending.batch.rows {
            saved.invalidated.remove(member_id(row)?);
        }
        self.changed();
        Ok(Commit {
            applied,
            native: native_after,
        })
    }
    fn native_current(
        &self,
        native: &std::collections::HashMap<String, crate::slack::models::User>,
    ) -> bool {
        self.saved.as_ref().is_some_and(|saved| {
            saved.records.len() == native.len()
                && saved.records.iter().all(|row| {
                    native
                        .get(row.source["id"].as_str().unwrap_or_default())
                        .and_then(|user| serde_json::to_value(user).ok())
                        .as_ref()
                        == Some(&row.native_witness)
                })
        })
    }
    /// Every native member must have a source observation, including members
    /// absent from the latest response. Coverage alone never establishes order.
    pub fn ready(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &std::collections::HashMap<String, crate::slack::models::User>,
    ) -> Result<Ready, Error> {
        let saved = self.identity(account, owner)?;
        let ids: BTreeSet<_> = saved
            .records
            .iter()
            .map(|r| r.source["id"].as_str().unwrap_or_default().to_owned())
            .collect();
        if saved.overflowed
            || !saved.pending.is_empty()
            || !saved.invalidated.is_empty()
            || ids != native.keys().cloned().collect()
            || !self.native_current(native)
        {
            return Err(unavailable());
        }
        Ok(Ready {
            stamp: self.stamp(account, owner, epoch, generation)?,
            rows: saved.records.iter().map(Record::current).collect(),
            provenance: saved.provenance,
        })
    }
}

fn native_snapshot(
    native: &std::collections::HashMap<String, crate::slack::models::User>,
) -> Result<std::collections::BTreeMap<String, Value>, Error> {
    native
        .iter()
        .map(|(id, user)| {
            serde_json::to_value(user)
                .map(|value| (id.clone(), value))
                .map_err(|_| unavailable())
        })
        .collect()
}

fn batch_size(batch: &Batch) -> usize {
    batch.rows.len().saturating_add(
        batch
            .disconnection
            .as_ref()
            .map_or(0, |d| d.requested.len().saturating_add(d.marked.len())),
    )
}
fn validate_disconnection(
    origin: Origin,
    rows: &[Value],
    interactions: &Value,
    effect: Option<&Disconnection>,
) -> Result<(), Error> {
    if origin != Origin::MemberInfo {
        return if effect.is_none() {
            Ok(())
        } else {
            Err(unavailable())
        };
    }
    let effect = effect.ok_or_else(unavailable)?;
    let requested: BTreeSet<_> = effect.requested.iter().map(String::as_str).collect();
    if requested.is_empty()
        || requested.len() != effect.requested.len()
        || effect.requested.len() > MAX_PENDING_ROWS
        || effect.marked.len() > MAX_PENDING_ROWS
        || effect
            .requested
            .iter()
            .any(|id| member_id(&serde_json::json!({"id":id})).is_err())
        || effect
            .marked
            .iter()
            .any(|id| !requested.contains(id.as_str()))
        || rows
            .iter()
            .any(|row| !member_id(row).is_ok_and(|id| requested.contains(id)))
        || interactions
            .as_object()
            .is_none_or(|map| map.keys().any(|id| !requested.contains(id.as_str())))
    {
        return Err(unavailable());
    }
    Ok(())
}
