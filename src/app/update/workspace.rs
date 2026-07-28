use super::*;

pub(super) fn activity_scrolled(app: &mut App, remaining: f32) -> Task<Message> {
    if app.main_view != crate::state::MainView::Activity
        || !should_load_older_activity(&app.activity, remaining)
    {
        return Task::none();
    }
    let cursor = app.activity.next_cursor.clone();
    load_activity(app, cursor)
}

pub(super) fn unreads_scrolled(app: &mut App, remaining: f32) -> Task<Message> {
    if app.main_view != crate::state::MainView::Unreads || remaining > LOAD_OLDER_ACTIVITY_BOTTOM_PX
    {
        return Task::none();
    }
    load_unreads(app, None)
}

pub(super) fn load_unreads(app: &mut App, preferred: Option<ChannelId>) -> Task<Message> {
    if app.main_view != crate::state::MainView::Unreads {
        return Task::none();
    }
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let (transport, ws_session) = {
        let Some((transport, session)) = app.live() else {
            return Task::none();
        };
        let Some(ws_session) = session.workspaces.get(&team).cloned() else {
            return Task::none();
        };
        (transport.clone(), ws_session)
    };
    let Some(ws) = app.workspaces.get(&team) else {
        return Task::none();
    };

    let mut channels: Vec<_> = ws
        .channels
        .values()
        .filter(|channel| ws.unread_total(channel) > 0)
        .map(|channel| {
            let last_read = ws
                .messages
                .get(&channel.id)
                .and_then(|messages| messages.last_read.clone())
                .or_else(|| channel.last_read.clone());
            (
                channel.id.clone(),
                ws.channel_recency(channel),
                ws.unread_total(channel),
                last_read,
            )
        })
        .collect();
    channels.sort_by(|a, b| {
        let order = b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0));
        if app.unreads.sort == UnreadsSort::Newest {
            order
        } else {
            order.reverse()
        }
    });
    if let Some(preferred) = preferred.as_ref() {
        channels.sort_by_key(|(channel, ..)| channel != preferred);
    }

    let pending: Vec<_> = channels
        .into_iter()
        .filter(|(channel, ..)| {
            !app.unreads.loaded.contains(channel)
                && !app.unreads.loading.contains_key(channel)
                && (preferred.as_ref() == Some(channel) || !app.unreads.collapsed.contains(channel))
        })
        .take(if preferred.is_some() {
            1
        } else {
            UNREADS_BATCH_SIZE
        })
        .collect();

    let mut tasks = Vec::with_capacity(pending.len());
    for (channel, _, unread_count, oldest) in pending {
        app.unreads.load_seq = app.unreads.load_seq.wrapping_add(1);
        let seq = app.unreads.load_seq;
        app.unreads.loading.insert(channel.clone(), seq);
        app.unreads.failed.remove(&channel);
        let transport = transport.clone();
        let client = app.client.clone();
        let ws_session = ws_session.clone();
        let result_team = team.clone();
        let result_channel = channel.clone();
        tasks.push(Task::perform(
            async move {
                api::fetch_history(
                    &transport,
                    &client,
                    &ws_session,
                    HistoryArgs {
                        channel,
                        oldest,
                        limit: Some(unread_count.saturating_add(1).clamp(56, 100)),
                        inclusive: true,
                        ..Default::default()
                    },
                )
                .await
            },
            move |result| {
                Message::Runtime(crate::app::RuntimeMessage::UnreadsChannelLoaded {
                    team: result_team.clone(),
                    channel: result_channel.clone(),
                    seq,
                    result,
                })
            },
        ));
    }
    Task::batch(tasks)
}

pub(super) fn mark_unread_channel(app: &mut App, channel: ChannelId) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let has_latest = app
        .workspaces
        .get(&team)
        .and_then(|ws| ws.messages.get(&channel))
        .and_then(ChannelMessages::latest_confirmed_ts)
        .is_some();
    if has_latest {
        mark_latest_visible(app, &team, &channel)
    } else {
        app.unreads.mark_when_loaded.insert(channel.clone());
        load_unreads(app, Some(channel))
    }
}

pub(super) fn focused_unread_channel(app: &App) -> Option<ChannelId> {
    if let Some(channel) = app.unreads.focused.as_ref().filter(|channel| {
        app.active_workspace()
            .and_then(|ws| ws.channels.get(*channel).map(|item| ws.unread_total(item)))
            .unwrap_or(0)
            > 0
    }) {
        return Some(channel.clone());
    }
    let ws = app.active_workspace()?;
    ws.channels
        .values()
        .filter(|channel| ws.unread_total(channel) > 0)
        .max_by_key(|channel| ws.channel_recency(channel))
        .map(|channel| channel.id.clone())
}

pub(in crate::app) fn should_load_older_activity(activity: &ActivityState, remaining: f32) -> bool {
    remaining <= LOAD_OLDER_ACTIVITY_BOTTOM_PX
        && activity.loaded
        && activity.next_cursor.is_some()
        && !activity.loading
}

pub(in crate::app) fn should_auto_load_activity(activity: &ActivityState) -> bool {
    activity.next_cursor.is_some()
        && (activity.items.is_empty()
            || activity.unread_only && !activity.items.iter().any(|item| item.is_unread))
}

pub(super) fn load_activity(app: &mut App, cursor: Option<String>) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws = ws.clone();
    let unread_only = app.activity.unread_only;
    app.activity.load_seq = app.activity.load_seq.wrapping_add(1);
    let seq = app.activity.load_seq;
    app.activity.loading = true;
    let requested_cursor = cursor.clone();
    Task::perform(
        async move { api::fetch_activity_feed(&transport, &client, &ws, 20, cursor, unread_only).await },
        move |result| {
            Message::Runtime(crate::app::RuntimeMessage::ActivityLoaded {
                team: team.clone(),
                cursor: requested_cursor.clone(),
                seq,
                result,
            })
        },
    )
}

pub(super) fn dms_scrolled(app: &mut App, remaining: f32) -> Task<Message> {
    if app.main_view != crate::state::MainView::Dms
        || remaining > LOAD_OLDER_ACTIVITY_BOTTOM_PX
        || !app.dms.loaded
        || app.dms.loading
        || app.dms.next_cursor.is_none()
    {
        return Task::none();
    }
    let cursor = app.dms.next_cursor.clone();
    load_dms(app, cursor)
}

pub(super) fn load_dms(app: &mut App, cursor: Option<String>) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws = ws.clone();
    app.dms.load_seq = app.dms.load_seq.wrapping_add(1);
    let seq = app.dms.load_seq;
    app.dms.loading = true;
    let requested_cursor = cursor.clone();
    Task::perform(
        async move { api::fetch_client_dms(&transport, &client, &ws, 100, cursor).await },
        move |result| {
            Message::Runtime(crate::app::RuntimeMessage::DmsLoaded {
                team: team.clone(),
                cursor: requested_cursor.clone(),
                seq,
                result,
            })
        },
    )
}

pub(super) fn refresh_counts(app: &App) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws = ws.clone();
    Task::perform(
        async move { api::fetch_counts(&transport, &client, &ws).await },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::CountsLoaded(
                team.clone(),
                result,
            ))
        },
    )
}

pub(super) fn hydrate_users_by_id(app: &App, team: &str, ids: Vec<UserId>) -> Task<Message> {
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
    let users: Vec<_> = ids
        .into_iter()
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

pub(super) fn hydrate_activity_messages(app: &App) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let mut groups: HashMap<ChannelId, Vec<MessageTs>> = HashMap::new();
    for item in &app.activity.items {
        let Some(channel) = item.channel() else {
            continue;
        };
        for ts in item.request_ts() {
            if app
                .activity
                .hydrated
                .contains_key(&(channel.to_owned(), ts.clone()))
            {
                continue;
            }
            let bucket = groups.entry(channel.to_owned()).or_default();
            if !bucket.contains(&ts) {
                bucket.push(ts);
            }
        }
    }
    if groups.is_empty() {
        return Task::none();
    }
    let message_ids: Vec<(ChannelId, Vec<MessageTs>)> = groups.into_iter().collect();

    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws = ws.clone();
    Task::perform(
        async move { api::fetch_messages_list(&transport, &client, &ws, message_ids).await },
        move |result| {
            Message::Runtime(crate::app::RuntimeMessage::ActivityMessagesLoaded(
                team.clone(),
                result,
            ))
        },
    )
}

pub(super) fn select_workspace(app: &mut App, team: TeamId) -> Task<Message> {
    if app.active_team.as_deref() == Some(&team) {
        return Task::none();
    }
    if !app.workspaces.contains_key(&team) {
        return Task::none();
    }

    if let (Some(current_team), Some(current_channel)) =
        (app.active_team.clone(), app.active_channel.clone())
    {
        app.last_active_channels
            .insert(current_team.clone(), current_channel.clone());
        if let Some(ws) = app.workspaces.get_mut(&current_team) {
            ws.last_active_channel = Some(current_channel);
        }
        mark_workspace_dirty(app, &current_team);
    }

    app.active_team = Some(team.clone());
    app.activity = ActivityState::default();
    app.unreads = UnreadsState::default();
    app.dms = DmsState::default();
    app.show_account_menu = false;
    app.account_menu_open = false;
    app.active_channel = preferred_channel(app, &team);
    if let (Some(ws), Some(channel)) = (app.workspaces.get_mut(&team), app.active_channel.clone()) {
        ws.last_active_channel = Some(channel);
        mark_workspace_dirty(app, &team);
    }
    app.active_thread = None;
    app.thread_open = false;
    app.search = None;
    app.search_input.clear();
    app.editing = None;
    app.edit_content = Content::new();
    app.text_selection = None;
    app.composer = Content::new();
    app.thread_composer = Content::new();

    let activity_task = match app.main_view {
        crate::state::MainView::Unreads => load_unreads(app, None),
        crate::state::MainView::Activity => load_activity(app, None),
        crate::state::MainView::Dms => load_dms(app, None),
        crate::state::MainView::Home => Task::none(),
    };
    let Some(channel) = app.active_channel.clone() else {
        return activity_task;
    };
    app.pending_scroll_to =
        channel_open_scroll_target(app, &team, &channel).map(|target| (channel.clone(), target));
    let focus = focus_active_composer(app);
    let mark = mark_latest_visible(app, &team, &channel);
    Task::batch([
        mark,
        refresh_channel_history(app, &team, &channel),
        scroll_to_pending(app, &channel),
        activity_task,
        focus,
    ])
}

pub(super) fn refresh_channel_history(
    app: &mut App,
    team: &str,
    channel: &ChannelId,
) -> Task<Message> {
    if app.live().is_none() {
        return Task::none();
    }
    let unread_anchor = app.unread_anchor(team, channel);
    let latest = {
        let cm = app
            .workspaces
            .get_mut(team)
            .map(|ws| ws.messages.entry(channel.clone()).or_default());
        let Some(cm) = cm else {
            return Task::none();
        };
        if cm.history_refreshing {
            return Task::none();
        }
        cm.history_refreshing = true;
        cm.history_failed = false;
        cm.loaded.then(|| cm.latest_confirmed_ts()).flatten()
    };

    if let Some(anchor) = unread_anchor {
        app.load_history_around(team, channel, anchor)
    } else if let Some(latest) = latest {
        app.load_history_since(team, channel, latest)
    } else {
        app.load_history(team, channel)
    }
}

pub(super) fn refresh_channel_history_around(
    app: &mut App,
    team: &str,
    channel: &ChannelId,
    anchor: MessageTs,
) -> Task<Message> {
    if app.live().is_none() {
        return Task::none();
    }
    let Some(cm) = app
        .workspaces
        .get_mut(team)
        .map(|ws| ws.messages.entry(channel.clone()).or_default())
    else {
        return Task::none();
    };
    if cm.history_refreshing {
        return Task::none();
    }
    cm.history_refreshing = true;
    cm.history_failed = false;
    app.load_history_around(team, channel, anchor)
}

pub(super) fn focus_active_composer(app: &App) -> Task<Message> {
    if app.thread_open {
        return operation::focus(ui::composer::THREAD_INPUT_ID);
    }
    if app.active_channel.is_some() {
        return operation::focus(ui::composer::CHANNEL_INPUT_ID);
    }
    Task::none()
}

pub(super) fn authenticate(app: &mut App, add_account: bool) -> Task<Message> {
    match std::env::current_exe() {
        Ok(exe) => Task::perform(
            async move {
                let mut command = tokio::process::Command::new(exe);
                command.env("SNACK_AUTH", "1");
                if add_account {
                    command.env("SNACK_AUTH_ADD", "1");
                }
                command
                    .status()
                    .await
                    .map(|status| status.success())
                    .unwrap_or(false)
            },
            Message::AuthenticationFinished,
        ),
        Err(e) => {
            app.toast(format!("could not locate snack binary: {e}"));
            Task::none()
        }
    }
}

pub(super) fn switch_account(app: &mut App, account_id: config::AccountId) -> Task<Message> {
    if app.active_account.as_deref() == Some(&account_id) {
        app.account_menu_open = false;
        return Task::none();
    }
    if !app.accounts.contains_key(&account_id) {
        app.toast("saved account not found");
        return Task::none();
    }
    if let Err(e) = config::set_active_account(&account_id) {
        app.toast(format!("could not switch account: {e}"));
        return Task::none();
    }
    app.activate_account(account_id)
}

pub(super) fn sign_out(app: &mut App) -> Task<Message> {
    let Some(account_id) = app.active_account.clone() else {
        return Task::none();
    };
    match config::remove_account(&account_id) {
        Ok(Some(next_account)) => {
            app.accounts.remove(&account_id);
            app.reset_account_runtime();
            let cache_error = Cache::remove_default(&account_id).err();
            let task = app.activate_account(next_account);
            if let Some(error) = cache_error {
                app.toast(format!(
                    "could not remove signed-out account cache: {error}"
                ));
            }
            task
        }
        Ok(None) => {
            app.accounts.clear();
            app.reset_account_runtime();
            if let Err(error) = Cache::remove_default(&account_id) {
                app.toast(format!(
                    "could not remove signed-out account cache: {error}"
                ));
            }
            Task::none()
        }
        Err(e) => {
            app.toast(format!("could not sign out: {e}"));
            Task::none()
        }
    }
}

pub(super) fn set_self_presence(app: &mut App, presence: Presence) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        app.account_menu_open = false;
        return Task::none();
    };
    let previous = app
        .workspaces
        .get(&team)
        .and_then(|ws| ws.presence.get(&ws.self_user_id).copied());
    if let Some(ws) = app.workspaces.get_mut(&team) {
        ws.set_presence(ws.self_user_id.clone(), presence);
    }
    app.account_menu_open = false;
    mark_workspace_dirty(app, &team);

    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let slack_presence = slack_presence_value(presence).to_owned();
    Task::perform(
        async move { api::set_presence(&transport, &client, &ws_session, slack_presence).await },
        move |result| {
            Message::Runtime(crate::app::RuntimeMessage::SelfPresenceUpdated {
                team: team.clone(),
                presence,
                previous,
                result,
            })
        },
    )
}

pub(super) fn slack_presence_value(presence: Presence) -> &'static str {
    match presence {
        Presence::Active => "auto",
        Presence::Away | Presence::Unknown => "away",
    }
}

pub(in crate::app) fn preferred_channel(app: &App, team: &str) -> Option<ChannelId> {
    let ws = app.workspaces.get(team)?;
    if let Some(channel) = app
        .last_active_channels
        .get(team)
        .filter(|channel| ws.channels.contains_key(*channel))
    {
        return Some(channel.clone());
    }
    if let Some(channel) = ws
        .last_active_channel
        .as_ref()
        .filter(|channel| ws.channels.contains_key(*channel))
    {
        return Some(channel.clone());
    }
    app.active_channel
        .as_ref()
        .filter(|channel| ws.channels.contains_key(*channel))
        .cloned()
        .or_else(|| ws.channels.keys().next().cloned())
}

pub(super) fn next_seq(app: &mut App) -> u64 {
    app.send_seq += 1;
    app.send_seq
}

pub(super) fn drop_target(app: &App) -> AttachTarget {
    if app.active_thread.is_none() {
        return AttachTarget::Channel;
    }
    if app.main_view != crate::state::MainView::Home || !app.thread_open {
        return AttachTarget::Thread;
    }
    let Some((cursor, window)) = cursor_in_window() else {
        return AttachTarget::Thread;
    };
    let zone = thread_panel_x_range(window.width, app.profile_pane.is_some() && app.profile_open);
    let target = if zone.contains(&cursor.x) {
        AttachTarget::Thread
    } else {
        AttachTarget::Channel
    };
    tracing::debug!(
        cursor = cursor.x,
        window = window.width,
        zone = ?zone,
        ?target,
        "routed file drop"
    );
    target
}

/// Horizontal span of the thread panel in the Home layout, in logical px.
///
/// The panel is pinned to the right edge inside the shell padding, behind the
/// profile pane when that is open.
pub(in crate::app) fn thread_panel_x_range(
    window_width: f32,
    profile_pane_open: bool,
) -> std::ops::Range<f32> {
    let gap = ui::theme::gap();
    let right = if profile_pane_open {
        gap + ui::profile::PANE_WIDTH + gap
    } else {
        gap
    };
    let end = window_width - right;
    (end - ui::theme::THREAD_WIDTH)..end
}

#[cfg(target_os = "macos")]
pub(super) fn cursor_in_window() -> Option<(iced::Point, iced::Size)> {
    crate::macos::cursor_in_window()
}

/// Other platforms give drops no coordinates either, and have no pointer query
/// wired up yet, so the caller keeps its thread-first fallback.
#[cfg(not(target_os = "macos"))]
pub(super) fn cursor_in_window() -> Option<(iced::Point, iced::Size)> {
    None
}

pub(super) fn composer_content_mut(app: &mut App, target: ComposerTarget) -> &mut Content {
    match target {
        ComposerTarget::Channel => &mut app.composer,
        ComposerTarget::Thread => &mut app.thread_composer,
        ComposerTarget::Edit => &mut app.edit_content,
    }
}

pub(super) fn attachments_mut(app: &mut App, target: AttachTarget) -> &mut Vec<ComposerAttachment> {
    match target {
        AttachTarget::Channel => &mut app.composer_attachments,
        AttachTarget::Thread => &mut app.thread_composer_attachments,
    }
}

pub(super) fn add_attachments(app: &mut App, target: AttachTarget, paths: Vec<PathBuf>) {
    for path in paths {
        if !path.is_file()
            || attachments_mut(app, target)
                .iter()
                .any(|attachment| attachment.path == path)
        {
            continue;
        }
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        app.attachment_seq += 1;
        let id = app.attachment_seq;
        attachments_mut(app, target).push(ComposerAttachment {
            id,
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("attachment")
                .to_owned(),
            path,
            bytes: metadata.len(),
            uploading: false,
            upload_started: None,
            upload_cancel: None,
            upload_progress: None,
            preview_path: None,
        });
    }
}

pub(super) fn is_video(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(extension) if matches!(
            extension.to_ascii_lowercase().as_str(),
            "mp4" | "mov" | "m4v" | "webm"
        )
    )
}

pub(super) fn is_local_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(extension) if matches!(
            extension.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
    )
}

pub(super) fn video_preview_tasks(paths: Vec<PathBuf>) -> Task<Message> {
    Task::batch(paths.into_iter().map(|source| {
        Task::perform(video_thumbnail(source.clone()), move |result| {
            Message::Conversation(crate::app::ConversationMessage::VideoPreviewReady {
                source: source.clone(),
                result,
            })
        })
    }))
}

pub(super) async fn video_thumbnail(source: PathBuf) -> Result<PathBuf, String> {
    let output =
        std::env::temp_dir().join(format!("snack-video-preview-{}.jpg", uuid::Uuid::new_v4()));
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            "0.1",
            "-i",
            source.to_string_lossy().as_ref(),
            "-frames:v",
            "1",
            "-vf",
            "scale=128:-1",
            output.to_string_lossy().as_ref(),
        ])
        .status()
        .await
        .map_err(|error| format!("start ffmpeg: {error}"))?;
    if status.success() {
        Ok(output)
    } else {
        Err("ffmpeg could not extract a video preview".to_owned())
    }
}

pub(super) fn is_auth_error(e: &SlackError) -> bool {
    matches!(e, SlackError::Api(code) if code == "invalid_auth" || code == "not_authed" || code == "token_revoked")
}

pub(super) fn maybe_send_typing(app: &mut App) {
    if app.composer.text().trim().is_empty() {
        return;
    }
    let (Some(team), Some(channel)) = (app.active_team.clone(), app.active_channel.clone()) else {
        return;
    };
    let now = Instant::now();
    let key = (team.clone(), channel.clone());
    if app
        .last_typing
        .get(&key)
        .is_some_and(|last| now.duration_since(*last) < Duration::from_secs(3))
    {
        return;
    }
    let Some(ws) = app.workspaces.get(&team) else {
        return;
    };
    let RealtimeStatus::Connected(connection) = &ws.rt else {
        return;
    };
    connection.send(realtime::user_typing_frame(&channel));
    app.last_typing.insert(key, now);
}

pub(super) fn mark_latest_visible(app: &mut App, team: &str, channel: &ChannelId) -> Task<Message> {
    let Some(ws) = app.workspaces.get(team) else {
        return Task::none();
    };
    let Some(cm) = ws.messages.get(channel) else {
        return Task::none();
    };
    if !cm.loaded {
        return Task::none();
    }
    let Some(latest) = cm.latest_confirmed_ts() else {
        return Task::none();
    };
    if cm
        .last_read
        .as_ref()
        .is_some_and(|last_read| crate::state::ts_key(last_read) >= crate::state::ts_key(&latest))
    {
        return Task::none();
    }
    if app.live().is_none() {
        return Task::none();
    }
    if !begin_mark(app, team, channel, &latest) {
        return Task::none();
    }
    app.mark_channel_read(team, channel, latest)
}

pub(super) fn mark_latest_thread(
    app: &mut App,
    team: &str,
    channel: &ChannelId,
    root_ts: &MessageTs,
    known_latest: Option<MessageTs>,
) -> Task<Message> {
    let latest = known_latest.or_else(|| {
        app.threads
            .get(&(team.to_owned(), channel.clone(), root_ts.clone()))
            .and_then(ChannelMessages::latest_confirmed_ts)
    });
    let Some(latest) = latest else {
        return Task::none();
    };
    if app.live().is_none() || !begin_thread_mark(app, team, channel, root_ts, &latest) {
        return Task::none();
    }
    app.mark_thread_read(team, channel, root_ts.clone(), latest)
}

pub(in crate::app) fn begin_mark(
    app: &mut App,
    team: &str,
    channel: &ChannelId,
    ts: &MessageTs,
) -> bool {
    begin_read_mark(
        app,
        ReadTarget::Conversation {
            team: team.to_owned(),
            channel: channel.clone(),
        },
        ts,
    )
}

pub(in crate::app) fn begin_thread_mark(
    app: &mut App,
    team: &str,
    channel: &ChannelId,
    root_ts: &MessageTs,
    ts: &MessageTs,
) -> bool {
    begin_read_mark(
        app,
        ReadTarget::Thread {
            team: team.to_owned(),
            channel: channel.clone(),
            root_ts: root_ts.clone(),
        },
        ts,
    )
}

fn begin_read_mark(app: &mut App, target: ReadTarget, ts: &MessageTs) -> bool {
    if app.mark_blocked.contains(&target) {
        return false;
    }
    let key = (target, ts.clone());
    if app.pending_marks.contains(&key) {
        return false;
    }
    app.pending_marks.insert(key);
    true
}

pub(super) fn reconcile_activity_read(
    app: &mut App,
    team: &str,
    channel: &str,
    root_ts: Option<&str>,
) {
    if app.active_team.as_deref() != Some(team) {
        return;
    }
    let mut transitioned = 0_u32;
    for item in &mut app.activity.items {
        let matches = item.channel() == Some(channel)
            && match root_ts {
                Some(root_ts) => item.is_thread() && item.thread_ts() == Some(root_ts),
                None => item.is_conversation(),
            };
        if matches && item.mark_read() {
            transitioned += 1;
        }
    }
    if transitioned > 0 {
        if let Some(count) = app
            .workspaces
            .get_mut(team)
            .and_then(|workspace| workspace.activity_unread_count.as_mut())
        {
            *count = count.saturating_sub(transitioned);
        }
    }
}

pub(in crate::app) fn is_permanent_mark_error(error: &SlackError) -> bool {
    match error {
        SlackError::Api(code) => matches!(
            code.as_str(),
            "channel_not_found" | "is_archived" | "not_in_channel" | "invalid_channel"
        ),
        _ => false,
    }
}
