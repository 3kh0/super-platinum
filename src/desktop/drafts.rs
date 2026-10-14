//! One async owner per account/destination, shared by local editing and delivery.
//! Ordinary remote draft autosave remains gated on complete response proof.
//! I/O runs off the UI thread; operation outcomes persist to the captured account
//! regardless of which workspace or component remains visible.
#![allow(dead_code)] // Remote autosave/control APIs await protocol verification.
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex, OnceLock},
};
use super_platinum_core::drafts::{
    DraftKey, LocalDraft, codec,
    store::sync::{BlockedLocal, Commit, LoadMode, Loaded, Prepare, Stored},
    sync::{DraftSync, Operation, RemoteDraft, Scope, Snapshot},
};
mod remote_review;
pub(crate) mod remote_review_check;
pub(crate) mod remote_review_fixture;
mod storage;
use storage::Backend;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    Storage,
    Stale,
    Unmanaged,
    AlreadyManaged,
    Unsupported,
    AlternateEdits,
    Pending,
    InvalidReceipt,
}

#[derive(Clone, Debug)]
pub(crate) struct View {
    pub local: LocalDraft,
    pub stored: Option<Stored>,
    pub blocked: Option<BlockedLocal>,
    pub uncommitted: Option<LocalDraft>,
}
impl View {
    fn from_loaded(mut loaded: Loaded, key: &DraftKey) -> Self {
        Self {
            local: loaded
                .local
                .into_iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or_default(),
            stored: loaded.synced.remove(key),
            blocked: loaded.blocked_local.remove(key),
            uncommitted: loaded.uncommitted_local.remove(key),
        }
    }
    fn ready(&self, expected: u64) -> Result<Stored, Fault> {
        if self.blocked.is_some() || self.uncommitted.is_some() {
            return Err(Fault::AlternateEdits);
        }
        let stored = self.stored.clone().ok_or(Fault::Unmanaged)?;
        if stored.revision != expected {
            return Err(Fault::Stale);
        }
        Ok(stored)
    }
}

pub(crate) struct Account {
    backend: Backend,
    initialization: tokio::sync::Mutex<Initialization>,
    gates: Mutex<BTreeMap<DraftKey, Arc<tokio::sync::Mutex<()>>>>,
}
impl Account {
    fn new(backend: Backend) -> Arc<Self> {
        Arc::new(Self {
            backend,
            initialization: tokio::sync::Mutex::new(Initialization::New),
            gates: Mutex::new(BTreeMap::new()),
        })
    }
    /// Shared across account switches. A second view must never run restart
    /// normalization while another view has a live request in flight.
    pub fn production(account: &str) -> Result<Arc<Self>, Fault> {
        if crate::fixture::is_fixture() {
            return Err(Fault::Storage);
        }
        static ACCOUNTS: OnceLock<Mutex<BTreeMap<String, Arc<Account>>>> = OnceLock::new();
        let mut accounts = ACCOUNTS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| Fault::Storage)?;
        Ok(accounts
            .entry(account.into())
            .or_insert_with(|| Self::new(Backend::Account(account.into())))
            .clone())
    }
    async fn initialize(&self) -> Result<(), Fault> {
        let mut initialization = self.initialization.lock().await;
        let backend = self.backend.clone();
        initialization
            .finish(move || async move { backend.load(LoadMode::AfterRestart).await.map(|_| ()) })
            .await
    }
    pub fn owner(self: &Arc<Self>, key: DraftKey) -> Owner {
        let gate = self
            .gates
            .lock()
            .unwrap()
            .entry(key.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        Owner {
            scope: Scope {
                account_id: self.backend.account().into(),
                key,
            },
            account: self.clone(),
            gate,
        }
    }
}

/// Keep the initialization task owned by the account when a view disappears.
/// Dropping a OnceCell initializer could leave its blocking normalization alive
/// while another initializer returns and dispatches a new operation.
enum Initialization {
    New,
    Running(tokio::task::JoinHandle<Result<(), Fault>>),
    Ready,
}
impl Initialization {
    async fn finish<F, Fut>(&mut self, start: F) -> Result<(), Fault>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), Fault>> + Send + 'static,
    {
        if matches!(self, Self::New) {
            *self = Self::Running(tokio::spawn(start()));
        }
        if let Self::Running(task) = self {
            let result = task.await.unwrap_or(Err(Fault::Storage));
            *self = if result.is_ok() {
                Self::Ready
            } else {
                Self::New
            };
            result
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
pub(crate) struct Owner {
    scope: Scope,
    account: Arc<Account>,
    gate: Arc<tokio::sync::Mutex<()>>,
}
fn local_snapshot(key: &DraftKey, local: &LocalDraft) -> Result<Option<Snapshot>, Fault> {
    if !local.attachments.is_empty() {
        return Err(Fault::Unsupported);
    }
    if local.composer.text.is_empty() && local.attachments.is_empty() {
        return Ok(None);
    }
    codec::encode_composer(key, &local.composer)
        .map(Some)
        .map_err(|_| Fault::Unsupported)
}
/// Reuse the exact supported representation already owned by this draft. A
/// caret-only edit must not erase explicit destination metadata or turn an
/// unchanged empty remote snapshot into a deletion. Unknown raw shapes stay
/// opaque; they never authorize a fallback through the local text encoder.
fn retained_snapshot(
    key: &DraftKey,
    local: &LocalDraft,
    retained: Option<&Snapshot>,
) -> Result<Option<Snapshot>, Fault> {
    let Some(raw) = retained else {
        return local_snapshot(key, local);
    };
    if !local.attachments.is_empty() {
        return Err(Fault::Unsupported);
    }
    let projection = codec::decode(key, raw).map_err(|_| Fault::Unsupported)?;
    if !projection.composer().text.is_empty() && local.composer.text.is_empty() {
        return Ok(None);
    }
    projection
        .snapshot_for(&local.composer)
        .map(Some)
        .map_err(|_| Fault::Unsupported)
}
fn same_local(a: &LocalDraft, b: &LocalDraft) -> bool {
    matches!((serde_json::to_value(a), serde_json::to_value(b)), (Ok(a), Ok(b)) if a == b)
}
impl Owner {
    async fn current(&self) -> Result<View, Fault> {
        self.account.initialize().await?;
        Ok(View::from_loaded(
            self.account.backend.load(LoadMode::Current).await?,
            &self.scope.key,
        ))
    }
    pub async fn load(&self) -> Result<View, Fault> {
        let _guard = self.gate.lock().await;
        self.current().await
    }
    async fn save(
        &self,
        expected: Option<u64>,
        local: LocalDraft,
        state: DraftSync,
    ) -> Result<Stored, Fault> {
        match self
            .account
            .backend
            .commit(self.scope.clone(), expected, local, state)
            .await?
        {
            Commit::Applied(stored) => Ok(stored),
            Commit::Stale(_) => Err(Fault::Stale),
        }
    }
    /// Explicit enrollment cannot overwrite a newer legacy edit. Unsupported
    /// content remains in its existing local/remote representation.
    pub async fn enroll(
        &self,
        local: LocalDraft,
        remote: Option<RemoteDraft>,
    ) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let view = self.current().await?;
        if view.stored.is_some() {
            return Err(Fault::AlreadyManaged);
        }
        if view.blocked.is_some() || view.uncommitted.is_some() {
            return Err(Fault::AlternateEdits);
        }
        if !same_local(&view.local, &local) {
            return Err(Fault::Stale);
        }
        let state = if let Some(remote) = remote {
            let snapshot = retained_snapshot(&self.scope.key, &local, Some(&remote.snapshot))?;
            if snapshot.as_ref() != Some(&remote.snapshot) {
                return Err(Fault::Unsupported);
            }
            DraftSync::adopt_remote(&self.scope, remote).map_err(|_| Fault::Unsupported)?
        } else {
            let snapshot = local_snapshot(&self.scope.key, &local)?;
            let mut state = DraftSync::new(self.scope.clone());
            state.edit(&self.scope, snapshot);
            state
        };
        self.save(None, local, state).await
    }
    /// Strict revision input makes stale UI callbacks observable. A caller may
    /// coalesce its own edits, but must never silently rebase another view's edit.
    pub async fn checkpoint(&self, expected: u64, local: LocalDraft) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let mut stored = self.current().await?.ready(expected)?;
        let snapshot = retained_snapshot(&self.scope.key, &local, stored.state.desired())?;
        stored.state.edit(&self.scope, snapshot);
        self.save(Some(expected), local, stored.state).await
    }
    pub async fn clear(&self, expected: u64) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let mut stored = self.current().await?.ready(expected)?;
        // Explicit clear is intent, including when an adopted raw snapshot was
        // already empty. Ordinary unchanged checkpoints preserve that snapshot.
        stored.state.edit(&self.scope, None);
        self.save(Some(expected), LocalDraft::default(), stored.state)
            .await
    }
}

pub(crate) mod checkpoint;
mod control;
mod dispatch;
pub(crate) mod integration;
pub(crate) mod network;
pub(crate) mod observation;
pub(crate) mod projection_check;
pub(crate) mod projection_fixture;
pub(crate) mod recovery;
pub(crate) mod rejected_fixture;
pub(crate) mod rejected_thread_fixture;
mod release;
pub(crate) mod send;
pub(crate) mod session;
pub(crate) mod settlement;
pub(crate) mod settlement_fixture;
#[cfg(test)]
use dispatch::Outcome;
#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod tests;
