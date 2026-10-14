//! Same-session canonical reconciliation. This module never sends or retries.
use super::{ShellState, ownership::UploadReadback};
use super_platinum_core::{
    domain::PendingUploadReceipt,
    slack::{events::RtEvent, models::Message},
    state::{ChannelMessages, cmp_ts},
};

/// Called only after the immutable send visit was checked by the result owner.
/// Reconcile once now as the realtime message may have preceded the share ACK.
pub(super) fn attach(
    shell: &mut ShellState,
    client_msg_id: &str,
    sender: &str,
    file_ids: Vec<String>,
) -> bool {
    if file_ids.is_empty()
        || file_ids.iter().any(String::is_empty)
        || file_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != file_ids.len()
    {
        return false;
    }
    let Some(pending) = shell
        .core
        .pending_file_messages
        .iter_mut()
        .find(|p| p.client_msg_id == client_msg_id)
    else {
        return false;
    };
    if !shell
        .core
        .workspaces
        .get(&pending.team)
        .is_some_and(|w| w.self_user_id == sender)
    {
        return false;
    }
    // Attaching a second conflicting identity must never replace the first.
    if pending.finalized.as_ref().is_some_and(|old| {
        old.account != shell.core.active_account || old.sender != sender || old.file_ids != file_ids
    }) {
        return false;
    }
    pending.finalized = Some(PendingUploadReceipt {
        account: shell.core.active_account.clone(),
        sender: sender.into(),
        file_ids,
    });
    let team = pending.team.clone();
    let channel = pending.channel.clone();
    reconcile(shell, &team, &channel, None);
    !shell
        .core
        .pending_file_messages
        .iter()
        .any(|p| p.client_msg_id == client_msg_id)
}

/// Restrict work to the destination and freshly ingested IDs. `None` is used
/// only once while attaching a receipt, never from a render or periodic tick.
pub(crate) fn reconcile(shell: &mut ShellState, team: &str, channel: &str, ids: Option<&[String]>) {
    if ids.is_some_and(<[String]>::is_empty) {
        return;
    }
    let mut confirmed = Vec::new();
    for pending in &shell.core.pending_file_messages {
        if pending.team != team || pending.channel != channel {
            continue;
        }
        let Some(receipt) = pending.finalized.as_ref() else {
            continue;
        };
        if receipt.account != shell.core.active_account
            || !shell
                .core
                .workspaces
                .get(team)
                .is_some_and(|w| w.self_user_id == receipt.sender)
        {
            continue;
        }
        let expected = UploadReadback {
            team: team.into(),
            channel: channel.into(),
            thread_ts: pending.thread_ts.clone(),
            reply_broadcast: pending.reply_broadcast,
            sender: receipt.sender.clone(),
            file_ids: receipt.file_ids.clone(),
        };
        let channel_bag = shell
            .core
            .workspaces
            .get(team)
            .and_then(|w| w.messages.get(channel));
        let thread_bag = pending.thread_ts.as_ref().and_then(|root| {
            shell
                .core
                .threads
                .get(&(team.into(), channel.into(), root.clone()))
        });
        let eligible = |message: &&Message| {
            let Some(ts) = message.ts.as_ref() else {
                return false;
            };
            expected.confirms(message)
                && !channel_bag.is_some_and(|bag| bag.was_deleted(ts))
                && !thread_bag.is_some_and(|bag| bag.was_deleted(ts))
        };
        let message = if let Some(ids) = ids {
            thread_bag
                .into_iter()
                .chain(channel_bag)
                .flat_map(|bag| ids.iter().filter_map(move |ts| lookup(bag, ts)))
                .find(eligible)
        } else {
            thread_bag
                .into_iter()
                .chain(channel_bag)
                .flat_map(|bag| bag.messages.iter())
                .find(eligible)
        };
        if let Some(message) = message {
            confirmed.push((
                pending.client_msg_id.clone(),
                pending.message_ts.clone(),
                pending.thread_ts.clone(),
                pending.reply_broadcast,
                message.clone(),
            ));
        }
    }
    for (client_id, optimistic, root, broadcast, message) in confirmed {
        super::upload_journal::confirmed(shell, &client_id);
        if broadcast || root.is_none() {
            let bag = shell
                .core
                .workspaces
                .get_mut(team)
                .unwrap()
                .messages
                .entry(channel.into())
                .or_default();
            insert_missing(bag, &message);
            bag.remove(&optimistic);
        }
        if let Some(root) = root {
            let bag = shell
                .core
                .threads
                .entry((team.into(), channel.into(), root))
                .or_default();
            insert_missing(bag, &message);
            bag.remove(&optimistic);
        }
        shell
            .core
            .pending_file_messages
            .retain(|p| p.client_msg_id != client_id);
        shell.pending_delivery.remove(&optimistic);
    }
}
/// History callers collect identities only when this destination has a receipt.
pub(crate) fn incoming(
    shell: &ShellState,
    team: &str,
    channel: &str,
    messages: &[Message],
) -> Vec<String> {
    if !has_receipt(shell, team, channel) {
        return Vec::new();
    }
    messages
        .iter()
        .filter_map(|message| message.ts.clone())
        .collect()
}

/// Capture before the realtime reducer consumes the event. A retired socket
/// must not trigger recovery from its payload or already loaded history.
pub(crate) fn event(
    shell: &ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<(String, String)> {
    if !shell
        .core
        .workspaces
        .get(team)
        .is_some_and(|w| w.rt_generation == generation)
    {
        return None;
    }
    let (channel, message) = match event {
        RtEvent::Message(message) => (message.channel.as_ref()?, message),
        RtEvent::MessageChanged {
            channel, message, ..
        } => (channel, message),
        _ => return None,
    };
    has_receipt(shell, team, channel).then(|| Some((channel.clone(), message.ts.clone()?)))?
}
fn has_receipt(shell: &ShellState, team: &str, channel: &str) -> bool {
    shell
        .core
        .pending_file_messages
        .iter()
        .any(|p| p.team == team && p.channel == channel && p.finalized.is_some())
}
fn lookup<'a>(bag: &'a ChannelMessages, ts: &str) -> Option<&'a Message> {
    let index = bag
        .messages
        .binary_search_by(|m| cmp_ts(m.ts.as_deref(), Some(ts)))
        .ok()?;
    let message = &bag.messages[index];
    (message.ts.as_deref() == Some(ts)).then_some(message)
}
fn insert_missing(bag: &mut ChannelMessages, message: &Message) {
    if lookup(bag, message.ts.as_deref().unwrap_or_default()).is_none() {
        bag.upsert(super_platinum_core::state::visible_message(message.clone()));
    }
}
#[cfg(test)]
mod tests;
