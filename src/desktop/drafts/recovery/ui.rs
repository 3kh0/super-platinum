use super::*;
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::slack::models::Message;
#[derive(Clone)]
enum Source {
    Network(Binding),
    Fixture(Arc<update_fixture::FixtureBinding>),
}
impl Source {
    fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Network(b) => b.current(shell),
            Self::Fixture(b) => b.current(shell),
        }
    }
    async fn read(&self, op: &Operation, now: u64) -> Readback {
        match self {
            Self::Network(b) => b.check_operation(op, now).await,
            Self::Fixture(b) => b.read(op, now).await,
        }
    }
}

pub(crate) async fn check(
    mut state: Signal<ShellState>,
    op: Operation,
    mut publish_inline: impl FnMut(&'static str) -> bool,
) -> Result<&'static str, CheckError> {
    let (owner, binding, epoch, editor) = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        if shell.core.active_account.as_deref() != Some(&op.scope.account_id) {
            return Err(Fault::Stale.into());
        }
        let (owner, binding) = if let Some(binding) = update_fixture::binding(&shell) {
            (binding.owner(&op.scope.key), Source::Fixture(binding))
        } else {
            let binding =
                Binding::capture(&shell, &op.scope.key.team).map_err(|_| Fault::Unsupported)?;
            (
                Account::production(&op.scope.account_id)?.owner(op.scope.key.clone()),
                Source::Network(binding),
            )
        };
        let editor = shell.draft_writes.writers.get(
            &op.scope.account_id,
            shell.core.account_epoch,
            &op.scope.key,
        );
        (owner, binding, shell.core.account_epoch, editor)
    };
    // The caller uses spawn_forever: receipt storage is independent of the row.
    // A retained completion retries SQLite only; it never repeats the form read.
    let mut checked = if let Some(checked) = owner.retry_check_commit(&op).await? {
        checked
    } else {
        let ticket = owner.reserve_check(&op, now_ms()).await?;
        owner
            .finish_check_with(op.clone(), ticket, || async {
                if !state.try_peek().is_ok_and(|shell| binding.current(&shell)) {
                    return Readback::blocked(network::Block::Session);
                }
                binding.read(&op, now_ms()).await
            })
            .await?
    };
    let update = op.kind == OperationKind::Update;
    if update && let Some(editor) = editor.as_ref() {
        if let Ok(snapshot) = editor.refresh_receipt().await {
            checked.view = snapshot.view;
            if let Some(pending) = snapshot.pending {
                checked.view.uncommitted = Some(pending);
            }
        } else if let Some(pending) = editor.snapshot().pending {
            checked.view.uncommitted = Some(pending);
        }
    }
    let released = if checked.confirmed && !update {
        if let Some(editor) = editor.as_ref() {
            editor.release().await.ok()
        } else if let Some(stored) = &checked.view.stored {
            owner
                .release(stored.revision)
                .await
                .ok()
                .map(|_| stored.revision)
        } else {
            None
        }
    } else {
        None
    };
    let text = if update && checked.confirmed {
        "Draft update confirmed. Your current text was kept."
    } else if update && checked.read.cooldown_until_ms.is_some() {
        "Slack asked us to wait. The saved cooldown applies before checking the update again."
    } else if update {
        "The draft update is still unconfirmed. Nothing was written again; your current text was kept."
    } else if checked.confirmed {
        "Delivery confirmed."
    } else if checked.read.cooldown_until_ms.is_some() {
        "Slack asked us to wait. Try Check delivery again after the cooldown."
    } else {
        "Delivery is still unconfirmed. This check did not find the message in the available history. Nothing was sent again."
    };
    if let Ok(mut shell) = state.try_write()
        && binding.current(&shell)
    {
        if update {
            // A receipt task may resume after another submit or writer handoff.
            // Re-read the live writer synchronously at the projection boundary.
            let current =
                shell
                    .draft_writes
                    .writers
                    .get(&op.scope.account_id, epoch, &op.scope.key);
            if !session::same_transport(&editor, &current) {
                update_fixture::ui_completed(&op.scope);
                return Ok(text);
            }
            if let Some(editor) = editor.as_ref() {
                checked.view.uncommitted = editor.snapshot().pending;
            }
        }
        if checked.confirmed && !update {
            reconcile(&mut shell, &op, &checked.read);
        }
        let current = shell
            .managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .and_then(|view| view.stored.as_ref())
            .map(|stored| stored.revision);
        if let (Some(released), Some(current)) = (released, current)
            && current <= released
        {
            shell
                .managed_drafts
                .remove_entry(&op.scope.account_id, epoch, &op.scope.key, current);
        } else {
            // A receipt is metadata, not authority to discard locally retained
            // alternatives or input submitted after its storage snapshot.
            if update
                && let Some(current) = shell
                    .managed_drafts
                    .entry(&op.scope.account_id, &op.scope.key)
            {
                if checked.view.uncommitted.is_none() {
                    checked.view.uncommitted = current.uncommitted.clone();
                }
                if checked.view.blocked.is_none() {
                    checked.view.blocked = current.blocked.clone();
                }
            }
            shell.managed_drafts.apply_view(
                &op.scope.account_id,
                epoch,
                op.scope.key.clone(),
                checked.view,
            );
        }
        if !inline_update_feedback(
            &shell,
            &op,
            epoch,
            checked.confirmed,
            text,
            &mut publish_inline,
        ) {
            shell.show_toast(text);
        }
    }
    update_fixture::ui_completed(&op.scope);
    Ok(text)
}
// Match RecoveryRows' owner/team projection and the Drafts surface that mounts
// it. The originating component must also accept the feedback synchronously:
// navigating away and back can mount a new row for the same operation.
fn inline_update_feedback(
    shell: &ShellState,
    op: &Operation,
    epoch: u64,
    confirmed: bool,
    text: &'static str,
    publish: &mut impl FnMut(&'static str) -> bool,
) -> bool {
    !confirmed
        && op.kind == OperationKind::Update
        && shell.core.active_account.as_ref() == Some(&op.scope.account_id)
        && shell.core.account_epoch == epoch
        && shell.managed_drafts.account.as_ref() == Some(&op.scope.account_id)
        && shell.managed_drafts.epoch == epoch
        && shell.core.active_team.as_ref() == Some(&op.scope.key.team)
        && shell.main_view == crate::state::MainView::Scheduled
        && shell.core.scheduled.filter == super_platinum_core::scheduled::DraftFilter::Drafts
        && shell.thread_root.is_none()
        && shell.overlay.is_none()
        && shell.profile_hover.is_none()
        && !crate::read_shortcuts::bulk::dialog_open(shell)
        && shell
            .managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .and_then(operation)
            .as_ref()
            == Some(op)
        && publish(text)
}
#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The reader already proved sender, destination, thread and exact client UUID.
/// Only replace a still-pending optimistic row, never a realtime edit/deletion.
fn reconcile(shell: &mut ShellState, op: &Operation, read: &Readback) {
    if op.scope.key.thread.is_some() {
        return;
    }
    let Some(Outcome::Sent { ts, .. }) = &read.outcome else {
        return;
    };
    let Some(workspace) = shell.core.workspaces.get_mut(&op.scope.key.team) else {
        return;
    };
    let Some(messages) = workspace.messages.get_mut(&op.scope.key.channel) else {
        return;
    };
    let optimistic: Vec<_> = messages
        .messages
        .iter()
        .filter(|message| message.client_msg_id.as_deref() == Some(&op.client_msg_id))
        .filter_map(|message| message.ts.clone())
        .filter(|ts| messages.pending.contains(ts))
        .collect();
    if optimistic.len() != 1 {
        return;
    }
    let optimistic = &optimistic[0];
    if messages.was_deleted(ts)
        || messages
            .messages
            .iter()
            .any(|message| message.ts.as_ref() == Some(ts))
    {
        messages.remove(optimistic);
    } else {
        let message = read
            .raw
            .as_ref()
            .and_then(|raw| raw.get("messages"))
            .and_then(serde_json::Value::as_array)
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|m| m.get("ts").and_then(serde_json::Value::as_str) == Some(ts))
            })
            .and_then(|raw| serde_json::from_value::<Message>(raw.clone()).ok());
        let Some(message) = message else {
            return;
        };
        messages.confirm(&op.client_msg_id, message);
    }
    shell.pending_delivery.remove(optimistic);
    shell.refresh_from_core();
}

/// Explicit return to ordinary local editing. No Slack request is involved.
pub(crate) async fn continue_locally(
    mut state: Signal<ShellState>,
    expected_account: String,
    expected_epoch: u64,
    key: DraftKey,
    expected: u64,
) -> Result<(), Fault> {
    let (account_id, epoch, transport, visit, owner, editor) = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        let account_id = shell.core.active_account.clone().ok_or(Fault::Stale)?;
        if account_id != expected_account || shell.core.account_epoch != expected_epoch {
            return Err(Fault::Stale);
        }
        let view = shell
            .managed_drafts
            .entry(&account_id, &key)
            .ok_or(Fault::Unmanaged)?;
        if !clean_local(view)
            || shell
                .managed_drafts
                .local_alternates
                .get(&(account_id.clone(), key.clone()))
                .is_some_and(|versions| !versions.is_empty())
            || view.stored.as_ref().map(|stored| stored.revision) != Some(expected)
        {
            return Err(Fault::Stale);
        }
        let account = settlement::account(&account_id)?;
        let editor = shell
            .draft_writes
            .writers
            .get(&account_id, shell.core.account_epoch, &key);
        (
            account_id,
            shell.core.account_epoch,
            shell.core.transport.clone(),
            shell.draft_writes.view_id,
            account.owner(key.clone()),
            editor,
        )
    };
    let released = if let Some(editor) = editor {
        if editor.waiting_for_return() {
            let (serial, restored) = editor.review_return(expected).await?;
            let mut shell = state.try_write().map_err(|_| Fault::Stale)?;
            if shell.core.active_account.as_ref() != Some(&account_id)
                || shell.core.account_epoch != epoch
                || shell.draft_writes.view_id != visit
                || !session::same_transport(&transport, &shell.core.transport)
                || editor.snapshot().serial != serial
                || shell.managed_drafts.local_review_pending(&account_id, &key)
                || !return_draft::project_return(&mut shell, &key, &restored.local)
                || !editor.finish_return(serial)
            {
                return Err(Fault::Stale);
            }
        }
        editor.release().await?
    } else {
        owner.release(expected).await?;
        expected
    };
    if let Ok(mut shell) = state.try_write()
        && shell.core.active_account.as_deref() == Some(&account_id)
        && shell.core.account_epoch == epoch
        && shell.draft_writes.view_id == visit
        && super::super::session::same_transport(&transport, &shell.core.transport)
    {
        let current = shell
            .managed_drafts
            .entry(&account_id, &key)
            .and_then(|view| view.stored.as_ref())
            .map(|stored| stored.revision);
        if let Some(current) = current.filter(|current| *current <= released) {
            shell
                .managed_drafts
                .remove_entry(&account_id, epoch, &key, current);
        }
        shell.show_toast("This draft is ready for local editing. Nothing was sent.");
    }
    Ok(())
}
