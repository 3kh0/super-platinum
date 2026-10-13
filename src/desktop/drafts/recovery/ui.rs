use super::*;
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::slack::models::Message;

pub(crate) async fn check(
    mut state: Signal<ShellState>,
    op: Operation,
) -> Result<&'static str, CheckError> {
    let (owner, binding, epoch, editor) = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        if shell.core.active_account.as_deref() != Some(&op.scope.account_id) {
            return Err(Fault::Stale.into());
        }
        let binding =
            Binding::capture(&shell, &op.scope.key.team).map_err(|_| Fault::Unsupported)?;
        let account = Account::production(&op.scope.account_id)?;
        let editor = shell.draft_writes.writers.get(
            &op.scope.account_id,
            shell.core.account_epoch,
            &op.scope.key,
        );
        (
            account.owner(op.scope.key.clone()),
            binding,
            shell.core.account_epoch,
            editor,
        )
    };
    owner.reserve_check(&op, now_ms()).await?;
    if !state.try_peek().is_ok_and(|shell| binding.current(&shell)) {
        return Err(Fault::Stale.into());
    }
    // The caller uses spawn_forever: receipt storage is independent of the row.
    let checked = owner
        .finish_check(op.clone(), binding.clone(), now_ms())
        .await?;
    let released = if checked.confirmed {
        if let Some(editor) = editor {
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
    let text = if checked.confirmed {
        "Delivery confirmed."
    } else if checked.read.cooldown_until_ms.is_some() {
        "Slack asked us to wait. Try Check delivery again after the cooldown."
    } else {
        "Delivery is still unconfirmed. This check did not find the message in the available history. Nothing was sent again."
    };
    if let Ok(mut shell) = state.try_write()
        && binding.current(&shell)
    {
        if checked.confirmed {
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
            shell.managed_drafts.apply_view(
                &op.scope.account_id,
                epoch,
                op.scope.key.clone(),
                checked.view,
            );
        }
        shell.show_toast(text);
    }
    Ok(text)
}
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
