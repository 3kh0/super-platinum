use dioxus::prelude::{Signal, WritableExt};
use super_platinum_core::slack::events::RtEvent;
use super_platinum_core::slack::realtime::{self, ConnectParams, RtUpdate};

use crate::state::ShellState;

pub async fn worker(mut state: Signal<ShellState>, params: ConnectParams) {
    let mut updates = realtime::connect(params);
    while let Some((team, update)) = updates.recv().await {
        let mut shell = state.write();
        match update {
            RtUpdate::Connected {
                generation,
                connection,
            } => {
                if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                    workspace.rt_generation = generation;
                    let self_user = workspace.self_user_id.clone();
                    connection.send(super_platinum_core::slack::realtime::presence_sub_frame(
                        std::slice::from_ref(&self_user),
                    ));
                    workspace.rt =
                        super_platinum_core::state::RealtimeStatus::Connected(connection);
                }
            }
            RtUpdate::Event { generation, event } => {
                let needs_user_hydration = match event.as_ref() {
                    RtEvent::Message(message) => message.user.as_deref().is_some_and(|user| {
                        shell
                            .core
                            .workspaces
                            .get(&team)
                            .is_none_or(|workspace| !workspace.users.contains_key(user))
                    }),
                    _ => false,
                };
                let notification =
                    crate::notification::for_event(&shell.core, &team, generation, &event);
                let arrival = match event.as_ref() {
                    RtEvent::Message(message)
                        if message.channel.as_deref() == shell.core.active_channel.as_deref() =>
                    {
                        message.ts.clone()
                    }
                    _ => None,
                };
                apply(&mut shell.core, &team, generation, *event);
                if let Some(arrival) = arrival {
                    let now = std::time::Instant::now();
                    shell.message_arrivals.insert(arrival, now);
                    shell.realtime_insert_started = Some(now);
                }
                shell.refresh_from_core();
                drop(shell);
                if needs_user_hydration {
                    dioxus::prelude::spawn(crate::bootstrap::hydrate_current_surface(state));
                }
                if let Some(notification) = notification {
                    dioxus::prelude::spawn(crate::notification::show(notification));
                }
            }
            RtUpdate::Disconnected { generation } => {
                if let Some(workspace) = shell.core.workspaces.get_mut(&team)
                    && generation >= workspace.rt_generation
                {
                    workspace.rt = super_platinum_core::state::RealtimeStatus::Disconnected;
                }
            }
        }
    }
}

fn apply(
    core: &mut super_platinum_core::CoreAppState,
    team: &str,
    generation: u64,
    event: RtEvent,
) {
    let Some(workspace) = core.workspaces.get_mut(team) else {
        return;
    };
    if generation != workspace.rt_generation {
        return;
    }
    match event {
        RtEvent::Message(message) => {
            let Some(channel) = message.channel.clone() else {
                return;
            };
            if let Some(user) = message.user.as_deref() {
                workspace.clear_typing_user(&channel, user);
            }
            let thread_root = message
                .thread_ts
                .clone()
                .filter(|root| message.ts.as_deref() != Some(root));
            if let Some(root) = thread_root {
                core.threads
                    .entry((team.to_owned(), channel.clone(), root))
                    .or_default()
                    .upsert(super_platinum_core::state::visible_message(message.clone()));
                if message.subtype.as_deref() != Some("thread_broadcast") {
                    return;
                }
            }
            workspace
                .messages
                .entry(channel)
                .or_default()
                .upsert(super_platinum_core::state::visible_message(message));
        }
        RtEvent::MessageChanged { channel, message } => {
            workspace
                .messages
                .entry(channel.clone())
                .or_default()
                .merge_update(message.clone());
            for ((thread_team, thread_channel, _), messages) in &mut core.threads {
                if thread_team == team && thread_channel == &channel {
                    messages.merge_update(message.clone());
                }
            }
        }
        RtEvent::MessageDeleted {
            channel,
            deleted_ts,
        } => {
            if let Some(messages) = workspace.messages.get_mut(&channel) {
                messages.remove(&deleted_ts);
            }
            for ((thread_team, thread_channel, _), messages) in &mut core.threads {
                if thread_team == team && thread_channel == &channel {
                    messages.remove(&deleted_ts);
                }
            }
        }
        RtEvent::UserTyping { channel, user } => {
            workspace.set_typing(&channel, user, std::time::Instant::now())
        }
        RtEvent::PresenceChange { users, presence } => {
            let presence = super_platinum_core::state::Presence::from_slack(&presence);
            for user in users {
                workspace.set_presence(user, presence);
            }
        }
        RtEvent::DndUpdated { user, dnd } => {
            if user == workspace.self_user_id {
                workspace.self_dnd = dnd;
            }
        }
        RtEvent::ReactionAdded {
            channel,
            ts,
            user,
            reaction,
        } => {
            workspace
                .messages
                .entry(channel)
                .or_default()
                .apply_reaction(&ts, &user, &reaction, true);
        }
        RtEvent::ReactionRemoved {
            channel,
            ts,
            user,
            reaction,
        } => {
            if let Some(messages) = workspace.messages.get_mut(&channel) {
                messages.apply_reaction(&ts, &user, &reaction, false);
            }
        }
        RtEvent::ActivityUpdated(item) => {
            if core.active_team.as_deref() == Some(team) {
                core.activity.upsert(item);
                core.activity.loaded = true;
            }
        }
        RtEvent::RoomJoin { room, .. }
        | RtEvent::RoomLeave { room, .. }
        | RtEvent::RoomUpdate { room } => {
            workspace.apply_room(room);
        }
        RtEvent::ChannelMarked {
            channel,
            ts,
            unread_count,
            mention_count,
        } => {
            let unread_count = unread_count.unwrap_or(0);
            let mention_count = mention_count.unwrap_or(0);
            let messages = workspace.messages.entry(channel.clone()).or_default();
            messages.last_read = Some(ts.clone());
            messages.unread_count = unread_count;
            messages.mention_count = mention_count;
            if let Some(channel) = workspace.channels.get_mut(&channel) {
                channel.last_read = Some(ts);
                channel.unread_count = Some(unread_count);
                channel.unread_count_display = Some(unread_count);
                channel.mention_count = Some(mention_count);
                channel.has_unreads = unread_count > 0 || mention_count > 0;
            }
        }
        RtEvent::Unknown(_) => {}
    }
}
