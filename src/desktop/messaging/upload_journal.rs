//! Durable recovery is a receipt journal, never an upload retry queue.
use super::{ShellState, upload_recovery};
use dioxus::prelude::*;
use std::sync::{Arc, Mutex};
use super_platinum_core::{
    domain::{ComposerAttachment, PendingFileMessage, PendingUploadReceipt},
    drafts::store::{
        DraftStore,
        uploads::{self, Draft, Stored},
    },
    error::AppError,
};
pub(crate) mod check;
pub(crate) mod fixture;
pub(super) mod network;
pub(super) use network::{Owner, reserve};
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(crate) struct State {
    rows: Vec<Stored>,
    revision: u64,
    load_serial: u64,
    backend: Backend,
}
#[derive(Clone, Default)]
enum Backend {
    #[default]
    Disk,
    Memory(Arc<Mutex<DraftStore>>),
    IsolatedDisk(std::path::PathBuf),
}
impl State {
    pub(crate) fn isolated_disk(path: std::path::PathBuf) -> Result<Self, AppError> {
        DraftStore::open(&path)?;
        Ok(Self {
            backend: Backend::IsolatedDisk(path),
            ..Default::default()
        })
    }
    /// Disconnected harness only; no configured account DB or keychain access.
    pub(crate) fn isolated() -> Self {
        Self {
            backend: Backend::Memory(Arc::new(Mutex::new(
                DraftStore::open(":memory:").expect("isolated journal"),
            ))),
            ..Default::default()
        }
    }
}
impl Backend {
    fn reserve(&self, draft: Draft) -> Result<uploads::Reservation, AppError> {
        match self {
            Self::Disk => crate::drafts::protected::owned_upload::reserve(draft),
            Self::IsolatedDisk(path) => DraftStore::open(path)?.reserve_upload(&draft),
            Self::Memory(db) => db.lock().unwrap().reserve_upload(&draft),
        }
    }
    fn finalize(
        &self,
        ticket: uploads::Ticket,
        ids: Vec<String>,
    ) -> Result<Option<Stored>, AppError> {
        match self {
            Self::Disk => crate::drafts::protected::owned_upload::finalize(ticket, ids),
            Self::IsolatedDisk(path) => DraftStore::open(path)?.finalize_upload(&ticket, ids),
            Self::Memory(db) => db.lock().unwrap().finalize_upload(&ticket, ids),
        }
    }
    fn retire(&self, row: Stored) -> Result<bool, AppError> {
        match self {
            Self::Disk => crate::drafts::protected::owned_upload::retire(row),
            Self::IsolatedDisk(path) => DraftStore::open(path)?.retire_upload(&row),
            Self::Memory(db) => db.lock().unwrap().retire_upload(&row),
        }
    }
    fn load(&self, account: &str) -> Result<Vec<Stored>, AppError> {
        match self {
            Self::Disk => crate::drafts::protected::owned_upload::load(account),
            Self::IsolatedDisk(path) => DraftStore::open(path)?.load_uploads(account),
            Self::Memory(db) => db.lock().unwrap().load_uploads(account),
        }
    }
}
fn owned(shell: &ShellState, row: &Stored) -> bool {
    shell.core.active_account.as_deref() == Some(row.draft.account_id.as_str())
        && shell
            .core
            .workspaces
            .get(&row.draft.team_id)
            .is_some_and(|w| w.self_user_id == row.draft.user_id)
}
fn remember(shell: &mut ShellState, row: Stored) {
    if !owned(shell, &row) {
        return;
    }
    shell
        .upload_journal
        .rows
        .retain(|r| r.draft.account_id == row.draft.account_id && r.ticket != row.ticket);
    shell.upload_journal.rows.push(row);
    shell.upload_journal.revision = shell.upload_journal.revision.wrapping_add(1);
}
/// Pure recovery reducer. Call only with an admitted account load; never send,
/// queue, restore a filesystem path, or overwrite a currently active upload.
pub(crate) fn restore(shell: &mut ShellState, account: &str, rows: Vec<Stored>) {
    if shell.core.active_account.as_deref() != Some(account) {
        return;
    }
    for row in rows.into_iter().take(uploads::MAX_ACTIVE) {
        if row.draft.validate().is_err() || row.draft.account_id != account || !owned(shell, &row) {
            continue;
        }
        let d = &row.draft;
        if shell
            .core
            .pending_file_messages
            .iter()
            .any(|p| p.client_msg_id == d.client_msg_id)
        {
            continue;
        }
        let mut message = super_platinum_core::slack::models::Message {
            kind: Some("message".into()),
            user: Some(d.user_id.clone()),
            channel: Some(d.channel_id.clone()),
            ts: Some(d.optimistic_ts.clone()),
            client_msg_id: Some(d.client_msg_id.clone()),
            text: Some(d.caption.clone()),
            thread_ts: d.thread_ts.clone(),
            ..Default::default()
        };
        if d.reply_broadcast {
            message.subtype = Some("thread_broadcast".into());
        }
        // A stale cache cannot overwrite an existing canonical row at this id.
        let bag = if let Some(root) = &d.thread_ts {
            shell
                .core
                .threads
                .entry((d.team_id.clone(), d.channel_id.clone(), root.clone()))
                .or_default()
        } else {
            shell
                .core
                .workspaces
                .get_mut(&d.team_id)
                .unwrap()
                .messages
                .entry(d.channel_id.clone())
                .or_default()
        };
        if bag.was_deleted(&d.optimistic_ts)
            || bag.messages.iter().any(|m| {
                m.ts.as_ref() == Some(&d.optimistic_ts)
                    && m.client_msg_id.as_ref() != Some(&d.client_msg_id)
            })
        {
            continue;
        }
        bag.upsert(message);
        if !bag.pending.contains(&d.optimistic_ts) {
            bag.pending.push(d.optimistic_ts.clone());
        }
        shell.core.pending_file_messages.push(PendingFileMessage {
            finalized: row.file_ids.clone().map(|file_ids| PendingUploadReceipt {
                account: Some(account.into()),
                sender: d.user_id.clone(),
                file_ids,
            }),
            team: d.team_id.clone(),
            channel: d.channel_id.clone(),
            thread_ts: d.thread_ts.clone(),
            reply_broadcast: d.reply_broadcast,
            message_ts: d.optimistic_ts.clone(),
            client_msg_id: d.client_msg_id.clone(),
            text: d.caption.clone(),
            attachments: d
                .attachments
                .iter()
                .enumerate()
                .map(|(i, a)| ComposerAttachment {
                    id: i as u64,
                    path: Default::default(),
                    name: a.name.clone(),
                    bytes: a.bytes,
                    uploading: false,
                    upload_started: None,
                    upload_cancel: None,
                    upload_progress: None,
                    preview_path: None,
                })
                .collect(),
        });
        shell.pending_delivery.insert(
            d.optimistic_ts.clone(),
            crate::state::PendingDeliveryStatus::Unconfirmed,
        );
        let (team, channel) = (d.team_id.clone(), d.channel_id.clone());
        remember(shell, row);
        upload_recovery::reconcile(shell, &team, &channel, None);
    }
    shell.refresh_from_core();
}
/// Root-owned background load. Any intervening journal mutation retires the
/// load's authority, so a stale disk snapshot cannot resurrect confirmed rows.
pub(crate) fn load(state: Signal<ShellState>) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
        && matches!(state.peek().upload_journal.backend, Backend::Disk)
    {
        return;
    }
    start_load(state);
}
#[derive(Debug, PartialEq, Eq)]
enum LoadDecision {
    Applied,
    Recheck,
    Retired,
}
fn apply_load(
    shell: &mut ShellState,
    owner: &Owner,
    revision: u64,
    serial: u64,
    rows: Vec<Stored>,
) -> LoadDecision {
    if !owner.current(shell) || shell.upload_journal.load_serial != serial {
        return LoadDecision::Retired;
    }
    if shell.upload_journal.revision != revision {
        return LoadDecision::Recheck;
    }
    restore(shell, &owner.account, rows);
    LoadDecision::Applied
}
#[cfg(test)]
fn accepts_load(shell: &ShellState, owner: &Owner, revision: u64, serial: u64) -> bool {
    owner.current(shell)
        && shell.upload_journal.revision == revision
        && shell.upload_journal.load_serial == serial
}
fn start_load(mut state: Signal<ShellState>) {
    let Some(owner) = Owner::capture(&state.peek(), None) else {
        return;
    };
    let (backend, mut revision, serial) = {
        let mut shell = state.write();
        let s = &mut shell.upload_journal;
        s.load_serial = s.load_serial.wrapping_add(1);
        (s.backend.clone(), s.revision, s.load_serial)
    };
    dioxus::core::spawn_forever(async move {
        loop {
            let account = owner.account.clone();
            let disk = backend.clone();
            let result = tokio::task::spawn_blocking(move || disk.load(&account)).await;
            let decision = {
                let mut shell = state.write();
                match result {
                    Ok(Ok(rows)) => apply_load(&mut shell, &owner, revision, serial, rows),
                    _ => {
                        if owner.current(&shell) && shell.upload_journal.load_serial == serial {
                            shell.show_toast("Saved upload recovery could not be loaded");
                        }
                        // Storage failures never turn into automatic retries,
                        // even when a concurrent mutation also retired the read.
                        return;
                    }
                }
            };
            if decision != LoadDecision::Recheck {
                return;
            }
            // Keep one task and one disk read in flight. Repeated send/settlement
            // activity may retire multiple snapshots; after it quiets, unrelated
            // older receipts must still load. There is no network or idle timer.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let shell = state.peek();
            if !owner.current(&shell) || shell.upload_journal.load_serial != serial {
                return;
            }
            revision = shell.upload_journal.revision;
        }
    });
}
/// Canonical evidence already checked by upload_recovery. Disk retirement is
/// exact and asynchronous; failure preserves the journal for a later read.
pub(super) fn confirmed(shell: &mut ShellState, client: &str) {
    let Some(index) = shell
        .upload_journal
        .rows
        .iter()
        .position(|r| r.draft.client_msg_id == client && owned(shell, r))
    else {
        return;
    };
    let row = shell.upload_journal.rows.remove(index);
    shell.upload_journal.revision = shell.upload_journal.revision.wrapping_add(1);
    let backend = shell.upload_journal.backend.clone();
    #[cfg(test)]
    {
        let _ = backend.retire(row);
    }
    #[cfg(not(test))]
    dioxus::core::spawn_forever(async move {
        let _ = tokio::task::spawn_blocking(move || backend.retire(row)).await;
    });
}
