//! One account-bound encounter-order store for directory channel consumers.
//! Wire/source records and typed native comparison witnesses remain separate.
use crate::{config::WorkspaceSession, slack::models::Channel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
mod ingress;
mod lifecycle;
pub mod opened;
pub use ingress::{AcceptedIngress, IngressWitness};
mod actions;
pub mod freshness;
pub mod inline_label;
mod joined;
mod memberships;
mod names;
mod observations;
pub mod resync;
pub mod search_memberships;
mod unchanged;
pub use lifecycle::BootLease;
pub use names::{NamePlan, NameSnapshot};
pub use observations::{Observation, RecoveryState};
pub use unchanged::Unchanged;
mod plan;
pub mod projection;
pub use plan::{Facts, RendererReply};
pub use projection::Error;
#[cfg(test)]
mod ingress_receipt_tests;
#[cfg(test)]
mod tests;
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    account: String,
    team: String,
    user: String,
}
impl Identity {
    fn capture(account: &str, w: &WorkspaceSession) -> Result<Self> {
        if account.is_empty() || w.team_id.is_empty() || w.user_id.is_empty() {
            return Err(Error::Shape);
        }
        Ok(Self {
            account: account.into(),
            team: w.team_id.clone(),
            user: w.user_id.clone(),
        })
    }
}
/// Observed native acquisition order never claims to reconstruct historical
/// order lost by a legacy cache, or another client's privately persisted store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    EmptyNativeIngress,
    ReacquiredNativeHistory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Boot,
    Directory,
    Direct,
    /// Actual conversations.open result through 46wa general upsert.
    OpenedConversation,
    /// A 0mwJ terminal marker, never a received HTTP row.
    RuntimeMissing,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    source: Value,
    wire: Value,
    native: Value,
    #[serde(default)]
    runtime_missing: bool,
    #[serde(default)]
    runtime_unknown: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<actions::Action>,
    origin: Origin,
    rows: Vec<Value>,
    #[serde(default)]
    boot_in_flight: Option<bool>,
    #[serde(default)]
    receipt_facts: Option<Facts>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    identity: Identity,
    provenance: Provenance,
    records: Vec<Record>,
    pending: Vec<Batch>,
    invalidated: BTreeSet<String>,
    overflowed: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Store {
    saved: Option<Saved>,
    epoch: Arc<()>,
    lifecycle: lifecycle::Lifecycle,
    // EmXR channelNamesToIds is a lazy, nonpersistent team-local index.
    names: Option<Arc<names::Index>>,
}
pub struct Stamp {
    epoch: Arc<()>,
    account: String,
    owner: WorkspaceSession,
    account_epoch: u64,
    generation: String,
}
pub struct Pending {
    stamp: Stamp,
    batch: Batch,
    before: Vec<Value>,
    native: BTreeMap<String, Value>,
    plan: plan::Plan,
}
impl Pending {
    pub fn before(&self) -> &[Value] {
        &self.before
    }
    pub fn incoming(&self) -> &[Value] {
        &self.batch.rows
    }
    pub fn origin(&self) -> Origin {
        self.batch.origin
    }
    pub fn fields(&self) -> &[String] {
        self.plan.fields()
    }
    pub fn generation(&self) -> &str {
        self.plan.generation()
    }
}
/// The store mints both outputs; callers cannot supply an arbitrary native-after
/// witness. Install the pair in one serial mutation after final owner checks.
pub struct Commit {
    store: Store,
    native: BTreeMap<String, Channel>,
}
impl Commit {
    pub fn into_parts(self) -> (Store, BTreeMap<String, Channel>) {
        (self.store, self.native)
    }
}
pub struct Ready {
    stamp: Stamp,
    rows: Vec<Value>,
    provenance: Provenance,
}
impl Ready {
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn provenance(&self) -> Provenance {
        self.provenance
    }
    pub fn current(
        &self,
        s: &Store,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> bool {
        s.current(&self.stamp, account, w, epoch, generation) && s.native_current(native)
    }
}
fn native_snapshot(native: &BTreeMap<String, Channel>) -> Result<BTreeMap<String, Value>> {
    native
        .iter()
        .map(|(id, c)| {
            serde_json::to_value(c)
                .map(|v| (id.clone(), v))
                .map_err(|_| Error::Shape)
        })
        .collect()
}
fn source_native_equal(a: &Value, b: &Value) -> bool {
    let strip = |v: &Value| {
        let mut v = v.clone();
        if let Some(o) = v.as_object_mut() {
            // 340b deliberately removes both fields from the channel store.
            // Their separate read/latest owners must not fabricate a raw-channel
            // update or reorder an otherwise current directory observation.
            o.remove("latest");
            o.remove("last_read");
        }
        v
    };
    strip(a) == strip(b)
}
// A direct eH action may store the manager's exact transient selector object.
// This shape is distinct from a 340b-processed wire record and cannot attest a
// received response or server absence.
fn runtime_placeholder(row: &Value) -> bool {
    row.as_object().is_some_and(|o| {
        o.len() == 3 && o.contains_key("id") && row["isUnknown"] == true && row["is_member"] == true
    })
}
fn session_equal(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.user_id == b.user_id
        && a.enterprise_id == b.enterprise_id
        && a.token == b.token
        && a.url == b.url
}
impl Store {
    /// Closed disconnected fixture ingress. Production cannot manufacture a
    /// completed boot phase through this feature-gated API.
    #[cfg(feature = "protected-test-support")]
    pub fn observe_fixture_boot(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &BTreeMap<String, Channel>,
        raw: &Value,
    ) -> Result<()> {
        let lease = self.begin_full_boot(account, owner, epoch)?;
        let result = self.observe_boot(account, owner, native, epoch, raw);
        if !self.finish_full_boot(lease, account, owner, epoch) {
            return Err(Error::MissingObservation);
        }
        result
    }
    pub(crate) fn begin_full_boot(
        &mut self,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
    ) -> Result<BootLease> {
        self.lifecycle.begin(account, w, epoch)
    }
    pub(crate) fn finish_full_boot(
        &mut self,
        lease: BootLease,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
    ) -> bool {
        self.lifecycle.finish(lease, account, w, epoch)
    }
    fn identity(&self, account: &str, w: &WorkspaceSession) -> Result<&Saved> {
        let id = Identity::capture(account, w)?;
        self.saved
            .as_ref()
            .filter(|s| s.identity == id)
            .ok_or(Error::MissingObservation)
    }
    fn changed(&mut self) {
        self.epoch = Arc::new(());
    }
    pub fn begin(
        &mut self,
        account: &str,
        w: &WorkspaceSession,
        native: &BTreeMap<String, Channel>,
    ) -> Result<()> {
        let id = Identity::capture(account, w)?;
        if self
            .saved
            .as_ref()
            .is_some_and(|saved| saved.identity != id)
        {
            // A foreign cache is not a fresh empty store. Account replacement
            // owns a different Workspace; never erase this retained history.
            return Err(Error::MissingObservation);
        }
        let provenance = if native.is_empty() && self.saved.is_none() {
            Provenance::EmptyNativeIngress
        } else {
            Provenance::ReacquiredNativeHistory
        };
        self.saved = Some(Saved {
            version: 1,
            identity: id,
            provenance,
            records: vec![],
            pending: vec![],
            invalidated: native.keys().cloned().collect(),
            overflowed: false,
        });
        self.names = None;
        self.changed();
        Ok(())
    }
    pub fn enqueue(
        &mut self,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        origin: Origin,
        rows: Vec<Value>,
    ) -> Result<()> {
        // Runtime markers require a live manager receipt, not caller-built rows.
        if origin == Origin::RuntimeMissing {
            return Err(Error::MissingObservation);
        }
        self.enqueue_checked(account, w, epoch, origin, rows)
    }
    fn enqueue_checked(
        &mut self,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        origin: Origin,
        rows: Vec<Value>,
    ) -> Result<()> {
        self.identity(account, w)?;
        let boot_in_flight = Some(self.lifecycle.phase(account, w, epoch)?);
        validate_rows(origin, &rows)?;
        self.enqueue_batch(Batch {
            action: None,
            receipt_facts: None,
            origin,
            rows,
            boot_in_flight,
        })
    }
    fn enqueue_batch(&mut self, batch: Batch) -> Result<()> {
        let Batch {
            action,
            origin,
            rows,
            boot_in_flight,
            receipt_facts,
        } = batch;
        let s = self.saved.as_mut().unwrap();
        if s.overflowed {
            return Err(Error::MissingObservation);
        }
        if s.pending.len() >= 64
            || (rows.len() + action.as_ref().map_or(0, |a| a.ids().len()))
                > 100_000usize.saturating_sub(s.pending.iter().map(|b| b.units()).sum())
        {
            s.overflowed = true;
            self.changed();
            return Err(Error::MissingObservation);
        }
        // Empty successful batches are not eviction or an inventory reset.
        if !rows.is_empty() || action.as_ref().is_some_and(|a| !a.ids().is_empty()) {
            s.pending.push(Batch {
                action,
                origin,
                rows,
                boot_in_flight,
                receipt_facts,
            });
            self.changed();
        }
        Ok(())
    }
    /// Actual native userBoot acquisition order. This is explicitly distinct
    /// from web client.channels' channels/ims/mpims order. Match native
    /// BootData::all_channels' first-ID precedence without sorting the arrays.
    pub fn observe_boot(
        &mut self,
        account: &str,
        w: &WorkspaceSession,
        native: &BTreeMap<String, Channel>,
        epoch: u64,
        raw: &Value,
    ) -> Result<()> {
        if !raw.is_object() {
            return Err(Error::Shape);
        }
        let mut rows = Vec::new();
        let mut seen = BTreeSet::new();
        for key in ["channels", "groups", "ims", "mpims"] {
            if let Some(values) = raw.get(key) {
                for row in values.as_array().ok_or(Error::Shape)? {
                    if seen.insert(projection::id(row)?.to_owned()) {
                        rows.push(row.clone());
                    }
                }
            }
        }
        if self.identity(account, w).is_err() {
            self.begin(account, w, native)?;
        }
        self.enqueue(account, w, epoch, Origin::Boot, rows)
    }
    pub fn has_pending(&self) -> bool {
        self.saved.as_ref().is_some_and(|s| !s.pending.is_empty())
    }
    pub fn invalidate(&mut self, id: &str) {
        if let Some(s) = &mut self.saved {
            if s.invalidated.insert(id.into()) {
                self.changed();
            }
        }
    }
    fn stamp(
        &self,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> Result<Stamp> {
        self.identity(account, w)?;
        if generation.is_empty() {
            return Err(Error::Shape);
        }
        Ok(Stamp {
            epoch: self.epoch.clone(),
            account: account.into(),
            owner: w.clone(),
            account_epoch: epoch,
            generation: generation.into(),
        })
    }
    pub fn current(
        &self,
        stamp: &Stamp,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        generation: &str,
    ) -> bool {
        Arc::ptr_eq(&self.epoch, &stamp.epoch)
            && stamp.account == account
            && session_equal(&stamp.owner, w)
            && stamp.account_epoch == epoch
            && stamp.generation == generation
            && self.identity(account, w).is_ok()
    }
    pub fn prepare(
        &self,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
        facts: Facts,
    ) -> Result<Pending> {
        let saved = self.identity(account, w)?;
        if saved.overflowed {
            return Err(Error::MissingObservation);
        }
        let before: Vec<_> = saved.records.iter().map(|r| r.source.clone()).collect();
        let batch = saved
            .pending
            .first()
            .ok_or(Error::MissingObservation)?
            .clone();
        if batch
            .receipt_facts
            .as_ref()
            .is_some_and(|captured| captured != &facts)
        {
            return Err(Error::MissingObservation);
        }
        if batch.action.is_some() {
            return Err(Error::MissingObservation);
        }
        let boot_in_flight = batch.boot_in_flight.ok_or(Error::MissingObservation)?;
        let plan = plan::Plan::capture(facts, boot_in_flight, generation, &before, &batch.rows)?;
        Ok(Pending {
            stamp: self.stamp(account, w, epoch, generation)?,
            batch,
            before,
            native: native_snapshot(native)?,
            plan,
        })
    }
    /// Process only the captured raw batch after its actual renderer/context
    /// observations. All field/name/flag observations still need caller-owned
    /// freshness checks before this synchronous operation and paired install.
    pub fn finish(
        &self,
        pending: Pending,
        current_facts: &Facts,
        reply: RendererReply,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<Commit> {
        if !self.current(&pending.stamp, account, w, epoch, generation)
            || native_snapshot(native)? != pending.native
        {
            return Err(Error::MissingObservation);
        }
        let saved = self.identity(account, w)?;
        if saved.pending.first() != Some(&pending.batch)
            || saved
                .records
                .iter()
                .map(|r| &r.source)
                .ne(pending.before.iter())
        {
            return Err(Error::MissingObservation);
        }
        let (processed, lower_names) = pending.plan.apply_with_names(current_facts, reply)?;
        if processed.before() != pending.before || processed.incoming() != pending.batch.rows {
            return Err(Error::Shape);
        }
        let observed_ids: BTreeSet<_> = pending
            .batch
            .rows
            .iter()
            .map(|row| projection::id(row).unwrap())
            .collect();
        let mut native_after = native.clone();
        // An unchanged canonical row is still a fresh acquired observation.
        // Re-project its source-owned fields before recording the native stamp;
        // otherwise a suppressed source update could certify native drift.
        for row in processed
            .rows()
            .iter()
            .filter(|row| observed_ids.contains(projection::id(row).unwrap()))
        {
            let id = projection::id(row)?;
            // Preserve native-only/read-state fields. They never fill source
            // presence or source order, and are retained only in this projection.
            let mut raw = pending
                .native
                .get(id)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            raw.extend(row.as_object().ok_or(Error::Shape)?.clone());
            let c: Channel =
                serde_json::from_value(Value::Object(raw)).map_err(|_| Error::Shape)?;
            if c.id != id {
                return Err(Error::Shape);
            }
            native_after.insert(c.id.clone(), c);
        }
        let previous: BTreeMap<_, _> = saved
            .records
            .iter()
            .map(|r| (projection::id(&r.source).unwrap(), r))
            .collect();
        let observed: BTreeMap<_, _> = pending
            .batch
            .rows
            .iter()
            .map(|v| (projection::id(v).unwrap(), v))
            .collect();
        let mut records = Vec::new();
        for row in processed.rows() {
            let id = projection::id(row)?;
            let observed = observed.get(id).copied();
            let old = previous.get(id).copied();
            records.push(Record {
                source: row.clone(),
                runtime_unknown: row["isUnknown"] == true && old.is_some_and(|r| r.runtime_unknown),
                runtime_missing: row.get("isNonExistent").and_then(Value::as_bool) == Some(true)
                    && ((observed.is_some() && pending.batch.origin == Origin::RuntimeMissing)
                        || old.is_some_and(|r| r.runtime_missing)),
                wire: observed
                    .filter(|_| pending.batch.origin != Origin::RuntimeMissing)
                    .cloned()
                    .or_else(|| old.map(|r| r.wire.clone()))
                    .unwrap_or(Value::Null),
                native: if observed.is_some() {
                    serde_json::to_value(native_after.get(id).ok_or(Error::Shape)?)
                        .map_err(|_| Error::Shape)?
                } else {
                    old.ok_or(Error::Shape)?.native.clone()
                },
            });
        }
        let mut store = self.clone();
        if let Some(index) = &mut store.names {
            Arc::make_mut(index).update(processed.actions(), &lower_names)?;
        }
        let saved = store.saved.as_mut().unwrap();
        saved.records = records;
        saved.pending.remove(0);
        for row in &pending.batch.rows {
            saved.invalidated.remove(projection::id(row)?);
        }
        store.changed();
        Ok(Commit {
            store,
            native: native_after,
        })
    }
    pub fn remove(
        &self,
        account: &str,
        w: &WorkspaceSession,
        ids: &BTreeSet<String>,
        native: &BTreeMap<String, Channel>,
    ) -> Result<Commit> {
        let s = self.identity(account, w)?;
        if !s.pending.is_empty() || s.overflowed {
            return Err(Error::MissingObservation);
        }
        let mut store = self.clone();
        if let Some(index) = &mut store.names {
            for id in ids {
                Arc::make_mut(index).remove(id);
            }
        }
        let s = store.saved.as_mut().unwrap();
        s.records
            .retain(|r| !ids.contains(projection::id(&r.source).unwrap()));
        s.invalidated.retain(|id| !ids.contains(id));
        store.changed();
        let mut native = native.clone();
        native.retain(|id, _| !ids.contains(id));
        Ok(Commit { store, native })
    }
    pub fn required_observations(&self, native: &BTreeMap<String, Channel>) -> BTreeSet<String> {
        let mut ids = self
            .saved
            .as_ref()
            .map(|s| s.invalidated.clone())
            .unwrap_or_default();
        let rows: BTreeMap<_, _> = self
            .saved
            .iter()
            .flat_map(|s| s.records.iter())
            .filter_map(|r| projection::id(&r.source).ok().map(|id| (id, r)))
            .collect();
        for (id, c) in native {
            if rows.get(id.as_str()).is_none_or(|r| {
                serde_json::to_value(c)
                    .ok()
                    .is_none_or(|v| !source_native_equal(&v, &r.native))
            }) {
                ids.insert(id.clone());
            }
        }
        ids.extend(
            rows.keys()
                .filter(|id| !native.contains_key(**id))
                .map(|id| (*id).to_owned()),
        );
        ids
    }
    fn native_current(&self, native: &BTreeMap<String, Channel>) -> bool {
        self.saved.as_ref().is_some_and(|s| {
            s.records.len() == native.len()
                && s.records.iter().all(|r| {
                    native
                        .get(projection::id(&r.source).unwrap_or(""))
                        .and_then(|c| serde_json::to_value(c).ok())
                        .is_some_and(|v| source_native_equal(&v, &r.native))
                })
        })
    }
    pub fn ready(
        &self,
        account: &str,
        w: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &BTreeMap<String, Channel>,
    ) -> Result<Ready> {
        let s = self.identity(account, w)?;
        if s.overflowed
            || !s.pending.is_empty()
            || !s.invalidated.is_empty()
            || !self.native_current(native)
        {
            return Err(Error::MissingObservation);
        }
        Ok(Ready {
            stamp: self.stamp(account, w, epoch, generation)?,
            rows: s.records.iter().map(|r| r.source.clone()).collect(),
            provenance: s.provenance,
        })
    }
    pub fn cached(&self) -> Option<Value> {
        self.saved
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok())
    }
    pub fn restore(raw: Option<&Value>, w: &WorkspaceSession) -> Self {
        let saved = raw
            .and_then(|v| serde_json::from_value::<Saved>(v.clone()).ok())
            .filter(|s| {
                if s.version != 1
                    || s.identity.account.is_empty()
                    || s.identity.team != w.team_id
                    || s.identity.user != w.user_id
                    || s.records.len() > 100_000
                    || s.pending.len() > 64
                {
                    return false;
                }
                let mut ids = BTreeSet::new();
                s.records.iter().all(|r| {
                    projection::id(&r.source).is_ok_and(|id| {
                        ids.insert(id.to_owned())
                            && ((r.runtime_unknown && runtime_placeholder(&r.source))
                                || (r.source["_name_lc"].is_string()
                                    && [
                                        "internalTeamIds",
                                        "connectedTeamIds",
                                        "connectedLimitedTeamIds",
                                    ]
                                    .iter()
                                    .all(|key| {
                                        r.source[*key]
                                            .as_array()
                                            .is_some_and(|v| v.iter().all(Value::is_string))
                                    })
                                    && r.source["fromAnotherTeam"].is_boolean()))
                            && (projection::id(&r.wire) == Ok(id)
                                || (r.wire.is_null()
                                    && ((r.runtime_missing && r.source["isNonExistent"] == true)
                                        || (r.runtime_unknown && r.source["isUnknown"] == true))))
                            && serde_json::from_value::<Channel>(r.native.clone())
                                .is_ok_and(|c| c.id == id)
                    })
                }) && s.pending.iter().map(|b| b.units()).sum::<usize>() <= 100_000
                    && s.pending
                        .iter()
                        .all(|b| b.rows.iter().all(|r| projection::id(r).is_ok()))
                    && s.pending.iter().all(|b| b.valid_action())
                    && s.pending.iter().all(|b| {
                        b.receipt_facts
                            .as_ref()
                            .is_none_or(|facts| ingress::valid_receipt_facts(facts, w))
                    })
                    && s.invalidated
                        .iter()
                        .all(|id| projection::id(&serde_json::json!({"id":id})).is_ok())
            });
        Self {
            saved,
            epoch: Arc::new(()),
            lifecycle: Default::default(),
            names: None,
        }
    }
}

fn validate_rows(origin: Origin, rows: &[Value]) -> Result<()> {
    for row in rows {
        projection::id(row)?;
        if origin == Origin::RuntimeMissing
            && !(row.as_object().is_some_and(|row| row.len() == 2) && row["isNonExistent"] == true)
        {
            return Err(Error::Shape);
        }
    }
    Ok(())
}
