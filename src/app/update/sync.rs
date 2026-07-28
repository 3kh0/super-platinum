use super::*;

pub(super) fn hydrate_missing_users(
    app: &App,
    team: &str,
    messages: &[SlackMessage],
) -> Task<Message> {
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(team) else {
        return Task::none();
    };
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };

    let mut seen = HashSet::new();
    let users: Vec<_> = messages
        .iter()
        .flat_map(message_user_ids)
        .filter(|user| !user.trim().is_empty())
        .filter(|user| needs_user_hydration(ws, &app.avatar_profile_hydrated, user))
        .filter(|user| seen.insert(user.clone()))
        .collect();

    if users.is_empty() {
        return Task::none();
    }

    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let team = team.to_owned();
    Task::perform(
        async move { api::fetch_users_info(&transport, &client, &ws_session, users).await },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::UsersLoaded {
                team: team.clone(),
                result,
            })
        },
    )
}

pub(super) fn hydrate_sidebar_channels(app: &App, team: &str) -> Task<Message> {
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(team) else {
        return Task::none();
    };
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };

    let mut seen = HashSet::new();
    let mut channels = Vec::new();
    for id in ws.starred_order.iter().chain(ws.priority_scores.keys()) {
        if !seen.insert(id.clone()) {
            continue;
        }
        let needs_hydration = ws
            .channels
            .get(id)
            .map(channel_needs_hydration)
            .unwrap_or_else(|| ws.starred_order.iter().any(|starred| starred == id));
        if needs_hydration {
            channels.push(id.clone());
        }
    }

    if channels.is_empty() {
        return Task::none();
    }

    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let team = team.to_owned();
    Task::perform(
        {
            let channels = channels.clone();
            async move { api::fetch_channels_info(&transport, &client, &ws_session, channels).await }
        },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::ChannelsLoaded {
                team: team.clone(),
                requested: channels.clone(),
                result,
            })
        },
    )
}

pub(in crate::app) fn channel_needs_hydration(channel: &Channel) -> bool {
    if channel.is_im {
        return channel
            .user
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty);
    }
    channel
        .name
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
}

pub(super) fn hydrate_visible_channels(app: &App, team: &str, channel: &str) -> Task<Message> {
    let messages = visible_channel_messages(app, team, channel);
    hydrate_message_channels(app, team, &messages)
}

pub(super) fn hydrate_message_channels(
    app: &App,
    team: &str,
    messages: &[SlackMessage],
) -> Task<Message> {
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(team) else {
        return Task::none();
    };
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };

    let mut seen = HashSet::new();
    let channels: Vec<_> = messages
        .iter()
        .flat_map(ui::blocks::mentioned_channel_ids)
        .filter(|channel| ws.channels.get(channel).is_none_or(channel_needs_hydration))
        .filter(|channel| {
            !app.channel_hydrated
                .contains(&(team.to_owned(), channel.clone()))
        })
        .filter(|channel| seen.insert(channel.clone()))
        .collect();

    if channels.is_empty() {
        return Task::none();
    }

    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let team = team.to_owned();
    Task::perform(
        {
            let channels = channels.clone();
            async move { api::fetch_channels_info(&transport, &client, &ws_session, channels).await }
        },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::ChannelsLoaded {
                team: team.clone(),
                requested: channels.clone(),
                result,
            })
        },
    )
}

pub(super) fn hydrate_sidebar_dm_users(app: &App, team: &str) -> Task<Message> {
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(team) else {
        return Task::none();
    };
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };

    let mut seen = HashSet::new();
    let users: Vec<_> = ws
        .channels
        .values()
        .filter(|channel| channel.is_im)
        .filter(|channel| {
            ws.should_show_unstarred_read_channels()
                || ws.unread_total(channel) > 0
                || app.active_channel.as_deref() == Some(channel.id.as_str())
                || ws.is_starred_channel(channel)
        })
        .filter_map(crate::state::dm_user_id)
        .filter(|user| !user.trim().is_empty())
        .filter(|user| needs_user_hydration(ws, &app.avatar_profile_hydrated, user))
        .filter(|user| seen.insert((*user).to_owned()))
        .map(str::to_owned)
        .collect();

    if users.is_empty() {
        return Task::none();
    }

    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let team = team.to_owned();
    Task::perform(
        async move { api::fetch_users_info(&transport, &client, &ws_session, users).await },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::UsersLoaded {
                team: team.clone(),
                result,
            })
        },
    )
}

pub(in crate::app) fn needs_user_hydration(
    ws: &Workspace,
    avatar_profile_hydrated: &HashSet<UserId>,
    user_id: &str,
) -> bool {
    let Some(user) = ws.users.get(user_id) else {
        return true;
    };
    ws.avatar_url(user_id).is_none() && !avatar_profile_hydrated.contains(user.id.as_str())
}

pub(super) fn hydrate_visible_missing_users(app: &App, team: &str, channel: &str) -> Task<Message> {
    let messages = visible_channel_messages(app, team, channel);
    hydrate_missing_users(app, team, &messages)
}

pub(super) fn visible_channel_messages(app: &App, team: &str, channel: &str) -> Vec<SlackMessage> {
    let mut messages: Vec<_> = app
        .workspaces
        .get(team)
        .and_then(|ws| ws.messages.get(channel))
        .map(|cm| {
            cm.messages
                .iter()
                .rev()
                .filter(|msg| crate::state::is_channel_timeline_visible(msg))
                .take(ui::channel::VISIBLE_MESSAGE_LIMIT)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    messages.reverse();
    messages
}

pub(super) fn load_avatar_previews(
    app: &mut App,
    team: &str,
    messages: Vec<SlackMessage>,
) -> Task<Message> {
    let mut seen = HashSet::new();
    let users = messages
        .iter()
        .flat_map(message_user_ids)
        .filter(|user| seen.insert(user.clone()))
        .collect();
    let mut seen_bots = HashSet::new();
    let bot_requests = messages
        .iter()
        .filter_map(crate::state::message_bot_avatar)
        .filter(|(key, _)| seen_bots.insert(key.clone()))
        .collect();
    Task::batch([
        load_avatar_url_previews(app, bot_requests),
        load_user_avatar_previews(app, team, users),
    ])
}

pub(super) fn load_user_avatar_previews(
    app: &mut App,
    team: &str,
    users: Vec<UserId>,
) -> Task<Message> {
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };

    let mut seen = HashSet::new();
    let requests: Vec<_> = users
        .into_iter()
        .filter(|user| !app.avatar_previews.contains_key(user))
        .filter(|user| seen.insert(user.clone()))
        .filter_map(|user| ws.avatar_url(&user).map(|url| (user, url)))
        .collect();

    load_avatar_url_previews(app, requests)
}

pub(super) fn load_avatar_url_previews(
    app: &mut App,
    requests: Vec<(String, String)>,
) -> Task<Message> {
    let Some(transport) = app.transport.clone() else {
        return Task::none();
    };

    let requests: Vec<_> = requests
        .into_iter()
        .filter(|(key, _)| !app.avatar_previews.contains_key(key))
        .collect();
    if requests.is_empty() {
        return Task::none();
    }

    for (user, _) in &requests {
        app.avatar_previews
            .insert(user.clone(), FilePreview::Loading);
    }

    let user_agent = crate::slack::xparams::Identity::from_capture().user_agent;
    Task::batch(requests.into_iter().map(|(user, url)| {
        let transport = transport.clone();
        let user_agent = user_agent.clone();
        Task::perform(
            async move { transport.get_bytes(&url, &user_agent).await },
            move |result| {
                Message::Discovery(crate::app::DiscoveryMessage::AvatarLoaded {
                    user: user.clone(),
                    result,
                })
            },
        )
    }))
}

pub(super) fn message_user_ids(msg: &SlackMessage) -> Vec<UserId> {
    let mut users = Vec::new();
    if let Some(user) = msg.user.clone() {
        users.push(user);
    }
    if let Some(user) = msg.parent_user_id.clone() {
        users.push(user);
    }
    users.extend(msg.reply_users.clone());
    for reaction in &msg.reactions {
        users.extend(reaction.users.clone());
    }
    users
}

pub(super) fn show_desktop_notification_task(notification: DesktopNotification) -> Task<Message> {
    Task::perform(
        async move { show_desktop_notification(notification).await },
        Message::DesktopNotificationShown,
    )
}

pub(super) async fn show_desktop_notification(
    notification: DesktopNotification,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || show_desktop_notification_blocking(&notification))
        .await
        .map_err(|e| format!("notification task join failed: {e}"))?
}

#[cfg(target_os = "macos")]
pub(super) fn show_desktop_notification_blocking(
    notification: &DesktopNotification,
) -> Result<(), String> {
    let authorized = mac_usernotifications::blocking::request_auth()
        .map_err(|error| format!("could not request notification permission: {error}"))?;
    if !authorized {
        return Err("notification permission was denied".to_owned());
    }

    mac_usernotifications::Notification::new()
        .title(&notification.title)
        .message(&notification.body)
        .default_sound()
        .send_blocking()
        .map(|_| ())
        .map_err(|error| format!("could not deliver notification: {error}"))
}

#[cfg(target_os = "linux")]
pub(super) fn show_desktop_notification_blocking(
    notification: &DesktopNotification,
) -> Result<(), String> {
    let icon = linux_notification_icon()?;
    notify_rust::Notification::new()
        .appname("Snack")
        .summary(&notification.title)
        .body(&notification.body)
        .icon("snack")
        .image_path(&icon.to_string_lossy())
        .hint(notify_rust::Hint::Category("im.received".to_owned()))
        .show()
        .map(|_| ())
        .map_err(|error| format!("could not deliver notification: {error}"))
}

#[cfg(target_os = "linux")]
pub(super) fn linux_notification_icon() -> Result<std::path::PathBuf, String> {
    const APP_ICON: &[u8] = include_bytes!("../../../assets/icons/icon-256.png");
    static ICON_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

    if let Some(icon) = ICON_PATH.get() {
        return Ok(icon.clone());
    }

    let project_dirs = directories::ProjectDirs::from("com", "echonet", "Snack")
        .ok_or_else(|| "Linux did not provide an application data directory".to_owned())?;
    let icon = project_dirs.data_local_dir().join("snack-notification.png");
    if let Some(parent) = icon.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create notification icon directory: {error}"))?;
    }
    if !std::fs::read(&icon).is_ok_and(|bytes| bytes == APP_ICON) {
        std::fs::write(&icon, APP_ICON)
            .map_err(|error| format!("could not write notification icon: {error}"))?;
    }
    let _ = ICON_PATH.set(icon.clone());
    Ok(icon)
}

#[cfg(target_os = "windows")]
pub(super) fn show_desktop_notification_blocking(
    notification: &DesktopNotification,
) -> Result<(), String> {
    crate::windows::ensure_notification_identity()?;
    notify_rust::Notification::new()
        .app_id(crate::windows::APP_ID)
        .summary(&notification.title)
        .body(&notification.body)
        .sound_name("IM")
        .show()
        .map(|_| ())
        .map_err(|error| format!("could not deliver notification: {error}"))
}

pub(in crate::app) fn notification_for_message(
    ws: &Workspace,
    channel: &str,
    msg: &SlackMessage,
    active_channel: Option<&str>,
) -> Option<DesktopNotification> {
    if active_channel == Some(channel) || msg.user.as_deref() == Some(&ws.self_user_id) {
        return None;
    }

    let direct = ws
        .channels
        .get(channel)
        .map(|channel| channel.is_im || channel.is_mpim)
        .unwrap_or(false);
    let mentioned = message_mentions_user(msg, &ws.self_user_id);
    if !(direct || mentioned) {
        return None;
    }

    let author = ws.message_author_name(msg);
    let channel_label = ws
        .channels
        .get(channel)
        .map(crate::state::channel_label)
        .unwrap_or_else(|| channel.to_owned());
    let title = if direct {
        author
    } else {
        format!("{author} in {channel_label}")
    };
    let body = ui::blocks::notification_text(ws, msg);
    let body = if body.trim().is_empty() {
        "[message]".to_owned()
    } else {
        body
    };

    Some(DesktopNotification { title, body })
}

pub(super) fn message_mentions_user(msg: &SlackMessage, user: &str) -> bool {
    if user.is_empty() {
        return false;
    }
    let encoded = format!("<@{user}>");
    msg.text
        .as_deref()
        .is_some_and(|text| text.contains(&encoded))
        || msg
            .blocks
            .iter()
            .any(|block| value_mentions_user(block, user, &encoded))
}

pub(super) fn value_mentions_user(value: &serde_json::Value, user: &str, encoded: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s == user || s.contains(encoded),
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| value_mentions_user(value, user, encoded)),
        serde_json::Value::Object(map) => {
            matches!(
                map.get("user_id")
                    .or_else(|| map.get("user"))
                    .and_then(serde_json::Value::as_str),
                Some(found) if found == user
            ) || map
                .values()
                .any(|value| value_mentions_user(value, user, encoded))
        }
        _ => false,
    }
}

pub(super) fn apply_realtime(
    app: &mut App,
    team: &str,
    generation: u64,
    event: RtEvent,
) -> Option<DesktopNotification> {
    let now = Instant::now();
    let active_channel = (app.active_team.as_deref() == Some(team))
        .then(|| app.active_channel.clone())
        .flatten();
    let Some(ws) = app.workspaces.get_mut(team) else {
        tracing::debug!(%team, "realtime event for unknown workspace, ignoring");
        return None;
    };
    if generation != ws.rt_generation {
        tracing::debug!(%team, generation, current = ws.rt_generation, "stale realtime event, ignoring");
        return None;
    }
    match event {
        RtEvent::Message(msg) => {
            let Some(channel) = msg.channel.clone() else {
                return None;
            };
            let pending_file_message = (!msg.files.is_empty()
                && msg.user.as_deref() == Some(ws.self_user_id.as_str()))
            .then(|| {
                app.pending_file_messages.iter().position(|pending| {
                    pending.team == team
                        && pending.channel == channel
                        && pending.thread_ts == msg.thread_ts
                        && msg.text.as_deref().unwrap_or_default() == pending.text
                })
            })
            .flatten()
            .map(|index| app.pending_file_messages.remove(index));
            if let Some(pending) = pending_file_message.as_ref() {
                for (file, attachment) in msg.files.iter().zip(&pending.attachments) {
                    let preview_path = attachment
                        .preview_path
                        .as_ref()
                        .or_else(|| is_local_image(&attachment.path).then_some(&attachment.path));
                    if let (Some(key), Some(path)) =
                        (crate::state::file_preview_key(file), preview_path)
                    {
                        app.file_previews.insert(
                            key,
                            FilePreview::Loaded(ImageHandle::from_path(path.clone())),
                        );
                    }
                }
            }
            let pending_message_ts = pending_file_message
                .as_ref()
                .map(|pending| pending.message_ts.as_str());
            let notification =
                notification_for_message(ws, &channel, &msg, active_channel.as_deref());
            if let Some(user) = msg.user.as_deref() {
                ws.clear_typing_user(&channel, user);
            }
            if let Some(root_ts) = thread_root_for_reply(&msg) {
                let key = (team.to_owned(), channel.clone(), root_ts);
                if let Some(cm) = app.threads.get_mut(&key) {
                    if let Some(pending_ts) = pending_message_ts {
                        cm.remove(pending_ts);
                    }
                    upsert_realtime_message(cm, msg.clone());
                }
                if msg.subtype.as_deref() != Some("thread_broadcast") {
                    return notification;
                }
            }
            if let Some(new_count) = app.chat_paused.get_mut(&channel) {
                *new_count += 1;
            }
            let cm = ws.messages.entry(channel).or_default();
            if let Some(pending_ts) = pending_message_ts {
                cm.remove(pending_ts);
            }
            upsert_realtime_message(cm, msg);
            notification
        }
        RtEvent::MessageChanged { channel, message } => {
            if let Some(root_ts) = thread_root_for_reply(&message) {
                if let Some(cm) = app
                    .threads
                    .get_mut(&(team.to_owned(), channel.clone(), root_ts))
                {
                    cm.merge_update(message.clone());
                }
                if message.subtype.as_deref() != Some("thread_broadcast") {
                    return None;
                }
            }
            ws.messages
                .entry(channel)
                .or_default()
                .merge_update(message);
            None
        }
        RtEvent::MessageDeleted {
            channel,
            deleted_ts,
        } => {
            if let Some(cm) = ws.messages.get_mut(&channel) {
                cm.remove(&deleted_ts);
            }
            for ((thread_team, thread_channel, _), cm) in &mut app.threads {
                if thread_team == team && thread_channel == &channel {
                    cm.remove(&deleted_ts);
                }
            }
            None
        }
        RtEvent::UserTyping { channel, user } => {
            ws.set_typing(&channel, user, now);
            None
        }
        RtEvent::PresenceChange { user, presence } => {
            ws.set_presence(user, Presence::from_slack(&presence));
            None
        }
        RtEvent::ReactionAdded {
            channel,
            ts,
            user,
            reaction,
        } => {
            ws.messages
                .entry(channel.clone())
                .or_default()
                .apply_reaction(&ts, &user, &reaction, true);
            apply_thread_reaction(
                &mut app.threads,
                team,
                &channel,
                &ts,
                &user,
                &reaction,
                true,
            );
            None
        }
        RtEvent::ReactionRemoved {
            channel,
            ts,
            user,
            reaction,
        } => {
            if let Some(cm) = ws.messages.get_mut(&channel) {
                cm.apply_reaction(&ts, &user, &reaction, false);
            }
            apply_thread_reaction(
                &mut app.threads,
                team,
                &channel,
                &ts,
                &user,
                &reaction,
                false,
            );
            None
        }
        RtEvent::ActivityUpdated(item) => {
            if app.active_team.as_deref() == Some(team) {
                app.activity.upsert(item);
                app.activity.loaded = true;
            }
            None
        }
        RtEvent::RoomJoin { room, .. } | RtEvent::RoomLeave { room, .. } => {
            ws.apply_room(room);
            None
        }
        RtEvent::RoomUpdate { room } => {
            ws.apply_room(room);
            None
        }
        RtEvent::ChannelMarked {
            channel,
            ts,
            unread_count,
            mention_count,
        } => {
            apply_channel_marked(
                ws,
                &channel,
                &ts,
                unread_count.unwrap_or(0),
                mention_count.unwrap_or(0),
            );
            None
        }
        RtEvent::Unknown(raw) => {
            tracing::debug!(kind = %raw.kind, "unknown realtime event, ignoring");
            None
        }
    }
}

pub(super) fn apply_channel_marked(
    ws: &mut Workspace,
    channel: &str,
    ts: &str,
    unread_count: u32,
    mention_count: u32,
) {
    let cm = ws.messages.entry(channel.to_owned()).or_default();
    if crate::state::cmp_ts(Some(ts), cm.last_read.as_deref()).is_lt() {
        return;
    }
    cm.last_read = Some(ts.to_owned());
    cm.unread_count = unread_count;
    cm.mention_count = mention_count;
    if let Some(c) = ws.channels.get_mut(channel) {
        c.last_read = Some(ts.to_owned());
        c.unread_count = Some(unread_count);
        c.unread_count_display = Some(unread_count);
        c.mention_count = Some(mention_count);
        c.has_unreads = unread_count > 0 || mention_count > 0;
    }
}

pub(super) fn upsert_realtime_message(cm: &mut ChannelMessages, msg: SlackMessage) {
    if let Some(cid) = msg.client_msg_id.clone() {
        if cm.confirm(&cid, msg.clone()) {
            return;
        }
    }
    if cm.confirm_matching_pending(msg.user.as_deref(), msg.text.as_deref(), msg.clone()) {
        return;
    }
    cm.upsert(msg);
}

pub(super) fn thread_root_for_reply(msg: &SlackMessage) -> Option<MessageTs> {
    let root_ts = msg.thread_ts.as_deref()?;
    let ts = msg.ts.as_deref()?;
    (root_ts != ts).then(|| root_ts.to_owned())
}

pub(super) fn apply_thread_reaction(
    threads: &mut HashMap<ThreadKey, ChannelMessages>,
    team: &str,
    channel: &str,
    ts: &str,
    user: &str,
    reaction: &str,
    added: bool,
) {
    for ((thread_team, thread_channel, _), cm) in threads {
        if thread_team == team && thread_channel == channel {
            cm.apply_reaction(ts, user, reaction, added);
        }
    }
}

pub(super) fn reaction_has_user_in(
    cm: &ChannelMessages,
    ts: &str,
    name: &str,
    user: &str,
) -> Option<bool> {
    let message = cm.messages.iter().find(|m| m.ts.as_deref() == Some(ts))?;
    let reaction = message.reactions.iter().find(|r| r.name == name)?;
    Some(crate::state::reaction_has_user(reaction, user))
}

pub(super) fn workspace_cacheable_event(event: &RtEvent) -> bool {
    matches!(
        event,
        RtEvent::Message(_)
            | RtEvent::MessageChanged { .. }
            | RtEvent::MessageDeleted { .. }
            | RtEvent::ReactionAdded { .. }
            | RtEvent::ReactionRemoved { .. }
            | RtEvent::ChannelMarked { .. }
    )
}

pub(super) fn mark_workspace_dirty(app: &mut App, team: &str) {
    if app.cache.is_none() || !app.workspaces.contains_key(team) {
        return;
    };
    app.cache_dirty.insert(team.to_owned(), Instant::now());
}

pub(super) fn flush_due_cache(app: &mut App, now: Instant) -> Task<Message> {
    let Some(account_id) = app.active_account.clone() else {
        app.cache_dirty.clear();
        app.cache_saving.clear();
        return Task::none();
    };
    if app.cache.is_none() {
        app.cache_dirty.clear();
        app.cache_saving.clear();
        return Task::none();
    }

    let due: Vec<_> = app
        .cache_dirty
        .iter()
        .filter(|(team, dirty_at)| {
            now.duration_since(**dirty_at) >= CACHE_SAVE_DEBOUNCE
                && !app.cache_saving.contains_key(*team)
        })
        .map(|(team, _)| team.clone())
        .collect();

    let tasks = due.into_iter().filter_map(|team| {
        let workspace = app.workspaces.get(&team)?.clone();
        let account_id = account_id.clone();
        let started_at = now;
        app.cache_saving.insert(team.clone(), started_at);
        Some(Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    Cache::open_default(&account_id, false)
                        .and_then(|cache| cache.save_workspace(&workspace))
                        .map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| Err(format!("cache save task failed: {e}")))
            },
            move |result| {
                Message::Discovery(crate::app::DiscoveryMessage::CacheSaved {
                    team: team.clone(),
                    started_at,
                    result,
                })
            },
        ))
    });

    Task::batch(tasks)
}
