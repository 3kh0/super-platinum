//! Optimistic copies are scoped to the captured thread or channel, never the
//! conversation that happened to become visible during a durable send.
use super::*;

pub(super) fn append_pending(
    shell: &mut ShellState,
    intent: &ConsumeIntent,
    local: &LocalDraft,
) -> Option<PendingSend> {
    let key = &intent.scope.key;
    let user = shell.core.workspaces.get(&key.team)?.self_user_id.clone();
    let (_, optimistic_ts) = ShellState::optimistic_identity("super-platinum-durable");
    let pending = PendingSend {
        account: Some(intent.scope.account_id.clone()),
        account_epoch: shell.core.account_epoch,
        team: key.team.clone(),
        channel: key.channel.clone(),
        thread_ts: key.thread.clone(),
        reply_broadcast: key.thread.is_some() && local.composer.reply_broadcast,
        text: local.composer.text.clone(),
        client_msg_id: intent.client_msg_id.clone(),
        optimistic_ts,
    };
    let message = message(&pending, intent, user, pending.optimistic_ts.clone());
    let messages = if let Some(root) = &key.thread {
        shell
            .core
            .threads
            .entry((key.team.clone(), key.channel.clone(), root.clone()))
            .or_default()
    } else {
        shell
            .core
            .workspaces
            .get_mut(&key.team)?
            .messages
            .entry(key.channel.clone())
            .or_default()
    };
    messages.upsert(message);
    messages.pending.push(pending.optimistic_ts.clone());
    shell
        .message_arrivals
        .insert(pending.optimistic_ts.clone(), std::time::Instant::now());
    if key.thread.is_none() && shell.composer_key(ComposerTarget::Channel).as_ref() == Some(key) {
        shell.core.pending_scroll_to = Some((
            key.channel.clone(),
            super_platinum_core::domain::PendingScrollTarget::Latest,
        ));
        shell.stick_to_bottom = true;
    }
    shell.refresh_from_core();
    shell.refresh_thread_from_core();
    Some(pending)
}

fn message(pending: &PendingSend, intent: &ConsumeIntent, user: String, ts: String) -> Message {
    Message {
        ts: Some(ts),
        client_msg_id: Some(pending.client_msg_id.clone()),
        channel: Some(pending.channel.clone()),
        thread_ts: pending.thread_ts.clone(),
        subtype: pending.reply_broadcast.then(|| "thread_broadcast".into()),
        user: Some(user),
        kind: Some("message".into()),
        text: Some(pending.text.clone()),
        blocks: intent.snapshot.blocks.clone(),
        ..Default::default()
    }
}

pub(super) fn apply_outcome(
    shell: &mut ShellState,
    pending: &PendingSend,
    intent: &ConsumeIntent,
    _local: &LocalDraft,
    outcome: &Outcome,
    applied: bool,
) {
    match outcome {
        Outcome::Sent { ts, .. } if applied => {
            if let Some(user) = shell
                .core
                .workspaces
                .get(&pending.team)
                .map(|w| w.self_user_id.clone())
            {
                let message = message(pending, intent, user, ts.clone());
                if let Some(root) = &pending.thread_ts {
                    if let Some(messages) = shell.core.threads.get_mut(&(
                        pending.team.clone(),
                        pending.channel.clone(),
                        root.clone(),
                    )) {
                        confirm(messages, pending, &message);
                    }
                }
                if pending.thread_ts.is_none() || pending.reply_broadcast {
                    if let Some(workspace) = shell.core.workspaces.get_mut(&pending.team) {
                        confirm(
                            workspace
                                .messages
                                .entry(pending.channel.clone())
                                .or_default(),
                            pending,
                            &message,
                        );
                    }
                }
            }
            shell.pending_delivery.remove(&pending.optimistic_ts);
        }
        outcome => {
            let rejected = applied && matches!(outcome, Outcome::Rejected);
            shell.pending_delivery.insert(
                pending.optimistic_ts.clone(),
                if rejected {
                    PendingDeliveryStatus::Failed
                } else {
                    PendingDeliveryStatus::Unconfirmed
                },
            );
            shell.show_toast(if rejected {
                "Message not sent. Its saved content is available in Drafts."
            } else {
                "Delivery is unconfirmed. Use Check delivery in Drafts before sending again."
            });
        }
    }
    shell.refresh_from_core();
    shell.refresh_thread_from_core();
}

fn confirm(
    messages: &mut super_platinum_core::state::ChannelMessages,
    pending: &PendingSend,
    message: &Message,
) {
    let Some(ts) = &message.ts else { return };
    // Preserve newer realtime edits and deletion tombstones in both copies.
    if messages.was_deleted(ts) || messages.messages.iter().any(|m| m.ts.as_ref() == Some(ts)) {
        messages.remove(&pending.optimistic_ts);
    } else {
        if !messages.confirm(&pending.client_msg_id, message.clone()) {
            messages.upsert(message.clone());
        }
    }
}
