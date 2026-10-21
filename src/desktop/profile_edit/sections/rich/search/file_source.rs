//! Nonpersistent, incomplete credential-owned file collection. Scope survives
//! queries and sockets; acquisition receipts do not borrow a successor socket.
use super::{Search, channel_host::Host, member_names};
use crate::{directory_member_fetches::Context, state::ShellState};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use super_platinum_core::slack::{
    Error,
    api::custom::texty::{file_title_source as source, files},
};
mod observation;
mod singleton;
#[cfg(test)]
mod tests;
pub(crate) use observation::FileObservation;
pub(crate) use singleton::FileInfoRead;

#[cfg(test)]
pub(super) fn fixture_target_source(
    shell: &mut ShellState,
    team: &str,
    rows: Vec<Value>,
) -> Result<Arc<Snapshot>, Error> {
    let owner = Arc::new(Context::capture(shell, team)?);
    let fence = Fence::capture(shell, owner, team.into(), 100)?;
    fence.publish(shell, &serde_json::json!({"ok":true,"files":rows}), &rows)
}

fn unavailable() -> Error {
    Error::Api("profile_file_search_source_unavailable".into())
}

fn truth(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        _ => true,
    }
}

#[derive(Clone, Default)]
pub(in crate::profile_edit::sections) struct State {
    identity: Arc<()>,
    owner: Option<Arc<Context>>,
    slots: BTreeMap<String, Slot>,
    queries: BTreeMap<String, files::QueryCache>,
}
#[derive(Clone, Default)]
struct Slot {
    identity: Arc<()>,
    store: source::Store,
    stamps: BTreeMap<String, Stamp>,
    invalidated: BTreeSet<String>,
    revision: u64,
}
#[derive(Clone)]
struct Stamp {
    owner: Arc<Context>,
    events: u64,
}
impl Stamp {
    fn current(&self, shell: &ShellState, id: &str) -> bool {
        self.owner.current(shell)
            && !crate::file_events::changed_since(
                shell,
                &self.owner.session.team_id,
                id,
                self.events,
            )
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| !w.deleted_files.contains(id))
    }
}
impl State {
    fn ensure(shell: &mut ShellState, owner: &Arc<Context>, namespace: &str, now_ms: i64) {
        Self::ensure_source(shell, owner);
        shell
            .profile_edit
            .sections
            .file_search
            .queries
            .entry(namespace.into())
            .or_insert_with(|| files::QueryCache::new(now_ms));
    }
    fn ensure_source(shell: &mut ShellState, owner: &Arc<Context>) {
        let keep = shell
            .profile_edit
            .sections
            .file_search
            .owner
            .as_ref()
            .is_some_and(|old| old.account_current(shell));
        let state = &mut shell.profile_edit.sections.file_search;
        if !keep {
            *state = Self::default();
            state.owner = Some(owner.clone());
        }
        state
            .slots
            .entry(owner.session.team_id.clone())
            .or_default();
    }
}

/// Local writes revoke receipts, retaining exact merge predecessors. The caller
/// invokes this at the existing accepted file mutation boundary, before async IO.
pub(crate) fn invalidate(shell: &mut ShellState, team: &str, id: &str) {
    let state = &mut shell.profile_edit.sections.file_search;
    if let Some(slot) = state.slots.get_mut(team) {
        slot.revision = slot.revision.wrapping_add(1);
        if slot.store.source(id).is_some() {
            slot.invalidated.insert(id.into());
        }
    }
}

pub(in crate::profile_edit::sections::rich) struct Snapshot {
    owner: Arc<Context>,
    state: Arc<()>,
    slot: Arc<()>,
    revision: u64,
    collection: source::Collection,
    store: source::Store,
    rows: Vec<source::CandidateObservation>,
    unavailable: Vec<String>,
    excluded: Vec<String>,
    stamps: BTreeMap<String, Stamp>,
    publication: Option<Arc<()>>,
}
impl Snapshot {
    pub(super) fn capture(shell: &ShellState, team: &str) -> Result<Arc<Self>, Error> {
        Self::under(shell, Arc::new(Context::capture(shell, team)?), None)
    }
    fn under(
        shell: &ShellState,
        owner: Arc<Context>,
        publication: Option<Arc<()>>,
    ) -> Result<Arc<Self>, Error> {
        let state = &shell.profile_edit.sections.file_search;
        if !owner.current(shell)
            || !state
                .owner
                .as_ref()
                .is_some_and(|o| o.account_current(shell))
        {
            return Err(unavailable());
        }
        let slot = state
            .slots
            .get(&owner.session.team_id)
            .ok_or_else(unavailable)?;
        let collection = slot.store.candidates();
        let mut unavailable = collection.unavailable_ids().to_vec();
        let mut rows = Vec::new();
        let mut excluded = Vec::new();
        let mut stamps = BTreeMap::new();
        for row in collection.rows() {
            match slot.stamps.get(row.id()) {
                Some(stamp)
                    if !slot.invalidated.contains(row.id()) && stamp.current(shell, row.id()) =>
                {
                    rows.push(row.clone());
                    stamps.insert(row.id().into(), stamp.clone());
                }
                _ => unavailable.push(row.id().into()),
            }
        }
        for row in collection.excluded() {
            match slot.stamps.get(row.id()) {
                Some(stamp)
                    if !slot.invalidated.contains(row.id()) && stamp.current(shell, row.id()) =>
                {
                    excluded.push(row.id().to_owned());
                    stamps.insert(row.id().to_owned(), stamp.clone());
                }
                _ => unavailable.push(row.id().to_owned()),
            }
        }
        Ok(Arc::new(Self {
            owner,
            state: state.identity.clone(),
            slot: slot.identity.clone(),
            revision: slot.revision,
            collection,
            store: slot.store.clone(),
            rows,
            unavailable,
            excluded,
            stamps,
            publication,
        }))
    }
    pub(in crate::profile_edit::sections::rich) fn rows(&self) -> &[source::CandidateObservation] {
        &self.rows
    }
    pub(in crate::profile_edit::sections::rich) fn unavailable_ids(&self) -> &[String] {
        &self.unavailable
    }
    /// Proven effective-category exclusions only. Currentness includes their
    /// original socket/event receipts just like selectable candidate rows.
    pub(in crate::profile_edit::sections::rich) fn excluded_ids(&self) -> &[String] {
        &self.excluded
    }
    /// Raw evidence for unavailable rows is not category, title or rank authority.
    pub(in crate::profile_edit::sections::rich) fn source(&self, id: &str) -> Option<&Value> {
        self.store.source(id)
    }
    pub(in crate::profile_edit::sections::rich) fn title(
        &self,
        id: &str,
    ) -> Result<source::Observation, Error> {
        if !self.rows.iter().any(|row| row.id() == id) {
            return Err(unavailable());
        }
        self.store.row(id)
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        let state = &shell.profile_edit.sections.file_search;
        self.owner.current(shell)
            && Arc::ptr_eq(&self.state, &state.identity)
            && state
                .owner
                .as_ref()
                .is_some_and(|o| o.account_current(shell))
            && state
                .slots
                .get(&self.owner.session.team_id)
                .is_some_and(|slot| {
                    Arc::ptr_eq(&self.slot, &slot.identity)
                        && self.revision == slot.revision
                        && self.collection.current(&slot.store)
                        && self
                            .stamps
                            .iter()
                            .all(|(id, stamp)| stamp.current(shell, id))
                })
    }
}

/// Closed offline fixture observation of the same current source used to rank
/// choices. This grants no acquisition, mutation, or live inspection authority.
pub(crate) fn fixture_completion_proof(shell: &ShellState) -> Result<Value, Error> {
    let cold = crate::slack_lists::comments::fixture::cold_files::enabled(shell);
    if !crate::profile_edit::sections::fixture::completion::files::enabled(shell)
        && !crate::slack_lists::comments::fixture::completion::files::enabled(shell)
        && !cold
    {
        return Err(unavailable());
    }
    let team = shell.core.active_team.as_deref().ok_or_else(unavailable)?;
    let snapshot = Snapshot::capture(shell, team)?;
    let rows: Vec<_> = snapshot
        .rows()
        .iter()
        .filter(|row| {
            matches!(
                row.id(),
                "FCOMPLETE1" | "FCOMPLETE2" | "FLISTFILE1" | "FLISTFILE2"
            )
        })
        .map(|row| {
            let mut proof = serde_json::json!({
                "id": row.id(), "source": row.source(), "effective": row.effective()
            });
            if cold {
                proof["receipt"] = row.receipt().clone();
            }
            proof
        })
        .collect();
    Ok(serde_json::json!({
        "rows": rows, "unavailableIds": snapshot.unavailable_ids(),
        "excludedIds": snapshot.excluded_ids()
    }))
}

/// Captured before the first request. Publication cannot substitute a newer
/// query, source collection, account, cookie, transport, socket, or file event.
pub(super) struct Read {
    fence: Fence,
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    context: Arc<crate::scheduled::Context>,
    names: Arc<member_names::Observation>,
}
struct Fence {
    owner: Arc<Context>,
    state: Arc<()>,
    slot: Arc<()>,
    collection: source::Collection,
    revision: u64,
    events: u64,
    namespace: String,
    publication: Arc<()>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CacheStatus {
    pub(super) exact: bool,
    pub(super) prefix: bool,
    pub(super) in_flight: bool,
}
impl Fence {
    fn capture(
        shell: &mut ShellState,
        owner: Arc<Context>,
        namespace: String,
        now_ms: i64,
    ) -> Result<Self, Error> {
        if !owner.current(shell) || namespace.is_empty() {
            return Err(unavailable());
        }
        State::ensure(shell, &owner, &namespace, now_ms);
        let state = &shell.profile_edit.sections.file_search;
        let slot = state
            .slots
            .get(&owner.session.team_id)
            .ok_or_else(unavailable)?;
        Ok(Self {
            owner,
            state: state.identity.clone(),
            slot: slot.identity.clone(),
            collection: slot.store.candidates(),
            revision: slot.revision,
            events: crate::file_events::snapshot_revision(shell),
            namespace,
            publication: Arc::new(()),
        })
    }
    fn account_current(&self, shell: &ShellState) -> bool {
        self.owner.account_current(shell)
            && Arc::ptr_eq(
                &self.state,
                &shell.profile_edit.sections.file_search.identity,
            )
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.account_current(shell)
            && self.owner.current(shell)
            && shell
                .profile_edit
                .sections
                .file_search
                .slots
                .get(&self.owner.session.team_id)
                .is_some_and(|slot| {
                    Arc::ptr_eq(&slot.identity, &self.slot)
                        && slot.revision == self.revision
                        && self.collection.current(&slot.store)
                })
    }
    fn response_current(
        &self,
        shell: &ShellState,
        carrier: &Value,
        rows: &[Value],
    ) -> Result<(), Error> {
        if !self.current(shell)
            || carrier["ok"] != true
            || carrier.get("error").is_some_and(|value| !value.is_null())
            || carrier["files"].as_array().map(Vec::as_slice) != Some(rows)
        {
            return Err(unavailable());
        }
        let team = &self.owner.session.team_id;
        let check = |row: &Value| -> Result<(), Error> {
            // Same positive row shape as files::response, not source upsert
            // eligibility or capacity. Upsert may still fail after prefix add.
            let id = row
                .as_object()
                .and_then(|_| row["id"].as_str())
                .filter(|id| !id.is_empty())
                .ok_or_else(unavailable)?;
            if crate::file_events::changed_since(shell, team, id, self.events)
                || shell
                    .core
                    .workspaces
                    .get(team)
                    .is_none_or(|w| w.deleted_files.contains(id))
            {
                return Err(unavailable());
            }
            Ok(())
        };
        for row in rows {
            check(row)?;
            if truth(&row["canvas_metadata"]["items"]) {
                let items = row["canvas_metadata"]["items"]
                    .as_array()
                    .ok_or_else(unavailable)?;
                for item in items {
                    if truth(&item["file"]) {
                        check(&item["file"])?;
                    }
                }
            }
        }
        Ok(())
    }
    fn record_response(
        &self,
        shell: &mut ShellState,
        query: &str,
        carrier: &Value,
        rows: &[Value],
        token: Option<&files::RequestToken>,
        now_ms: i64,
    ) -> Result<(), Error> {
        // Zero-copy scan of the already bounded transport JSON. No projection
        // or source-capacity check precedes the original prefix-cache effect.
        self.response_current(shell, carrier, rows)?;
        shell
            .profile_edit
            .sections
            .file_search
            .queries
            .get_mut(&self.namespace)
            .ok_or_else(unavailable)?
            .record_response(query, rows.len(), token, false, now_ms);
        Ok(())
    }
    fn stage(
        &self,
        shell: &ShellState,
        carrier: &Value,
        rows: &[Value],
        order: Option<&source::ChannelMapOrder>,
    ) -> Result<(source::Store, Vec<String>), Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        let team = &self.owner.session.team_id;
        let slot = shell
            .profile_edit
            .sections
            .file_search
            .slots
            .get(team)
            .ok_or_else(unavailable)?;
        let mut next = slot.store.clone();
        let receipt = if let Some(order) = order {
            next.ingest_batch_ordered(carrier, rows, &self.owner.session, order)?
        } else {
            next.ingest_batch(carrier, rows, &self.owner.session)?
        };
        // Include embedded rows, and IDs absent at request time. File events
        // during acquisition cannot be hidden by a previously empty cache.
        if receipt.entries().iter().any(|entry| {
            crate::file_events::changed_since(shell, team, entry.id(), self.events)
                || shell
                    .core
                    .workspaces
                    .get(team)
                    .is_none_or(|w| w.deleted_files.contains(entry.id()))
        }) {
            return Err(unavailable());
        }
        let stamps: Vec<_> = receipt
            .entries()
            .iter()
            .map(|entry| entry.id().to_owned())
            .collect();
        Ok((next, stamps))
    }
    #[cfg(test)]
    fn publish(
        &self,
        shell: &mut ShellState,
        carrier: &Value,
        rows: &[Value],
    ) -> Result<Arc<Snapshot>, Error> {
        self.publish_ordered(shell, carrier, rows, None)
    }
    fn publish_ordered(
        &self,
        shell: &mut ShellState,
        carrier: &Value,
        rows: &[Value],
        order: Option<&source::ChannelMapOrder>,
    ) -> Result<Arc<Snapshot>, Error> {
        let (next, stamps) = self.stage(shell, carrier, rows, order)?;
        let team = &self.owner.session.team_id;
        let slot = shell
            .profile_edit
            .sections
            .file_search
            .slots
            .get_mut(team)
            .ok_or_else(unavailable)?;
        slot.store = next;
        for id in stamps {
            slot.invalidated.remove(&id);
            slot.stamps.insert(
                id,
                Stamp {
                    owner: self.owner.clone(),
                    events: self.events,
                },
            );
        }
        Snapshot::under(shell, self.owner.clone(), Some(self.publication.clone()))
    }
}
impl Read {
    pub(super) fn capture(
        shell: &mut ShellState,
        search: &Search<impl Host>,
        names: Arc<member_names::Observation>,
        now_ms: i64,
    ) -> Result<Self, Error> {
        if !search.current(shell) || !names.current(shell, search) {
            return Err(unavailable());
        }
        let owner = Arc::new(Context::capture(shell, search.lease.team())?);
        let namespace = names
            .enterprise
            .clone()
            .unwrap_or_else(|| owner.session.team_id.clone());
        Ok(Self {
            fence: Fence::capture(shell, owner, namespace, now_ms)?,
            owner: {
                let lease = search.lease.clone();
                Arc::new(move |shell| lease.current(shell))
            },
            context: search.context.clone(),
            names,
        })
    }
    fn query_current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        (self.owner)(shell)
            && search.current(shell)
            && self.names.current(shell, search)
            && Arc::ptr_eq(&self.context, &search.context)
    }
    pub(super) fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        self.query_current(shell, search) && self.fence.current(shell)
    }
    pub(super) fn publish(
        &self,
        shell: &mut ShellState,
        search: &Search<impl Host>,
        response: &files::OrderedResponse,
    ) -> Result<Arc<Snapshot>, Error> {
        if !self.current(shell, search) {
            return Err(unavailable());
        }
        self.fence.publish_ordered(
            shell,
            response.raw(),
            response.rows(),
            Some(response.channel_map_order()),
        )
    }
    pub(super) fn cached(
        &self,
        shell: &mut ShellState,
        search: &Search<impl Host>,
        query: &str,
        now_ms: i64,
    ) -> Result<CacheStatus, Error> {
        if !self.current(shell, search) {
            return Err(unavailable());
        }
        let cache = shell
            .profile_edit
            .sections
            .file_search
            .queries
            .get_mut(&self.fence.namespace)
            .ok_or_else(unavailable)?;
        Ok(CacheStatus {
            exact: cache.contains(query, now_ms),
            prefix: cache.contains_prefix_of(query, now_ms),
            in_flight: cache.is_request_in_flight(now_ms),
        })
    }
    pub(super) fn begin_request(
        &self,
        shell: &mut ShellState,
        search: &Search<impl Host>,
        now_ms: i64,
    ) -> Result<files::RequestToken, Error> {
        if !self.current(shell, search) {
            return Err(unavailable());
        }
        Ok(shell
            .profile_edit
            .sections
            .file_search
            .queries
            .get_mut(&self.fence.namespace)
            .ok_or_else(unavailable)?
            .set_request_in_flight(now_ms))
    }
    /// Actual successful response bookkeeping precedes the source upsert.
    pub(super) fn record_response(
        &self,
        shell: &mut ShellState,
        search: &Search<impl Host>,
        query: &str,
        carrier: &Value,
        rows: &[Value],
        token: Option<&files::RequestToken>,
        now_ms: i64,
    ) -> Result<(), Error> {
        if !self.current(shell, search) {
            return Err(unavailable());
        }
        self.fence
            .record_response(shell, query, carrier, rows, token, now_ms)
    }

    /// Exact-cache bookkeeping requires this read's successfully installed pair.
    pub(super) fn record_upsert(
        &self,
        shell: &mut ShellState,
        search: &Search<impl Host>,
        published: &Snapshot,
        query: &str,
        count: usize,
        now_ms: i64,
    ) -> Result<(), Error> {
        if !self.query_current(shell, search)
            || !published.current(shell)
            || !published
                .publication
                .as_ref()
                .is_some_and(|id| Arc::ptr_eq(id, &self.fence.publication))
        {
            return Err(unavailable());
        }
        shell
            .profile_edit
            .sections
            .file_search
            .queries
            .get_mut(&self.fence.namespace)
            .ok_or_else(unavailable)?
            .record_upsert(query, count, false, now_ms);
        Ok(())
    }
    /// Cleanup may outlive a query/socket, but never its credential scope and
    /// never a newer request token. It does not certify an empty response.
    pub(super) fn clear_request(&self, shell: &mut ShellState, token: &files::RequestToken) {
        if self.fence.account_current(shell) {
            if let Some(cache) = shell
                .profile_edit
                .sections
                .file_search
                .queries
                .get_mut(&self.fence.namespace)
            {
                cache.clear_request_in_flight(token);
            }
        }
    }
}
