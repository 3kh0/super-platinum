//! One worker orders legacy writes and explicit durable sync requests. Replies
//! to commit/prepare are never sent before SQLite commits, and failed requests
//! are never automatically retried. Only ordinary local writes are coalesced.
use super::{
    DraftKey, DraftStore, Entries, LocalDraft,
    sync::{
        self, Commit, LoadMode, Loaded, LocalChoice, LocalReview, LocalSettlement, Prepare, Release,
    },
};
use crate::{
    drafts::sync::{DraftSync, Scope},
    error::AppError,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Condvar, Mutex, OnceLock, mpsc},
};

type Updates = BTreeMap<(String, DraftKey), LocalDraft>;
type Reply<T> = mpsc::SyncSender<Result<T, AppError>>;
enum Request {
    ReadLocalReview(Scope, Reply<LocalReview>),
    SettleLocalVersions(LocalReview, LocalChoice, Reply<LocalSettlement>),
    Load(String, Reply<Entries>),
    LoadSynced(String, LoadMode, Reply<Loaded>),
    Commit(
        Scope,
        Option<u64>,
        LocalDraft,
        DraftSync,
        Option<String>,
        Reply<Commit>,
    ),
    Prepare(Scope, u64, Reply<Prepare>),
    Release(Scope, u64, Reply<Release>),
    Alternate(Scope, LocalDraft, Reply<()>),
    OwnedAlternate(Scope, LocalDraft, super::LocalWriter, u64, Reply<()>),
    ConditionalLocal(Scope, LocalDraft, LocalDraft, Reply<bool>),
    OwnedConditionalLocal(
        Scope,
        LocalDraft,
        LocalDraft,
        super::LocalWriter,
        u64,
        Reply<bool>,
    ),
}
#[derive(Default)]
struct Pending {
    updates: Updates,
    requests: Vec<Request>,
    failed: BTreeSet<String>,
    blocked: BTreeSet<String>,
}
#[derive(Default)]
struct Worker {
    pending: Mutex<Pending>,
    ready: Condvar,
}
impl Worker {
    fn enqueue(&self, account: String, entries: BTreeMap<DraftKey, LocalDraft>) {
        let mut pending = self.pending.lock().unwrap();
        for (key, draft) in entries {
            pending.updates.insert((account.clone(), key), draft);
        }
        self.ready.notify_one();
    }
    fn submit(&self, request: Request) {
        self.pending.lock().unwrap().requests.push(request);
        self.ready.notify_one();
    }
    fn cycle(&self, open: impl Fn(&str) -> Result<DraftStore, AppError>) -> bool {
        let (updates, requests) = {
            let mut pending = self.pending.lock().unwrap();
            (
                std::mem::take(&mut pending.updates),
                std::mem::take(&mut pending.requests),
            )
        };
        let mut accounts: BTreeMap<String, Entries> = BTreeMap::new();
        for ((account, key), draft) in updates {
            accounts.entry(account).or_default().push((key, draft));
        }
        let mut retry = false;
        for (account, entries) in accounts {
            let result = open(&account).and_then(|mut store| store.save_legacy(&entries));
            let mut pending = self.pending.lock().unwrap();
            match result {
                Ok(rejected) => {
                    pending.failed.remove(&account);
                    if !rejected.is_empty() {
                        pending.blocked.insert(account);
                    }
                }
                Err(_) => {
                    retry = true;
                    pending.failed.insert(account.clone());
                    for (key, draft) in entries {
                        pending
                            .updates
                            .entry((account.clone(), key))
                            .or_insert(draft);
                    }
                }
            }
        }
        for request in requests {
            match request {
                Request::ReadLocalReview(scope, reply) => {
                    let result = open(&scope.account_id).and_then(|mut store| {
                        if self
                            .pending
                            .lock()
                            .unwrap()
                            .updates
                            .contains_key(&(scope.account_id.clone(), scope.key.clone()))
                        {
                            return Err(sync::invalid("local draft edits have not committed"));
                        }
                        store.read_local_review(&scope)
                    });
                    let _ = reply.send(result);
                }
                Request::SettleLocalVersions(review, choice, reply) => {
                    let scope = &review.scope;
                    let result = open(&scope.account_id).and_then(|mut store| {
                        // Linearizes before later enqueues. Those inputs remain
                        // queued; no UI mutex is held across SQLite I/O.
                        if self.pending.lock().unwrap().updates.contains_key(&(scope.account_id.clone(), scope.key.clone())) {
                            return Ok(LocalSettlement::Blocked);
                        }
                        let result = store.settle_local_versions(&review, &choice)?;
                        let blocked: bool = store.0.query_row("SELECT EXISTS(SELECT 1 FROM composer_draft_blocked) OR EXISTS(SELECT 1 FROM composer_draft_alternates)", [], |row| row.get(0))?;
                        let mut pending = self.pending.lock().unwrap();
                        if blocked { pending.blocked.insert(scope.account_id.clone()); }
                        else { pending.blocked.remove(&scope.account_id); }
                        Ok(result)
                    });
                    let _ = reply.send(result);
                }
                Request::Load(account, reply) => {
                    let result = open(&account).and_then(|store| {
                        let managed = store.managed_keys()?;
                        let mut entries: BTreeMap<_, _> = store.load()?.into_iter().collect();
                        for ((owner, key), draft) in &self.pending.lock().unwrap().updates {
                            if owner == &account && !managed.contains(key) {
                                if draft.empty() {
                                    entries.remove(key);
                                } else {
                                    entries.insert(key.clone(), draft.clone());
                                }
                            }
                        }
                        Ok(entries.into_iter().collect())
                    });
                    let _ = reply.send(result);
                }
                Request::LoadSynced(account, mode, reply) => {
                    let result = open(&account)
                        .and_then(|mut store| store.load_synced(&account, mode))
                        .map(|mut loaded| {
                            let mut pending = self.pending.lock().unwrap();
                            if loaded.blocked_local.is_empty() && loaded.local_alternates.is_empty()
                            {
                                pending.blocked.remove(&account);
                            } else {
                                pending.blocked.insert(account.clone());
                            }
                            loaded.uncommitted_local = pending
                                .updates
                                .iter()
                                .filter(|((owner, _), _)| owner == &account)
                                .map(|((_, key), draft)| (key.clone(), draft.clone()))
                                .collect();
                            loaded
                        });
                    let _ = reply.send(result);
                }
                Request::Commit(scope, expected, local, state, settle, reply) => {
                    let result = open(&scope.account_id).and_then(|mut store| {
                        let result = store.commit_synced(
                            &scope,
                            expected,
                            &local,
                            &state,
                            settle.as_deref(),
                        )?;
                        let blocked: bool = store.0.query_row(
                            "SELECT EXISTS(SELECT 1 FROM composer_draft_blocked)",
                            [],
                            |row| row.get(0),
                        )?;
                        let mut pending = self.pending.lock().unwrap();
                        if blocked {
                            pending.blocked.insert(scope.account_id.clone());
                        } else {
                            pending.blocked.remove(&scope.account_id);
                        }
                        Ok(result)
                    });
                    let _ = reply.send(result);
                }
                Request::ConditionalLocal(scope, expected, local, reply) => {
                    let result = open(&scope.account_id).and_then(|mut store| {
                        let waiting = self
                            .pending
                            .lock()
                            .unwrap()
                            .updates
                            .contains_key(&(scope.account_id.clone(), scope.key.clone()));
                        if waiting {
                            store.preserve_local_conflict(&scope, &local)?;
                            return Ok(false);
                        }
                        store.save_local_if_unchanged(&scope, &expected, &local)
                    });
                    if matches!(&result, Ok(false)) {
                        self.pending
                            .lock()
                            .unwrap()
                            .blocked
                            .insert(scope.account_id.clone());
                    }
                    let _ = reply.send(result);
                }
                Request::OwnedConditionalLocal(scope, expected, local, writer, revision, reply) => {
                    let result = open(&scope.account_id).and_then(|mut store| {
                        let waiting = self
                            .pending
                            .lock()
                            .unwrap()
                            .updates
                            .contains_key(&(scope.account_id.clone(), scope.key.clone()));
                        if waiting {
                            store
                                .preserve_owned_local_conflict(&scope, &local, &writer, revision)?;
                            return Ok(false);
                        }
                        store.save_owned_local_if_unchanged(
                            &scope, &expected, &local, &writer, revision,
                        )
                    });
                    if matches!(&result, Ok(false)) {
                        self.pending
                            .lock()
                            .unwrap()
                            .blocked
                            .insert(scope.account_id.clone());
                    }
                    let _ = reply.send(result);
                }
                Request::Release(scope, expected, reply) => {
                    let result = open(&scope.account_id).and_then(|mut store| {
                        // This snapshot orders release before later enqueues.
                        // The single worker applies those in its next cycle;
                        // never hold the UI enqueue mutex during disk I/O.
                        let waiting = self
                            .pending
                            .lock()
                            .unwrap()
                            .updates
                            .contains_key(&(scope.account_id.clone(), scope.key.clone()));
                        if waiting {
                            return Err(sync::invalid("local draft edits have not committed"));
                        }
                        store.release_synced(&scope, expected)
                    });
                    let _ = reply.send(result);
                }
                Request::Alternate(scope, local, reply) => {
                    let result = open(&scope.account_id)
                        .and_then(|mut store| store.save_local_alternate(&scope, local));
                    if result.is_ok() {
                        self.pending
                            .lock()
                            .unwrap()
                            .blocked
                            .insert(scope.account_id);
                    }
                    let _ = reply.send(result);
                }
                Request::OwnedAlternate(scope, local, writer, revision, reply) => {
                    let result = open(&scope.account_id).and_then(|mut store| {
                        store.save_owned_local_alternate(&scope, &local, &writer, revision)
                    });
                    if result.is_ok() {
                        self.pending
                            .lock()
                            .unwrap()
                            .blocked
                            .insert(scope.account_id);
                    }
                    let _ = reply.send(result);
                }
                Request::Prepare(scope, expected, reply) => {
                    // A failed or newer queued local edit must be dealt with
                    // before authorizing a request for older persisted content.
                    let waiting = self
                        .pending
                        .lock()
                        .unwrap()
                        .updates
                        .contains_key(&(scope.account_id.clone(), scope.key.clone()));
                    let result = if waiting {
                        Err(sync::invalid("local draft edits have not committed"))
                    } else {
                        open(&scope.account_id)
                            .and_then(|mut store| store.prepare_synced(&scope, expected))
                    };
                    let _ = reply.send(result);
                }
            }
        }
        retry
    }
    fn run(&self) {
        loop {
            {
                let mut pending = self.pending.lock().unwrap();
                while pending.updates.is_empty() && pending.requests.is_empty() {
                    pending = self.ready.wait(pending).unwrap();
                }
            }
            if self.cycle(DraftStore::open_default) {
                let pending = self.pending.lock().unwrap();
                let _ = self
                    .ready
                    .wait_timeout(pending, std::time::Duration::from_secs(1))
                    .unwrap();
            }
        }
    }
}
fn worker() -> &'static Worker {
    static WORKER: OnceLock<Worker> = OnceLock::new();
    WORKER.get_or_init(|| {
        std::thread::Builder::new()
            .name("super-platinum-drafts".into())
            .spawn(|| worker().run())
            .expect("draft worker starts");
        Worker::default()
    })
}
fn receive<T>(receiver: mpsc::Receiver<Result<T, AppError>>) -> Result<T, AppError> {
    receiver
        .recv()
        .map_err(|_| sync::invalid("draft worker stopped"))?
}
/// Fire-and-forget local edits, coalesced by destination. Enrolled destinations
/// are retained as blocked alternate edits on disk, never applied to sync state.
pub fn enqueue(account: String, entries: BTreeMap<DraftKey, LocalDraft>) {
    if !entries.is_empty() {
        worker().enqueue(account, entries);
    }
}
/// Call only on a background worker. This legacy load may include uncommitted
/// local-only edits and must never authorize Slack writes.
pub fn load(account: &str) -> Result<Entries, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::Load(account.into(), sender));
    receive(receiver)
}
/// Background-only load; AfterRestart must run before any new dispatch.
pub fn load_synced(account: &str, mode: LoadMode) -> Result<Loaded, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::LoadSynced(account.into(), mode, sender));
    receive(receiver)
}
/// Background-only atomic paired save. No automatic retries on failure.
/// To explicitly settle a blocked legacy edit, pass its token from load_synced.
pub fn commit_synced(
    scope: Scope,
    expected: Option<u64>,
    local: LocalDraft,
    state: DraftSync,
    settle_blocked: Option<String>,
) -> Result<Commit, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::Commit(
        scope,
        expected,
        local,
        state,
        settle_blocked,
        sender,
    ));
    receive(receiver)
}
/// Background-only dispatch barrier. A lost reply never permits retrying a
/// create. Reload and reconcile the saved operation's identity instead.
pub fn prepare_synced(scope: Scope, expected: u64) -> Result<Prepare, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::Prepare(scope, expected, sender));
    receive(receiver)
}
/// Queued at UI event time; waiting for its commit belongs off the UI thread.
pub struct LocalWrite(mpsc::Receiver<Result<bool, AppError>>);
impl LocalWrite {
    pub fn wait(self) -> Result<bool, AppError> {
        receive(self.0)
    }
}
pub fn queue_local_if_unchanged(
    scope: Scope,
    expected: LocalDraft,
    local: LocalDraft,
) -> LocalWrite {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::ConditionalLocal(scope, expected, local, sender));
    LocalWrite(receiver)
}

pub fn queue_owned_local_if_unchanged(
    scope: Scope,
    expected: LocalDraft,
    local: LocalDraft,
    writer: super::LocalWriter,
    revision: u64,
) -> LocalWrite {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::OwnedConditionalLocal(
        scope, expected, local, writer, revision, sender,
    ));
    LocalWrite(receiver)
}

/// Background-only release barrier; pending local edits block release.
pub fn release_synced(scope: Scope, expected: u64) -> Result<Release, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::Release(scope, expected, sender));
    receive(receiver)
}
/// Preserve an alternate without permitting a concurrent release to turn it
/// into an ordinary local overwrite.
pub fn save_local_alternate(scope: Scope, local: LocalDraft) -> Result<(), AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::Alternate(scope, local, sender));
    receive(receiver)
}

pub fn save_owned_local_alternate(
    scope: Scope,
    local: LocalDraft,
    writer: super::LocalWriter,
    revision: u64,
) -> Result<(), AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::OwnedAlternate(
        scope, local, writer, revision, sender,
    ));
    receive(receiver)
}

/// Background-only immutable inventory for an explicit local recovery choice.
pub fn read_local_review(scope: Scope) -> Result<LocalReview, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::ReadLocalReview(scope, sender));
    receive(receiver)
}
/// Background-only CAS. Does not prepare, retry, or dispatch any Slack action.
pub fn settle_local_versions(
    review: LocalReview,
    choice: LocalChoice,
) -> Result<LocalSettlement, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::SettleLocalVersions(review, choice, sender));
    receive(receiver)
}

pub fn save_failed(account: &str) -> bool {
    let pending = worker().pending.lock().unwrap();
    pending.failed.contains(account) || pending.blocked.contains(account)
}

#[cfg(test)]
mod tests;
