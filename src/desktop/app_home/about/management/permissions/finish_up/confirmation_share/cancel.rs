//! Local recovery for this Share queue only. Never a remote deletion or replay.
use super::*;
use crate::app_home::channel_fetches::context::Context;
use crate::state::PendingDeliveryStatus;
use super_platinum_core::{state::ChannelMessages, workflow_share::Message};

#[derive(Clone)]
pub(super) struct Action {
    pub(super) attempt: u64,
    pub(super) generation: u64,
    pub(super) owner: Arc<Context>,
    pub(super) message: Message,
    status: PendingDeliveryStatus,
}
impl Action {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        if !self.owner.account_current(s)
            || s.core.active_team.as_deref() != Some(&self.owner.session.team_id)
            || s.core.active_channel.as_deref() != Some(self.message.channel())
            || s.pending_delivery.get(self.message.local_ts()) != Some(&self.status)
        {
            return false;
        }
        if attempt(s, self.attempt)
            .and_then(|a| a.confirmation_share.as_ref())
            .and_then(|v| v.operation.as_ref())
            .is_none_or(|op| op.generation() != self.generation)
        {
            return false;
        }
        let operation = attempt(s, self.attempt)
            .and_then(|a| a.confirmation_share.as_ref())
            .and_then(|v| v.operation.as_ref())
            .and_then(run::Operation::cancellable);
        if !operation.is_some_and(|(owner, message, status)| {
            Arc::ptr_eq(owner, &self.owner) && message == &self.message && status == self.status
        }) {
            return false;
        }
        s.core
            .workspaces
            .get(&self.owner.session.team_id)
            .and_then(|w| w.messages.get(self.message.channel()))
            .is_some_and(|bag| pending_original(bag, &self.message, &self.owner.session.user_id))
    }
}

fn same_row(
    row: &super_platinum_core::slack::models::Message,
    message: &Message,
    user: &str,
) -> bool {
    // Full original rich payload, not merely a reused timestamp/client id.
    if row.ts.as_deref() != Some(message.local_ts())
        || row.client_msg_id.as_deref() != Some(message.client_msg_id())
    {
        return false;
    }
    serde_json::to_value(row).is_ok_and(|raw| {
        serde_json::to_value(run::optimistic(message, user)).is_ok_and(|original| raw == original)
    })
}
pub(super) fn pending_original(bag: &ChannelMessages, message: &Message, user: &str) -> bool {
    bag.pending.iter().any(|ts| ts == message.local_ts())
        && bag
            .messages
            .iter()
            .filter(|row| row.ts.as_deref() == Some(message.local_ts()))
            .count()
            == 1
        && bag.messages.iter().any(|row| same_row(row, message, user))
}
fn remove_copy(bag: &mut ChannelMessages, message: &Message, user: &str) {
    if bag
        .messages
        .iter()
        .filter(|row| row.ts.as_deref() == Some(message.local_ts()))
        .count()
        == 1
        && bag.messages.iter().any(|row| same_row(row, message, user))
    {
        // No remote tombstone: a genuinely applied uncertain send may later
        // arrive through Slack's independent realtime/history reconciliation.
        bag.remove(message.local_ts());
    }
}
fn cancel(mut state: Signal<ShellState>, action: Action) {
    let mut s = state.write();
    if !action.current(&s) {
        return;
    }
    let team = &action.owner.session.team_id;
    let user = &action.owner.session.user_id;
    let channel = action.message.channel();
    if let Some(bag) = s
        .core
        .workspaces
        .get_mut(team)
        .and_then(|w| w.messages.get_mut(channel))
    {
        remove_copy(bag, &action.message, user);
    }
    for ((copy_team, copy_channel, _), bag) in &mut s.core.threads {
        if copy_team == team && copy_channel == channel {
            remove_copy(bag, &action.message, user);
        }
    }
    s.pending_delivery.remove(action.message.local_ts());
    if let Some(op) = attempt_mut(&mut s, action.attempt)
        .and_then(|a| a.confirmation_share.as_mut())
        .and_then(|v| v.operation.as_mut())
    {
        op.mark_locally_cancelled();
    }
    s.refresh_from_core();
}

pub(crate) fn row(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    channel: &str,
    ts: &str,
) -> Element {
    let action = snapshot
        .app_home
        .about
        .management
        .permissions
        .first_attempts
        .iter()
        .find_map(|a| {
            let (owner, message, status) = a
                .confirmation_share
                .as_ref()?
                .operation
                .as_ref()?
                .cancellable()?;
            if message.channel() != channel || message.local_ts() != ts {
                return None;
            }
            let action = Action {
                attempt: a.original.id,
                generation: a
                    .confirmation_share
                    .as_ref()?
                    .operation
                    .as_ref()?
                    .generation(),
                owner: owner.clone(),
                message: message.clone(),
                status,
            };
            action.current(snapshot).then_some(action)
        });
    let Some(action) = action else {
        return rsx! {};
    };
    let retry = resend::eligible(snapshot, &action);
    let blocked = resend::blocked(snapshot, &action);
    let retry_action = action.clone();
    rsx! {
        div { class:"upload-delivery", "data-workflow-share-delivery":ts,
            ondoubleclick:|event|event.stop_propagation(),
            if retry {
                button {type:"button",class:"upload-delivery-check", "data-workflow-share-resend":ts,
                    "aria-label":"Retry failed workflow message", disabled:blocked,
                    title:if blocked {"Wait before trying again."} else {"Retry this original workflow message."},
                    onclick:move |event| {event.stop_propagation();resend::send(state,retry_action.clone());},
                    "Try again"
                }
            }
            button { type:"button", class:"upload-delivery-check", "data-workflow-share-cancel":ts,
                "aria-label":"Cancel failed workflow message",
                title:"Remove this local message. This does not undo a message Slack may have received.",
                onclick:move |event| { event.stop_propagation(); cancel(state,action.clone()); },
                "Cancel"
            }
        }
    }
}

#[cfg(test)]
mod tests;
