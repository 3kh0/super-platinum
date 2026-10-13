//! Explicit local restoration of a definitely rejected direct send. No HTTP.
use super::*;
use crate::drafts::Owner;
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use super_platinum_core::drafts::sync::{ConsumeIntent, ConsumeKind};

pub(crate) fn rejected(view: &View) -> Option<ConsumeIntent> {
    if view.blocked.is_some() || view.uncommitted.is_some() {
        return None;
    }
    let stored = view.stored.as_ref()?;
    let Workflow::Rejected(op) = stored.state.workflow() else {
        return None;
    };
    let intent = stored.state.consumption()?;
    (op.kind == OperationKind::Send
        && intent.kind == ConsumeKind::Send
        && op.consume_id.as_deref() == Some(intent.id.as_str())
        && op.scope == intent.scope
        && op.client_msg_id == intent.client_msg_id
        && op.snapshot.as_ref() == Some(&intent.snapshot)
        && op.base.is_none()
        && intent.base.is_none()
        && stored.state.acknowledged().is_none()
        && intent.scope.key.thread.is_none())
    .then(|| intent.clone())
}
impl Owner {
    pub async fn return_rejected(
        &self,
        expected: u64,
        intent: &ConsumeIntent,
    ) -> Result<View, Fault> {
        let _gate = self.gate.lock().await;
        let view = self.current().await?;
        let mut stored = view.ready(expected)?;
        if rejected(&view).as_ref() != Some(intent)
            || intent.scope != self.scope
            || !view.local.empty()
        {
            return Err(Fault::Stale);
        }
        let review = self
            .account
            .backend
            .read_local_review(self.scope.clone())
            .await?;
        if review.blocked.is_some() || !review.alternates.is_empty() {
            return Err(Fault::AlternateEdits);
        }
        let snapshot = stored
            .state
            .cancel_consumption(&self.scope, &intent.id)
            .map_err(|_| Fault::Pending)?;
        let local = LocalDraft {
            composer: codec::decode(&self.scope.key, &snapshot)
                .map_err(|_| Fault::Unsupported)?
                .composer()
                .clone(),
            attachments: vec![],
        };
        let stored = self
            .save(Some(expected), local.clone(), stored.state)
            .await?;
        Ok(View {
            stored: Some(stored),
            local,
            blocked: None,
            uncommitted: None,
        })
    }
}
pub(super) fn empty_live(shell: &ShellState, key: &DraftKey) -> bool {
    shell
        .core
        .drafts
        .items
        .get(key)
        .is_none_or(LocalDraft::empty)
        && (shell.composer_key(ComposerTarget::Channel).as_ref() != Some(key)
            || (shell.core.composer.text.is_empty() && shell.core.composer_attachments.is_empty()))
}

/// Scheduled renders no composer DOM. A synchronous Rust handoff here cannot
/// race keystrokes from a stale native field after an asynchronous replacement.
pub(super) fn project_return(shell: &mut ShellState, key: &DraftKey, local: &LocalDraft) -> bool {
    if shell.main_view != MainView::Scheduled
        || shell.core.active_team.as_ref() != Some(&key.team)
        || !empty_live(shell, key)
    {
        return false;
    }
    shell.core.drafts.items.insert(key.clone(), local.clone());
    if shell.composer_key(ComposerTarget::Channel).as_ref() == Some(key) {
        shell.core.composer = local.composer.clone();
        shell.core.composer_attachments = local.attachments.clone();
    }
    true
}

pub(crate) async fn return_to_draft(
    mut state: Signal<ShellState>,
    account: String,
    epoch: u64,
    key: DraftKey,
    expected: u64,
    intent: ConsumeIntent,
) -> Result<(), Fault> {
    let (transport, visit, editor, serial) = {
        let mut shell = state.try_write().map_err(|_| Fault::Stale)?;
        if shell.core.active_account.as_ref() != Some(&account)
            || shell.core.account_epoch != epoch
            || shell.core.active_team.as_ref() != Some(&key.team)
            || shell.main_view != MainView::Scheduled
            || shell.managed_drafts.local_review_pending(&account, &key)
            || !empty_live(&shell, &key)
        {
            return Err(Fault::Stale);
        }
        let view = shell
            .managed_drafts
            .entry(&account, &key)
            .cloned()
            .ok_or(Fault::Unmanaged)?;
        if view.stored.as_ref().map(|s| s.revision) != Some(expected)
            || rejected(&view).as_ref() != Some(&intent)
        {
            return Err(Fault::Stale);
        }
        let owner = settlement::account(&account)?;
        let editor = shell
            .draft_writes
            .editor(&owner, epoch, key.clone(), view)?;
        let snapshot = editor.snapshot();
        if snapshot.running || snapshot.pending.is_some() {
            return Err(Fault::Pending);
        }
        (
            shell.core.transport.clone(),
            shell.draft_writes.view_id,
            editor,
            snapshot.serial,
        )
    };
    let restored = editor
        .return_rejected(expected, intent.clone(), serial)
        .await?;
    let mut projected = false;
    if let Ok(mut shell) = state.try_write()
        && shell.core.active_account.as_ref() == Some(&account)
        && shell.core.account_epoch == epoch
        && shell.draft_writes.view_id == visit
        && session::same_transport(&transport, &shell.core.transport)
    {
        // Invalidate any pre-return refresh before it can reintroduce old metadata.
        shell.managed_drafts.projection_epoch =
            shell.managed_drafts.projection_epoch.wrapping_add(1);
        shell.managed_drafts.loading = false;
        shell
            .managed_drafts
            .apply_view(&account, epoch, key.clone(), restored.clone());
        if editor.snapshot().serial == serial && project_return(&mut shell, &key, &restored.local) {
            projected = editor.finish_return(serial);
        }
        if remove_failed_row(&mut shell, &intent, &restored.local) {
            shell.refresh_from_core();
        }
        shell.show_toast(if projected {
            "The unsent message is back in your drafts. Nothing was sent."
        } else {
            "The unsent message is saved in Drafts. Review it there before continuing."
        });
    }
    // Only after projection can later typing safely be treated as edits to the
    // restored draft. Input that arrived during restoration stays separate.
    if projected && let Ok(released) = editor.release().await {
        if let Ok(mut shell) = state.try_write()
            && shell.core.active_account.as_ref() == Some(&account)
            && shell.core.account_epoch == epoch
            && shell.draft_writes.view_id == visit
            && session::same_transport(&transport, &shell.core.transport)
        {
            shell
                .managed_drafts
                .remove_entry(&account, epoch, &key, released);
        }
    }
    Ok(())
}

fn remove_failed_row(
    shell: &mut ShellState,
    intent: &ConsumeIntent,
    restored: &LocalDraft,
) -> bool {
    use crate::state::PendingDeliveryStatus;
    let key = &intent.scope.key;
    if shell.core.active_account.as_ref() != Some(&intent.scope.account_id) {
        return false;
    }
    let Some(workspace) = shell.core.workspaces.get_mut(&key.team) else {
        return false;
    };
    let Some(messages) = workspace.messages.get_mut(&key.channel) else {
        return false;
    };
    let matches = messages
        .messages
        .iter()
        .filter(|message| {
            message.client_msg_id.as_deref() == Some(&intent.client_msg_id)
                && message.user.as_deref() == Some(&workspace.self_user_id)
                && message.edited.is_none()
                && message.blocks == intent.snapshot.blocks
                && message.text.as_deref() == Some(restored.composer.text.as_str())
                && message.ts.as_ref().is_some_and(|ts| {
                    messages.pending.contains(ts)
                        && shell.pending_delivery.get(ts) == Some(&PendingDeliveryStatus::Failed)
                })
        })
        .filter_map(|message| message.ts.clone())
        .collect::<Vec<_>>();
    // Ambiguous identity is evidence to retain, never permission to remove.
    if let [ts] = matches.as_slice() {
        messages.remove(ts);
        shell.pending_delivery.remove(ts);
        shell.message_arrivals.remove(ts);
        return true;
    }
    false
}

#[cfg(test)]
mod tests;
