//! One worker orders legacy writes and explicit durable sync requests. Replies
//! to commit/prepare are never sent before SQLite commits, and failed requests
//! are never automatically retried. Only ordinary local writes are coalesced.
use super::terminal;
use super::{
    DraftKey, DraftStore, Entries, LocalDraft,
    sync::{
        self, Commit, LoadMode, Loaded, LocalChoice, LocalReview, LocalSettlement, Prepare,
        RejectedScheduleCommit, Release, RemoteChoice, RemoteReview, RemoteSettlement,
        ReviewedScheduleCommit,
    },
};
use crate::{
    drafts::sync::{DraftSync, RejectedScheduleChoice, Scope, Snapshot},
    error::AppError,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Condvar, Mutex, OnceLock, mpsc},
};

mod accounting;
mod admission;
pub(super) mod configured;
pub(crate) mod protected_queue;
#[path = "worker/request_identity.rs"]
mod request_identity;
pub(super) mod uploads;

type Updates = BTreeMap<(String, DraftKey), LocalDraft>;
type Reply<T> = mpsc::SyncSender<Result<T, AppError>>;
enum Request {
    StoreUpgrade(Box<super::protected::upgrade::Command>),
    SlashJournal(Box<crate::slash_commands::journal::Command>),
    SlashProtected(Box<super::protected::slash_runtime::Command>),
    Hosted(Box<super::protected::hosted_files::runtime::Command>),
    Protected(Box<super::protected::commands::Command>),
    Upload(uploads::Command),
    ReadLocalReview(Scope, Reply<LocalReview>),
    SettleLocalVersions(LocalReview, LocalChoice, Reply<LocalSettlement>),
    ReconcileRemoteReview(RemoteReview, RemoteChoice, Reply<RemoteSettlement>),
    CommitReviewedSchedule(LocalReview, Snapshot, i64, Reply<ReviewedScheduleCommit>),
    ReviewRejectedSchedule(
        LocalReview,
        RejectedScheduleChoice,
        i64,
        Reply<RejectedScheduleCommit>,
    ),
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
    accounting: Option<accounting::Book>,
    admission: Option<admission::State>,
    upgrade_generation: u64,
    updates: Updates,
    refused_configured_updates: Updates,
    requests: Vec<Request>,
    failed: BTreeSet<String>,
    blocked: BTreeSet<String>,
}
#[derive(Default)]
struct Worker {
    #[cfg(test)]
    slash_open_gate: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    upgrade_id: OnceLock<String>,
    protected_fixture: bool,
    configured_default: bool,
    pending: Mutex<Pending>,
    ready: Condvar,
    accounted: Condvar,
    #[cfg(test)]
    accounted_waiters: std::sync::atomic::AtomicUsize,
}
impl Worker {
    fn enqueue(&self, account: String, entries: BTreeMap<DraftKey, LocalDraft>) {
        if self.configured_default && !configured::registry().ordinary(&account) {
            let mut pending = self.pending.lock().unwrap();
            pending.failed.insert(account.clone());
            for (key, value) in entries {
                pending
                    .refused_configured_updates
                    .insert((account.clone(), key), value);
            }
            return;
        }
        let mut pending = self.pending.lock().unwrap();
        {
            pending.upgrade_generation = pending.upgrade_generation.saturating_add(1);
            if let Some(admission) = &mut pending.admission {
                admission.unsupported = true;
            }
        }
        for (key, draft) in entries {
            if let Some(book) = &mut pending.accounting {
                book.update(&account, &key, false);
            }
            pending.updates.insert((account.clone(), key), draft);
        }
        self.ready.notify_one();
    }
    fn submit(&self, request: Request) {
        self.submit_recorded(request, false);
    }
    fn submit_bootstrap(&self, request: Request) {
        // Only existing retained A/B startup routes call this. Old Client calls
        // are latched before routing, including rejected AfterRestart attempts.
        let bootstrap = matches!(&request, Request::StoreUpgrade(_))
            || matches!(&request, Request::Protected(command) if matches!(&command.action, super::protected::commands::Action::Load {mode:super::protected::LoadMode::AfterRestart,..}));
        self.submit_recorded(request, bootstrap);
    }
    fn submit_recorded(&self, request: Request, _bootstrap: bool) {
        if self.configured_default {
            configured::registry().ordinary(request.identity().account);
        }
        let mut pending = self.pending.lock().unwrap();
        if !_bootstrap {
            if let Some(admission) = &mut pending.admission {
                admission.unsupported = true;
            }
        }
        {
            pending.upgrade_generation = pending.upgrade_generation.saturating_add(1);
        }
        {
            let index = pending.requests.len();
            if let Some(book) = &mut pending.accounting {
                book.request(index, request.identity());
            }
        }
        pending.requests.push(request);
        drop(pending);
        self.ready.notify_one();
    }
    fn cycle(&self, open: impl Fn(&str) -> Result<DraftStore, AppError>) -> bool {
        let upgrade_generation;
        let accounting_batch;
        let (updates, requests) = {
            let mut pending = self.pending.lock().unwrap();
            {
                upgrade_generation = pending.upgrade_generation;
            }
            {
                let requests = pending.requests.len();
                let updates = pending.updates.len();
                accounting_batch = pending
                    .accounting
                    .as_mut()
                    .map(|book| book.detach(requests, updates))
                    .unwrap_or_default();
            }
            (
                std::mem::take(&mut pending.updates),
                std::mem::take(&mut pending.requests),
            )
        };
        let mut executing = accounting::Executing::new(self, accounting_batch);
        let mut accounts: BTreeMap<String, Entries> = BTreeMap::new();
        for ((account, key), draft) in updates {
            accounts.entry(account).or_default().push((key, draft));
        }
        let mut retry = false;
        for (account, entries) in accounts {
            let diagnostic_account = executing.update_account(&account);
            let result = open(&account).and_then(|mut store| store.save_legacy(&entries));
            let update_reports = executing.update_reports(&account, &result);
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
                        let Pending {
                            updates,
                            accounting,
                            ..
                        } = &mut *pending;
                        // Move the original key into the real entry. Diagnostics
                        // borrow it only after proving this retry is inserted;
                        // no unbounded diagnostic key clone under Pending.
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            updates.entry((account.clone(), key))
                        {
                            if let Some(book) = accounting {
                                let (owner, key) = entry.key();
                                book.update(owner, key, true);
                            }
                            entry.insert(draft);
                        }
                    }
                }
            }
            drop(pending);
            if let Some(account) = diagnostic_account {
                executing.finish_updates(&account, update_reports);
            }
        }
        let upgrade_sole = requests.len() == 1;
        let mut slash_detached: BTreeMap<String, usize> = {
            let mut counts = BTreeMap::new();
            for request in &requests {
                if let Some(account) = request.slash_fence_account() {
                    *counts.entry(account.to_owned()).or_insert(0) += 1;
                }
            }
            counts
        };
        for (_request_index, request) in requests.into_iter().enumerate() {
            if let Some(account) = request.slash_fence_account() {
                if let Some(count) = slash_detached.get_mut(account) {
                    *count -= 1;
                }
            }
            let _report = match request {
                Request::SlashProtected(command) => {
                    let account = command.account().to_owned();
                    match open(&account) {
                        Ok(mut store) => {
                            #[cfg(test)]
                            if let Some((entered, resume)) =
                                self.slash_open_gate.lock().unwrap().take()
                            {
                                entered.send(()).expect("isolated opener observer");
                                resume
                                    .recv_timeout(std::time::Duration::from_secs(5))
                                    .expect("isolated opener release");
                            }
                            // Sample AFTER opener, covering same-batch detached work,
                            // new arrivals, failed writes and preserved blocked input.
                            let pending = self.pending.lock().unwrap();
                            let activity = pending.upgrade_generation;
                            let failed_inputs = self.slash_failed_inputs(&pending);
                            let blocked = failed_inputs
                                || slash_detached.get(&account).is_some_and(|n| *n > 0)
                                || pending.failed.contains(&account)
                                || pending.blocked.contains(&account)
                                || pending.updates.keys().any(|(owner, _)| owner == &account)
                                || pending
                                    .requests
                                    .iter()
                                    .any(|r| r.slash_fence_account() == Some(account.as_str()));
                            drop(pending);
                            (*command).execute(Ok(&mut store), blocked, activity)
                        }
                        Err(_) => {
                            (*command).execute(Err(super::protected::Error::Storage), false, 0)
                        }
                    }
                }
                Request::Hosted(command) => {
                    let account = command.account().to_owned();
                    match open(&account) {
                        Ok(mut store) => {
                            let pending = self.pending.lock().unwrap();
                            let activity = pending.upgrade_generation;
                            let blocked = self.slash_failed_inputs(&pending)
                                || slash_detached.get(&account).is_some_and(|n| *n > 0)
                                || pending.failed.contains(&account)
                                || pending.blocked.contains(&account)
                                || pending.updates.keys().any(|(a, _)| a == &account)
                                || pending
                                    .requests
                                    .iter()
                                    .any(|r| r.slash_fence_account() == Some(account.as_str()));
                            drop(pending);
                            (*command).execute(Ok(&mut store), blocked, activity)
                        }
                        Err(_) => {
                            (*command).execute(Err(super::protected::Error::Storage), false, 0)
                        }
                    }
                }
                Request::StoreUpgrade(command) => {
                    upgrade::execute(self, *command, upgrade_generation, upgrade_sole)
                }
                Request::SlashJournal(command) => {
                    let account = command.account().to_owned();
                    match open(&account) {
                        Ok(mut store) => {
                            let blocked = {
                                let pending = self.pending.lock().unwrap();
                                pending.failed.contains(&account)
                                    || pending.blocked.contains(&account)
                                    || pending.updates.keys().any(|(owner, _)| owner == &account)
                            };
                            store.slash_journal_command(*command, blocked)
                        }
                        Err(_) => (*command)
                            .execute(Err(crate::slash_commands::Failure::Persistence), false),
                    }
                }
                Request::Protected(command) => {
                    let updates = self
                        .pending
                        .lock()
                        .unwrap()
                        .updates
                        .iter()
                        .filter(|((owner, _), _)| owner == &command.account)
                        .map(|((_, key), value)| (key.clone(), value.clone()))
                        .collect();
                    command.execute(&open, updates, self.protected_fixture)
                }

                Request::Upload(command) => uploads::execute(command, &open),
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
                    let report = terminal::app(&result, terminal::read);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::legacy::local_settlement);
                    let _ = reply.send(result);
                    report
                }
                Request::ReconcileRemoteReview(review, choice, reply) => {
                    let scope = &review.inventory().scope;
                    let result = open(&scope.account_id).and_then(|mut store| {
                        // Failed/queued legacy input is not part of the durable
                        // inventory the human reviewed. Keep it and refuse CAS.
                        // Later enqueues remain queued; no UI mutex spans I/O.
                        if self.pending.lock().unwrap().updates.contains_key(&(scope.account_id.clone(), scope.key.clone())) {
                            return Ok(RemoteSettlement::Blocked);
                        }
                        let result = store.reconcile_remote_review(&review, &choice)?;
                        let blocked: bool = store.0.query_row("SELECT EXISTS(SELECT 1 FROM composer_draft_blocked) OR EXISTS(SELECT 1 FROM composer_draft_alternates)", [], |row| row.get(0))?;
                        let mut pending = self.pending.lock().unwrap();
                        if blocked { pending.blocked.insert(scope.account_id.clone()); }
                        else { pending.blocked.remove(&scope.account_id); }
                        Ok(result)
                    });
                    let report = terminal::app(&result, terminal::legacy::remote_settlement);
                    let _ = reply.send(result);
                    report
                }
                Request::CommitReviewedSchedule(expected, reviewed, post_at, reply) => {
                    let scope = &expected.scope;
                    let result = open(&scope.account_id).and_then(|mut store| {
                        // Failed or queued legacy input is not in this durable review. Refuse
                        // at the same pending-input linearization boundary as RemoteReview.
                        // Later inputs remain queued. Never hold a UI mutex across disk I/O.
                        if self
                            .pending
                            .lock()
                            .unwrap()
                            .updates
                            .contains_key(&(scope.account_id.clone(), scope.key.clone()))
                        {
                            return Ok(ReviewedScheduleCommit::Blocked);
                        }
                        store.commit_reviewed_schedule(&expected, &reviewed, post_at)
                    });
                    let report = terminal::app(&result, terminal::legacy::reviewed_schedule);
                    let _ = reply.send(result);
                    report
                }
                Request::ReviewRejectedSchedule(expected, choice, now_secs, reply) => {
                    let scope = &expected.scope;
                    let result = open(&scope.account_id).and_then(|mut store| {
                        if self
                            .pending
                            .lock()
                            .unwrap()
                            .updates
                            .contains_key(&(scope.account_id.clone(), scope.key.clone()))
                        {
                            return Ok(RejectedScheduleCommit::Blocked);
                        }
                        store.review_rejected_schedule(&expected, &choice, now_secs)
                    });
                    let report = terminal::app(&result, terminal::legacy::rejected_schedule);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::read);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, |_| match mode {
                        LoadMode::Current => {
                            terminal::Report::new(terminal::Execution::ReadObserved)
                        }
                        LoadMode::AfterRestart => {
                            terminal::Report::new(terminal::Execution::StorageAccepted)
                        }
                    });
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::legacy::commit);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::legacy::conditional);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::legacy::conditional);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::legacy::release);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::accepted);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::accepted);
                    let _ = reply.send(result);
                    report
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
                    let report = terminal::app(&result, terminal::legacy::prepare);
                    let _ = reply.send(result);
                    report
                }
            };
            executing.finish_request(_request_index, _report);
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
            if self.cycle(|account| configured::registry().open_ordinary(account)) {
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
        Worker {
            configured_default: true,
            ..Worker::default()
        }
    })
}
pub(crate) fn enqueue_slash_journal(command: crate::slash_commands::journal::Command) {
    worker().submit(Request::SlashJournal(Box::new(command)));
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

/// Background-only explicit positive review CAS, serialized with local writers.
/// A lost reply is recovered by reading; this never retries or dispatches Slack.
pub fn reconcile_remote_review(
    review: RemoteReview,
    choice: RemoteChoice,
) -> Result<RemoteSettlement, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::ReconcileRemoteReview(review, choice, sender));
    receive(receiver)
}

/// Background-only exact inventory CAS; never prepares or dispatches Slack.
pub fn commit_reviewed_schedule(
    expected: LocalReview,
    reviewed: Snapshot,
    post_at: i64,
) -> Result<ReviewedScheduleCommit, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::CommitReviewedSchedule(
        expected, reviewed, post_at, sender,
    ));
    receive(receiver)
}

pub fn review_rejected_schedule(
    expected: LocalReview,
    choice: RejectedScheduleChoice,
    now_secs: i64,
) -> Result<RejectedScheduleCommit, AppError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    worker().submit(Request::ReviewRejectedSchedule(
        expected, choice, now_secs, sender,
    ));
    receive(receiver)
}

pub fn save_failed(account: &str) -> bool {
    let pending = worker().pending.lock().unwrap();
    pending.failed.contains(account) || pending.blocked.contains(account)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "worker/remote_review_tests.rs"]
mod remote_review_tests;

#[cfg(test)]
#[path = "worker/schedule_tests.rs"]
mod schedule_tests;

#[cfg(test)]
#[path = "worker/rejected_schedule_tests.rs"]
mod rejected_schedule_tests;

#[cfg(test)]
#[path = "worker/dispatch_stack_tests.rs"]
mod dispatch_stack_tests;

#[cfg(test)]
#[path = "worker/slash_journal_tests.rs"]
mod slash_journal_tests;

#[path = "worker/upgrade.rs"]
mod upgrade;
#[cfg(test)]
pub(in crate::drafts::store) use upgrade::TestWorker as UpgradeTestWorker;
pub(in crate::drafts::store) use upgrade::UpgradeFence;
pub(crate) mod owned_queue;

#[cfg(test)]
#[path = "worker/terminal_tests.rs"]
mod terminal_tests;
