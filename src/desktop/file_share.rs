//! Existing hosted-file Share. A distinct owner from message forwarding.
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::sync::Arc;
use super_platinum_core::{
    file_sharing::Receipt,
    slack::{
        Error, api,
        events::RtEvent,
        file_share_policy::{self as policy, Eligibility},
    },
};
mod admission;
pub(crate) mod check;
mod delivery;
mod draft_io;
mod drafts;
mod durable;
mod recipients;
pub(crate) use durable::close::{dialog as close_dialog, review_current as review_captured_note};
pub(crate) use durable::has_capture;
pub(crate) use durable::open::{retry as retry_open, start as open};
pub(crate) use durable::saved::{
    discard as discard_saved_note, review as open_saved_draft, review_local as open_captured_note,
};
mod fixture;
pub(crate) use fixture::Capability as FixtureCapability;
mod policy_owner;
pub(crate) mod pressure;
mod recovery;
mod retry;
pub(crate) use retry::{RetryState, retry_result, retry_status};
mod state;
pub(crate) use admission::destinations;
pub(crate) use delivery::start;
pub(crate) use recovery::{close_recovery, open_receipt, open_recovery, retained_status};
pub(crate) use state::{
    Destination, Dialog, Phase, Recovery, State, carry_pending, connection, event,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    Owner,
    Changed,
    Source,
    Destination,
    Note,
    Storage,
    Busy,
    Capacity,
    PolicyCapacity,
    Cooldown,
    Read,
}
impl Refusal {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Owner => "Reconnect to this workspace before sharing. Your note is retained.",
            Self::Changed => "This file or destination changed. Reopen Share before continuing.",
            Self::Source => "This file cannot be shared here.",
            Self::Destination => {
                "Choose an available channel or open direct message that allows new posts."
            }
            Self::Note => "The note could not be read. Your draft is still here.",
            Self::Storage => {
                "The share record could not be saved. Check saved file shares before trying again."
            }
            Self::Busy => "Another file share needs to finish saving first.",
            Self::Capacity => "Too many unresolved file shares are retained.",
            Self::PolicyCapacity => "File sharing checks are busy. Try again shortly.",
            Self::Cooldown => "Slack asked us to wait before sharing again. Your note is retained.",
            Self::Read => "File sharing could not be checked. Your note is retained.",
        }
    }
}
fn next_id() -> Result<String, Refusal> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_update(
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
        |n| n.checked_add(1),
    )
    .map(|n| format!("file-share-{n}"))
    .map_err(|_| Refusal::Capacity)
}
pub(crate) fn visible(shell: &ShellState) -> Option<&Dialog> {
    if !matches!(shell.overlay, Some(Overlay::FileShare))
        || !shell.file_share.context.as_ref()?.current(shell)
    {
        return None;
    }
    shell.file_share.dialog.as_ref()
}
pub(crate) fn recovery_visible(shell: &ShellState) -> Option<&Recovery> {
    if !matches!(shell.overlay, Some(Overlay::FileShare))
        || !shell.file_share.context.as_ref()?.current(shell)
        || !shell.file_share.recovery.open
    {
        return None;
    }
    Some(&shell.file_share.recovery)
}
pub(crate) fn can_open(shell: &ShellState, target: &crate::file_actions::Target) -> bool {
    (shell
        .file_share_fixture
        .as_ref()
        .is_some_and(|c| c.current(shell))
        || (shell.file_share_fixture.is_none()
            && shell.core.active_account.is_some()
            && shell.core.transport.is_some()
            && shell
                .core
                .session
                .as_ref()
                .is_some_and(|s| s.workspaces.contains_key(&target.team))))
        && shell.core.active_team.as_ref() == Some(&target.team)
        && shell.core.account_epoch == target.epoch
        && target
            .attachment
            .as_ref()
            .and_then(|g| g.canonical_file(shell))
            .is_some_and(|f| policy::source(f) == Eligibility::Allowed)
}
fn uncaptured(dialog: &Dialog) -> bool {
    dialog.closing
        || matches!(
            dialog.phase,
            Phase::Loading | Phase::Editing | Phase::Preparing
        ) && dialog.receipt.is_none()
}
pub(crate) fn choose(shell: &mut ShellState, id: &str, destination: &str) -> Result<(), Refusal> {
    let selected = admission::selected(shell, destination).ok_or(Refusal::Destination)?;
    let recipient = recipients::capture(shell, destination).ok_or(Refusal::Destination)?;
    let dialog = visible(shell)
        .filter(|d| d.id == id && d.phase == Phase::Editing && d.durable.is_some() && !d.closing)
        .ok_or(Refusal::Changed)?;
    if !dialog.admission.as_ref().is_some_and(|s| s.current(shell)) {
        return Err(Refusal::Changed);
    }
    let dialog = shell.file_share.dialog.as_mut().ok_or(Refusal::Changed)?;
    dialog.destination = Some(selected);
    shell.file_share.selected_recipient = Some((id.into(), destination.into(), recipient));
    shell.file_share.dialog.as_mut().unwrap().error = None;
    Ok(())
}
pub(crate) fn destination_current(shell: &ShellState, dialog: &Dialog) -> bool {
    dialog
        .destination
        .as_ref()
        .is_some_and(|d| recipients::chosen(shell, &dialog.id, &d.id))
}
pub(crate) fn close(shell: &mut ShellState, id: &str) {
    if visible(shell).is_some_and(|d| d.id == id) {
        if let Some(dialog) = &shell.file_share.dialog {
            durable::retire(dialog, &shell.file_share.custody);
        }
        shell.file_share.dialog = None;
        shell.overlay = None;
    }
}
/// Generic synchronous dismissal cannot read a live editor. Explicit UI Close
/// uses close_dialog. Refuse to discard an uncaptured editing draft here.
pub(crate) fn dismiss(shell: &mut ShellState) {
    if visible(shell).is_some_and(uncaptured) {
        return;
    }
    shell.file_share.dialog = None;
    shell.file_share.recovery.open = false;
    shell.overlay = None;
}
#[cfg(test)]
mod tests;
