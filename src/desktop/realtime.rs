use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::events::RtEvent;
use super_platinum_core::slack::realtime::{self, ConnectParams, RtUpdate};

use crate::state::ShellState;

pub async fn worker(mut state: Signal<ShellState>, params: ConnectParams) {
    // The supervisor watches the same link verdict the shell paints on the
    // rail, so a dead socket is dropped in seconds and the reconnect waits for
    // the network to be confirmed back instead of dialling into nothing.
    let health = state
        .read()
        .core
        .transport
        .as_ref()
        .map(|transport| transport.health_watch());
    let mut updates = realtime::connect(params, health);
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
                // Nothing replays the frames that arrived while the socket was
                // down, so whatever happened to the open conversation meanwhile
                // — a reaction above all, which no later frame mentions again —
                // would stay missing until it was left and reopened. Refetching
                // what is on screen is how the reader stops being the last to
                // know. The first connect needs none of this: the conversation
                // it opens fetches itself.
                let reconnected = generation > 1;
                let thread = shell
                    .thread_root
                    .clone()
                    .zip(shell.core.active_channel.clone());
                drop(shell);
                if reconnected {
                    dioxus::prelude::spawn(crate::bootstrap::refresh_selected_channel(state));
                    if let Some((root, channel)) = thread {
                        dioxus::prelude::spawn(crate::bootstrap::open_thread(state, channel, root));
                    }
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
                    .merge_update(super_platinum_core::state::visible_message(message.clone()));
                if message.subtype.as_deref() != Some("thread_broadcast") {
                    return;
                }
            }
            // Merged, not replaced. A frame that re-sends a message we already
            // hold is a partial: `message_replied` carries the parent's text and
            // reply counts but no `reactions` at all, so overwriting the stored
            // copy with it wiped every pill off a message the moment anyone
            // replied to it. Slack merges these frames field by field, and so
            // must we; a wholesale replacement is what `conversations.history`
            // is for, where the payload really is the whole message.
            workspace
                .messages
                .entry(channel)
                .or_default()
                .merge_update(super_platinum_core::state::visible_message(message));
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
        } => apply_reaction(core, team, &channel, &ts, &user, &reaction, true),
        RtEvent::ReactionRemoved {
            channel,
            ts,
            user,
            reaction,
        } => apply_reaction(core, team, &channel, &ts, &user, &reaction, false),
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

/// Puts a reaction on every copy of the message the app holds.
///
/// The transcript is not the only place a message is rendered from: a thread
/// bag holds its own copy of the root and of every reply, and that is what the
/// thread pane draws — the pane Activity opens beside its list included. Only
/// updating the channel meant a pill added while a thread was open never
/// appeared there, which is exactly where a reacted message is read from.
///
/// A channel with no transcript loaded is left alone rather than given an empty
/// one: the reaction is already on the server copy, and the history fetch that
/// opens the conversation brings it in.
fn apply_reaction(
    core: &mut super_platinum_core::CoreAppState,
    team: &str,
    channel: &str,
    ts: &str,
    user: &str,
    name: &str,
    added: bool,
) {
    if let Some(messages) = core
        .workspaces
        .get_mut(team)
        .and_then(|workspace| workspace.messages.get_mut(channel))
    {
        messages.apply_reaction(ts, user, name, added);
    }
    for ((thread_team, thread_channel, _), messages) in &mut core.threads {
        if thread_team == team && thread_channel == channel {
            messages.apply_reaction(ts, user, name, added);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack::models::{Message as SlackMessage, Reaction};
    use super_platinum_core::state::Workspace;

    const TEAM: &str = "T1";
    const CHANNEL: &str = "C1";
    const ROOT: &str = "1.0";

    fn message(ts: &str, thread_ts: Option<&str>) -> SlackMessage {
        SlackMessage {
            ts: Some(ts.into()),
            channel: Some(CHANNEL.into()),
            user: Some("U1".into()),
            text: Some("hello".into()),
            thread_ts: thread_ts.map(str::to_owned),
            ..Default::default()
        }
    }

    /// A workspace holding one reacted root message, with the thread open on it
    /// and carrying its own copy of the root and one reply — the shape the
    /// thread pane draws from.
    fn core() -> super_platinum_core::CoreAppState {
        let mut core = super_platinum_core::CoreAppState::new(
            super_platinum_core::config::Settings::default(),
        );
        let mut workspace =
            Workspace::from_session(&super_platinum_core::config::WorkspaceSession {
                team_id: TEAM.into(),
                enterprise_id: None,
                user_id: "USELF".into(),
                name: "test".into(),
                url: "https://t".into(),
                token: String::new(),
            });
        let mut root = message(ROOT, Some(ROOT));
        root.reactions = vec![Reaction {
            name: "tada".into(),
            users: vec!["U2".into()],
            count: 1,
            ..Default::default()
        }];
        workspace
            .messages
            .entry(CHANNEL.into())
            .or_default()
            .upsert(root.clone());
        core.workspaces.insert(TEAM.into(), workspace);
        let thread = core
            .threads
            .entry((TEAM.into(), CHANNEL.into(), ROOT.into()))
            .or_default();
        thread.upsert(root);
        thread.upsert(message("2.0", Some(ROOT)));
        core
    }

    fn reactions(core: &super_platinum_core::CoreAppState, ts: &str) -> Vec<String> {
        core.workspaces[TEAM].messages[CHANNEL]
            .messages
            .iter()
            .find(|message| message.ts.as_deref() == Some(ts))
            .map(|message| message.reactions.iter().map(|r| r.name.clone()).collect())
            .unwrap_or_default()
    }

    fn thread_reactions(core: &super_platinum_core::CoreAppState, ts: &str) -> Vec<String> {
        core.threads[&(TEAM.into(), CHANNEL.into(), ROOT.into())]
            .messages
            .iter()
            .find(|message| message.ts.as_deref() == Some(ts))
            .map(|message| message.reactions.iter().map(|r| r.name.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_thread_reply_does_not_wipe_the_parents_reactions() {
        let mut core = core();
        // What Slack sends the moment someone replies in a thread: the parent,
        // complete except that it names no reactions at all.
        let mut parent = message(ROOT, Some(ROOT));
        parent.reply_count = Some(1);
        apply(&mut core, TEAM, 0, RtEvent::Message(parent));
        assert_eq!(reactions(&core, ROOT), ["tada"]);
    }

    #[test]
    fn a_reaction_reaches_the_thread_copy_of_a_message() {
        let mut core = core();
        apply(
            &mut core,
            TEAM,
            0,
            RtEvent::ReactionAdded {
                channel: CHANNEL.into(),
                ts: "2.0".into(),
                user: "U2".into(),
                reaction: "eyes".into(),
            },
        );
        assert_eq!(thread_reactions(&core, "2.0"), ["eyes"]);
    }

    #[test]
    fn a_reaction_on_a_root_shows_in_both_the_channel_and_the_thread() {
        let mut core = core();
        apply(
            &mut core,
            TEAM,
            0,
            RtEvent::ReactionAdded {
                channel: CHANNEL.into(),
                ts: ROOT.into(),
                user: "U3".into(),
                reaction: "eyes".into(),
            },
        );
        assert_eq!(reactions(&core, ROOT), ["tada", "eyes"]);
        assert_eq!(thread_reactions(&core, ROOT), ["tada", "eyes"]);
        apply(
            &mut core,
            TEAM,
            0,
            RtEvent::ReactionRemoved {
                channel: CHANNEL.into(),
                ts: ROOT.into(),
                user: "U3".into(),
                reaction: "eyes".into(),
            },
        );
        assert_eq!(reactions(&core, ROOT), ["tada"]);
        assert_eq!(thread_reactions(&core, ROOT), ["tada"]);
    }

    #[test]
    fn a_reaction_in_an_unloaded_channel_creates_no_transcript() {
        let mut core = core();
        apply(
            &mut core,
            TEAM,
            0,
            RtEvent::ReactionAdded {
                channel: "C-NEVER-OPENED".into(),
                ts: "9.0".into(),
                user: "U2".into(),
                reaction: "eyes".into(),
            },
        );
        assert!(
            !core.workspaces[TEAM]
                .messages
                .contains_key("C-NEVER-OPENED")
        );
    }
}
