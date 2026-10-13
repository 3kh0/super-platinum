//! Route managed local edits immediately; poll only cheap persistence metadata.
use super::{
    Account, Fault, View,
    checkpoint::{Editor, Queue, Snapshot},
};
use crate::state::ShellState;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use super_platinum_core::drafts::{DraftKey, LocalDraft, store};

pub(crate) struct Desktop {
    pub view_id: u64,
    pub writers: Queue,
    pub sending: BTreeSet<DraftKey>,
    scope: Option<(String, u64)>,
    signatures: BTreeMap<DraftKey, Signature>,
}
impl Default for Desktop {
    fn default() -> Self {
        static VISITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            view_id: VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            writers: Default::default(),
            sending: Default::default(),
            scope: None,
            signatures: Default::default(),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Signature {
    serial: u64,
    revision: Option<u64>,
    error: Option<Fault>,
    unsaved: bool,
    running: bool,
    blocked: Option<String>,
}
impl From<&Snapshot> for Signature {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            serial: snapshot.serial,
            revision: snapshot.view.stored.as_ref().map(|stored| stored.revision),
            error: snapshot.error,
            unsaved: snapshot.unsaved,
            running: snapshot.running,
            blocked: snapshot
                .view
                .blocked
                .as_ref()
                .map(|blocked| blocked.token.clone()),
        }
    }
}

impl Desktop {
    pub fn begin_send(&mut self, account: &str, epoch: u64, key: DraftKey) -> bool {
        self.scope(account, epoch);
        self.sending.insert(key)
    }

    pub fn finish_send(&mut self, account: &str, epoch: u64, key: &DraftKey) {
        if self
            .scope
            .as_ref()
            .is_some_and(|(owner, visit)| owner == account && *visit == epoch)
        {
            self.sending.remove(key);
        }
    }

    fn scope(&mut self, account: &str, epoch: u64) {
        let scope = (account.to_owned(), epoch);
        if self.scope.as_ref() != Some(&scope) {
            self.scope = Some(scope);
            self.signatures.clear();
            self.sending.clear();
        }
    }

    pub fn editor(
        &mut self,
        account: &Arc<Account>,
        epoch: u64,
        key: DraftKey,
        loaded: View,
    ) -> Result<Arc<Editor>, Fault> {
        self.scope(account.backend.account(), epoch);
        self.writers.editor(account, epoch, key, loaded)
    }

    /// Called after explicit release/settlement has serialized with the owner.
    /// Dropping this reference never cancels a detached write already started.
    pub fn remove(&mut self, account: &str, epoch: u64, key: &DraftKey) {
        if self
            .scope
            .as_ref()
            .is_some_and(|(owner, visit)| owner == account && *visit == epoch)
        {
            self.writers.remove(account, epoch, key);
            self.signatures.remove(key);
            self.sending.remove(key);
        }
    }
}

/// Legacy send/upload/schedule paths must not consume content owned by the
/// paired managed store. An unknown startup state is never proof of no owner.
pub(crate) fn legacy_allowed(shell: &ShellState, key: &DraftKey) -> Result<(), &'static str> {
    if crate::fixture::is_fixture() {
        return Ok(());
    }
    legacy_ownership(shell, key)
}

fn legacy_ownership(shell: &ShellState, key: &DraftKey) -> Result<(), &'static str> {
    let account =
        shell.core.active_account.as_deref().ok_or(
            "Saved draft ownership is unavailable. Reconnect before sending or scheduling.",
        )?;
    let epoch = shell.core.account_epoch;
    if !shell.managed_drafts.ready_for(account, epoch) {
        return Err(
            "Saved draft status is still loading or unavailable. Refresh drafts before sending or scheduling.",
        );
    }
    if shell
        .draft_writes
        .scope
        .as_ref()
        .is_some_and(|(owner, visit)| owner == account && *visit == epoch)
        && shell.draft_writes.sending.contains(key)
    {
        return Err("This draft already has a send being prepared.");
    }
    if shell.managed_drafts.local_review_pending(account, key) {
        return Err(
            "Saved local versions need review in Drafts before sending or scheduling. Your current draft is still here.",
        );
    }
    if shell
        .managed_drafts
        .entry(account, key)
        .is_some_and(|view| view.stored.is_some())
        || shell
            .draft_writes
            .writers
            .get(account, epoch, key)
            .is_some_and(|editor| editor.snapshot().view.stored.is_some())
    {
        return Err(
            "This draft has saved delivery state. Resolve it in Drafts before scheduling or uploading files.",
        );
    }
    Ok(())
}

pub(crate) fn route(shell: &mut ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    if changes.is_empty() || crate::fixture::is_fixture() {
        return;
    }
    let Some(account_id) = shell.core.active_account.clone() else {
        return;
    };
    let epoch = shell.core.account_epoch;
    shell.draft_writes.scope(&account_id, epoch);
    let changes = route_existing(shell, changes);
    let known = shell.managed_drafts.account.as_deref() == Some(&account_id)
        && shell.managed_drafts.epoch == epoch
        && changes
            .keys()
            .any(|key| shell.managed_drafts.entry(&account_id, key).is_some());
    let legacy = if known {
        match Account::production(&account_id) {
            Ok(account) => route_managed(shell, &account, changes),
            Err(_) => changes,
        }
    } else {
        changes
    };
    // Unknown ownership must use the existing worker: its managed-row check
    // stores a blocked alternate rather than overwriting enrolled state.
    if !legacy.is_empty() {
        store::enqueue(account_id, legacy);
    }
}

fn route_managed(
    shell: &mut ShellState,
    account: &Arc<Account>,
    changes: BTreeMap<DraftKey, LocalDraft>,
) -> BTreeMap<DraftKey, LocalDraft> {
    let changes = route_existing(shell, changes);
    let account_id = account.backend.account();
    let epoch = shell.core.account_epoch;
    let current = shell.core.active_account.as_deref() == Some(account_id)
        && shell.managed_drafts.account.as_deref() == Some(account_id)
        && shell.managed_drafts.epoch == epoch;
    let mut legacy = BTreeMap::new();
    for (key, local) in changes {
        let view = current
            .then(|| shell.managed_drafts.entry(account_id, &key))
            .flatten()
            .cloned();
        if let Some(view) = view
            && let Ok(editor) = shell.draft_writes.editor(account, epoch, key.clone(), view)
        {
            editor.submit(local);
        } else {
            legacy.insert(key, local);
        }
    }
    legacy
}

fn route_existing(
    shell: &ShellState,
    changes: BTreeMap<DraftKey, LocalDraft>,
) -> BTreeMap<DraftKey, LocalDraft> {
    let Some(account) = shell.core.active_account.as_deref() else {
        return changes;
    };
    changes
        .into_iter()
        .filter_map(|(key, local)| {
            if let Some(editor) =
                shell
                    .draft_writes
                    .writers
                    .get(account, shell.core.account_epoch, &key)
            {
                editor.submit(local);
                None
            } else {
                Some((key, local))
            }
        })
        .collect()
}

pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let Some(account) = shell.core.active_account.clone() else {
        return false;
    };
    let epoch = shell.core.account_epoch;
    shell.draft_writes.scope(&account, epoch);
    if shell.managed_drafts.account.as_deref() != Some(&account)
        || shell.managed_drafts.epoch != epoch
    {
        return false;
    }
    let snapshots = shell.draft_writes.writers.snapshots(&account, epoch);
    let mut changed = false;
    for (key, snapshot) in snapshots {
        if snapshot.view.stored.is_none()
            && !snapshot.running
            && !snapshot.unsaved
            && snapshot.error.is_none()
        {
            // Released submissions are enqueued synchronously. Once their
            // observer is idle there is no delayed writer left to retain.
            shell.draft_writes.writers.remove(&account, epoch, &key);
            shell.draft_writes.signatures.remove(&key);
            // Keep any foreground send guard until its owner finishes, and
            // never remove newer managed ownership for this destination.
            let identity = (account.clone(), key);
            if let Some(view) = shell.managed_drafts.entries.get_mut(&identity)
                && view.stored.is_none()
            {
                // This writer only proves its own saved value. Independent
                // refused versions and newer input still require review.
                if view
                    .uncommitted
                    .as_ref()
                    .is_some_and(|local| super::same_local(local, &snapshot.view.local))
                {
                    view.uncommitted = None;
                    changed = true;
                }
                if view.blocked.is_none() && view.uncommitted.is_none() {
                    shell.managed_drafts.entries.remove(&identity);
                    changed = true;
                }
            }
            continue;
        }
        let signature = Signature::from(&snapshot);
        if shell.draft_writes.signatures.get(&key) == Some(&signature) {
            continue;
        }
        shell.draft_writes.signatures.insert(key.clone(), signature);
        // A released writer has no managed ownership to recreate. Its failed
        // local input still needs an account-scoped recovery row, however.
        if snapshot.view.stored.is_none() {
            if snapshot.error.is_some()
                && let Some(pending) = snapshot.pending
            {
                changed |= shell.managed_drafts.apply_local_recovery(
                    &account,
                    epoch,
                    key.clone(),
                    pending,
                );
                if let Some(blocked) = snapshot.view.blocked
                    && let Some(view) = shell
                        .managed_drafts
                        .entries
                        .get_mut(&(account.clone(), key))
                    && view.stored.is_none()
                    && view.blocked.is_none()
                {
                    // The first refused version may differ from the newest
                    // unsaved input. Show both without replacing older evidence.
                    view.blocked = Some(blocked);
                    changed = true;
                }
            }
            continue;
        }
        let mut view = snapshot.view;
        if let Some(current) = shell.managed_drafts.entry(&account, &key)
            && current.stored.as_ref().map(|stored| stored.revision)
                > view.stored.as_ref().map(|stored| stored.revision)
        {
            // Another observer may already have advanced metadata. Keep it,
            // while still exposing this editor's retained pending input.
            view = current.clone();
        }
        // Metadata alone: the visible composer and DraftBook may already hold
        // newer input. Even a failed write must never replace either of them.
        if let Some(pending) = snapshot.pending {
            view.uncommitted = Some(pending);
        }
        changed |= shell.managed_drafts.apply_view(&account, epoch, key, view);
    }
    changed
}

#[cfg(test)]
#[path = "integration_tests.rs"]
mod tests;
