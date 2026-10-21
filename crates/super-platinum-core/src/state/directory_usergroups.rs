//! Ordered native user-group source history. Legacy typed rows are not history.
//! Metadata projection and current-user membership are independent observations.
use crate::{config::WorkspaceSession, slack::models::UserGroup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod effective;
pub mod freshness;
mod lifecycle;
mod projection;
pub use lifecycle::BootLease;
#[cfg(test)]
mod tests;

const MAX_ROWS: usize = 100_000;
const MAX_PENDING: usize = 1024;
const MAX_BYTES: usize = 64 * 1024 * 1024;
pub type Native = HashMap<String, UserGroup>;
type NativeProof = BTreeMap<String, Value>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unknown,
    Owner,
    Stale,
    Shape,
    Limit,
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Origin {
    Upsert,
    Fetch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NativeMembers {
    Preserve,
    IncomingIfPresent,
    Replace(Vec<String>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Operation {
    Metadata {
        origin: Origin,
        rows: Vec<Value>,
        native_members: NativeMembers,
    },
    /// An actually observed replacement of the self-membership ID collection.
    Membership { ids: Vec<String> },
    /// Socket source operation; `kind` is a closed subteam event name.
    Event {
        kind: String,
        raw: Value,
        sonic: bool,
    },
    /// Actual usergroups.users.list response, never a directory metadata guess.
    Members {
        id: String,
        users: Vec<String>,
        append: bool,
        skip_org_count: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Current {
    pub source: Value,
    pub effective: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    account: String,
    team: String,
    user: String,
    rows: Vec<Current>,
    membership: Option<Vec<String>>,
    membership_flags: BTreeMap<String, bool>,
    native: NativeProof,
    pending: Vec<(String, Operation)>,
}
#[derive(Clone)]
struct Binding {
    account: String,
    owner: WorkspaceSession,
    epoch: u64,
}
impl Binding {
    fn current(&self, account: &str, owner: &WorkspaceSession, epoch: u64) -> bool {
        self.account == account && self.epoch == epoch && same(&self.owner, owner)
    }
}
#[derive(Clone)]
struct Queued {
    token: Arc<()>,
    completed: Arc<AtomicBool>,
    claimed: Arc<AtomicBool>,
    generation: String,
    operation: Operation,
}
#[derive(Clone)]
pub struct Store {
    fresh: bool,
    saved: Option<Saved>,
    binding: Option<Arc<Binding>>,
    identity: Arc<()>,
    revision: u64,
    tail: u64,
    queue: Vec<Queued>,
    gap: Arc<AtomicBool>,
    changes: tokio::sync::watch::Sender<u64>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            fresh: false,
            saved: None,
            binding: None,
            identity: Arc::new(()),
            revision: 0,
            tail: 0,
            queue: vec![],
            gap: Arc::new(AtomicBool::new(false)),
            changes: tokio::sync::watch::channel(0).0,
        }
    }
}
impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupSource")
            .field("fresh", &self.fresh)
            .field("rows", &self.saved.as_ref().map(|s| s.rows.len()))
            .field("pending", &self.queue.len())
            .finish()
    }
}

#[derive(Clone)]
pub struct Observation {
    identity: Arc<()>,
    revision: u64,
    tail: u64,
    binding: Arc<Binding>,
    native: NativeProof,
}
impl Observation {
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> bool {
        self.binding.current(account, owner, epoch)
            && self.base_current(store, native)
            && self.tail == store.tail
    }
    fn base_current(&self, store: &Store, native: &Native) -> bool {
        !store.gap.load(Ordering::Acquire)
            && Arc::ptr_eq(&self.identity, &store.identity)
            && self.revision == store.revision
            && store
                .binding
                .as_ref()
                .is_some_and(|b| Arc::ptr_eq(b, &self.binding))
            && native_proof(native).is_ok_and(|p| p == self.native)
            && store
                .saved
                .as_ref()
                .is_some_and(|s| s.native == self.native)
    }
}
#[derive(Clone)]
pub struct Snapshot {
    observed: Observation,
    rows: Arc<Vec<Current>>,
    membership: Option<Vec<String>>,
}
impl Snapshot {
    pub fn rows(&self) -> &[Current] {
        &self.rows
    }
    pub fn membership(&self) -> Option<&[String]> {
        self.membership.as_deref()
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> bool {
        !store.has_pending() && self.observed.current(store, account, owner, epoch, native)
    }
}
pub struct Ticket {
    identity: Arc<()>,
    token: Arc<()>,
    binding: Arc<Binding>,
    completed: Arc<AtomicBool>,
    gap: Arc<AtomicBool>,
    changes: tokio::sync::watch::Sender<u64>,
    receiver: tokio::sync::watch::Receiver<u64>,
}
impl Ticket {
    pub async fn wait(&mut self) -> Result<()> {
        if self.gap.load(Ordering::Acquire) {
            return Err(Error::Unknown);
        }
        self.receiver.changed().await.map_err(|_| Error::Unknown)?;
        if self.gap.load(Ordering::Acquire) {
            Err(Error::Unknown)
        } else {
            Ok(())
        }
    }
    pub fn cancel(&self) {
        if !self.completed.load(Ordering::Acquire) {
            self.gap.store(true, Ordering::Release);
            self.changes.send_modify(|v| *v = v.wrapping_add(1));
        }
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
    ) -> bool {
        !store.gap.load(Ordering::Acquire)
            && Arc::ptr_eq(&self.identity, &store.identity)
            && self.binding.current(account, owner, epoch)
            && store
                .binding
                .as_ref()
                .is_some_and(|b| Arc::ptr_eq(b, &self.binding))
    }
    pub fn finished(&self, store: &Store) -> bool {
        Arc::ptr_eq(&self.identity, &store.identity)
            && store
                .binding
                .as_ref()
                .is_some_and(|b| Arc::ptr_eq(b, &self.binding))
            && !store
                .queue
                .iter()
                .any(|q| Arc::ptr_eq(&q.token, &self.token))
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub struct Pending {
    observed: Observation,
    head: Queued,
    fields: Vec<String>,
}
impl Pending {
    pub fn generation(&self) -> &str {
        &self.head.generation
    }
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
    pub fn current(
        &self,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> bool {
        self.observed.binding.current(account, owner, epoch)
            && self.observed.base_current(store, native)
            && store
                .queue
                .first()
                .is_some_and(|q| Arc::ptr_eq(&q.token, &self.head.token))
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.head.claimed.store(false, Ordering::Release);
    }
}
pub struct RendererReply {
    pub generation: String,
    pub fields: Vec<String>,
    pub lowered: Vec<String>,
    pub locale: Option<String>,
}
pub struct Commit {
    store: Store,
    native: Native,
    results: Vec<Current>,
    completed: Option<Arc<AtomicBool>>,
}
impl Commit {
    pub fn into_parts(self) -> (Store, Native) {
        if let Some(done) = self.completed {
            done.store(true, Ordering::Release);
            self.store.changes.send_modify(|v| *v = v.wrapping_add(1));
        }
        (self.store, self.native)
    }
    pub fn results(&self) -> &[Current] {
        &self.results
    }
}

impl Store {
    /// Call only when constructing a genuinely new, uncached Workspace.
    pub fn fresh() -> Self {
        Self {
            fresh: true,
            ..Self::default()
        }
    }
    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }
    pub fn is_legacy(&self) -> bool {
        !self.fresh && self.saved.is_none()
    }
    pub fn observe(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> Result<Observation> {
        if self.gap.load(Ordering::Acquire) {
            return Err(Error::Unknown);
        }
        let binding = self.binding.as_ref().ok_or(Error::Unknown)?;
        if !binding.current(account, owner, epoch) {
            return Err(Error::Owner);
        }
        let saved = self.saved.as_ref().ok_or(Error::Unknown)?;
        let native = native_proof(native)?;
        if native != saved.native {
            return Err(Error::Stale);
        }
        Ok(Observation {
            identity: self.identity.clone(),
            revision: self.revision,
            tail: self.tail,
            binding: binding.clone(),
            native,
        })
    }
    pub fn snapshot(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> Result<Snapshot> {
        if self.has_pending() {
            return Err(Error::Unknown);
        }
        let observed = self.observe(account, owner, epoch, native)?;
        let saved = self.saved.as_ref().ok_or(Error::Unknown)?;
        Ok(Snapshot {
            observed,
            rows: Arc::new(saved.rows.clone()),
            membership: saved.membership.clone(),
        })
    }
    /// Reservation is synchronous. Tail reservations do not retire a claimed head.
    pub fn enqueue(
        &mut self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        generation: &str,
        native: &Native,
        operation: Operation,
    ) -> Result<Ticket> {
        let observed = self.observe(account, owner, epoch, native)?;
        if generation.is_empty() {
            return Err(Error::Shape);
        }
        projection::validate(&operation)?;
        if self.queue.len() >= MAX_PENDING {
            return Err(Error::Limit);
        }
        let next_tail = self.tail.checked_add(1).ok_or(Error::Limit)?;
        let queued = Queued {
            token: Arc::new(()),
            completed: Arc::new(AtomicBool::new(false)),
            claimed: Arc::new(AtomicBool::new(false)),
            generation: generation.into(),
            operation,
        };
        let bytes = serde_json::to_vec(&queued.operation)
            .map_err(|_| Error::Shape)?
            .len();
        let existing = self.queue.iter().try_fold(0usize, |n, q| {
            serde_json::to_vec(&q.operation)
                .map(|v| n.saturating_add(v.len()))
                .map_err(|_| Error::Shape)
        })?;
        if existing.saturating_add(bytes) > MAX_BYTES {
            return Err(Error::Limit);
        }
        let ticket = Ticket {
            identity: self.identity.clone(),
            token: queued.token.clone(),
            binding: observed.binding,
            completed: queued.completed.clone(),
            gap: self.gap.clone(),
            changes: self.changes.clone(),
            receiver: self.changes.subscribe(),
        };
        self.queue.push(queued);
        self.tail = next_tail;
        Ok(ticket)
    }
    pub fn next_for(
        &self,
        ticket: &Ticket,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> Result<Option<Pending>> {
        if !ticket.current(self, account, owner, epoch) {
            return Err(Error::Stale);
        }
        if !self
            .queue
            .first()
            .is_some_and(|q| Arc::ptr_eq(&q.token, &ticket.token))
        {
            return Ok(None);
        }
        self.next(account, owner, epoch, native)
    }
    fn next(
        &self,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> Result<Option<Pending>> {
        let observed = self.observe(account, owner, epoch, native)?;
        let Some(head) = self.queue.first() else {
            return Ok(None);
        };
        let fields = projection::fields(
            &self.saved.as_ref().ok_or(Error::Unknown)?.rows,
            &head.operation,
        )?;
        if head
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(None);
        }
        Ok(Some(Pending {
            observed,
            head: head.clone(),
            fields,
        }))
    }
    pub fn commit(
        &self,
        pending: Pending,
        reply: RendererReply,
        account: &str,
        owner: &WorkspaceSession,
        epoch: u64,
        native: &Native,
    ) -> Result<Commit> {
        if !pending.current(self, account, owner, epoch, native) {
            return Err(Error::Stale);
        }
        if reply.generation != pending.generation()
            || reply.fields != pending.fields
            || reply.lowered.len() != pending.fields.len()
            || (pending.fields.is_empty() && reply.locale.is_some())
            || (!pending.fields.is_empty() && reply.locale.as_ref().is_none_or(|v| v.is_empty()))
        {
            return Err(Error::Shape);
        }
        let mut store = self.clone();
        let saved = store.saved.as_mut().ok_or(Error::Unknown)?;
        let mut next_native = native.clone();
        let lowered = pending.fields.iter().cloned().zip(reply.lowered).collect();
        let results =
            projection::apply(saved, &mut next_native, &pending.head.operation, &lowered)?;
        saved.native = native_proof(&next_native)?;
        store.queue.remove(0);
        store.revision = store.revision.checked_add(1).ok_or(Error::Limit)?;
        store.check_size()?;
        Ok(Commit {
            store,
            native: next_native,
            results,
            completed: Some(pending.head.completed.clone()),
        })
    }
    fn check_size(&self) -> Result<()> {
        if self
            .saved
            .as_ref()
            .is_some_and(|s| s.rows.len() > MAX_ROWS || s.native.len() > MAX_ROWS)
            || serde_json::to_vec(&self.saved)
                .map_err(|_| Error::Shape)?
                .len()
                > MAX_BYTES
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
fn native_proof(native: &Native) -> Result<NativeProof> {
    native
        .iter()
        .map(|(id, row)| {
            if id != &row.id {
                return Err(Error::Shape);
            }
            Ok((
                id.clone(),
                serde_json::to_value(row).map_err(|_| Error::Shape)?,
            ))
        })
        .collect()
}
fn same(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.user_id == b.user_id
        && a.enterprise_id == b.enterprise_id
        && a.name == b.name
        && a.url == b.url
        && a.token == b.token
}
