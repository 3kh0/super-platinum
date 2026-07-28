use super::*;

pub(super) fn search_submitted(app: &mut App) -> Task<Message> {
    let query = app.search_input.trim().to_owned();
    if query.is_empty() {
        return Task::none();
    }
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    app.search = Some(SearchState {
        query: query.clone(),
        team: team.clone(),
        page: 1,
        page_count: 0,
        total: 0,
        hits: Vec::new(),
        loading: true,
    });
    run_search(app, &team, query, 1)
}

pub(super) fn search_page_requested(app: &mut App, page: u32) -> Task<Message> {
    let (team, query) = match app.search.as_mut() {
        Some(state) => {
            if page < 1 || (state.page_count > 0 && page > state.page_count) {
                return Task::none();
            }
            state.page = page;
            state.loading = true;
            (state.team.clone(), state.query.clone())
        }
        None => return Task::none(),
    };
    run_search(app, &team, query, page)
}

pub(super) fn run_search(app: &App, team: &str, query: String, page: u32) -> Task<Message> {
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let team = team.to_owned();
    let args = SearchArgs {
        query: query.clone(),
        count: 20,
        page,
    };
    Task::perform(
        async move { api::fetch_search_messages(&transport, &client, &ws_session, args).await },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::SearchLoaded {
                team: team.clone(),
                query: query.clone(),
                page,
                result,
            })
        },
    )
}

pub(super) fn search_hits(ws: &Workspace, response: &SearchMessagesPage) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    for item in &response.items {
        let channel_id = item
            .channel
            .as_ref()
            .map(|c| c.id.clone())
            .or_else(|| item.messages.first().and_then(|m| m.channel.clone()));
        let Some(channel_id) = channel_id else {
            continue;
        };
        let label = ws
            .channels
            .get(&channel_id)
            .map(crate::state::channel_label)
            .or_else(|| item.channel.as_ref().map(crate::state::channel_label))
            .unwrap_or_else(|| channel_id.clone());
        for msg in &item.messages {
            let mut msg = crate::state::visible_message(msg.clone());
            if msg.channel.is_none() {
                msg.channel = Some(channel_id.clone());
            }
            hits.push(SearchHit {
                channel: channel_id.clone(),
                channel_label: label.clone(),
                message: msg,
            });
        }
    }
    hits
}

pub(super) fn open_search_result(
    app: &mut App,
    channel: ChannelId,
    ts: MessageTs,
    thread_ts: Option<MessageTs>,
) -> Task<Message> {
    app.search = None;
    app.main_view = crate::state::MainView::Home;
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    app.active_channel = Some(channel.clone());
    app.last_active_channels
        .insert(team.clone(), channel.clone());
    if let Some(ws) = app.workspaces.get_mut(&team) {
        ws.last_active_channel = Some(channel.clone());
    }
    mark_workspace_dirty(app, &team);
    app.editing = None;
    app.edit_content = Content::new();
    app.pending_scroll_to = Some((channel.clone(), PendingScrollTarget::Message(ts.clone())));

    let mut tasks = vec![mark_latest_visible(app, &team, &channel)];
    let target_loaded = app
        .workspaces
        .get(&team)
        .and_then(|ws| ws.messages.get(&channel))
        .is_some_and(|cm| {
            cm.messages
                .iter()
                .any(|message| message.ts.as_deref() == Some(ts.as_str()))
        });
    if target_loaded {
        tasks.push(refresh_channel_history(app, &team, &channel));
    } else {
        tasks.push(refresh_channel_history_around(app, &team, &channel, ts));
    }
    tasks.push(load_visible_file_previews(app, &team, &channel));
    tasks.push(scroll_to_pending(app, &channel));

    match thread_ts {
        Some(root) => {
            app.active_thread = Some((channel.clone(), root.clone()));
            app.thread_open = true;
            tasks.push(mark_latest_thread(app, &team, &channel, &root, None));
            let needs_thread = !app
                .threads
                .get(&(team.clone(), channel.clone(), root.clone()))
                .map(|cm| cm.loaded)
                .unwrap_or(false);
            if needs_thread && app.transport.is_some() {
                tasks.push(app.load_thread(&team, &channel, &root, None));
            }
        }
        None => {
            app.active_thread = None;
            app.thread_open = false;
        }
    }
    Task::batch(tasks)
}

pub(super) fn palette_toggled(app: &mut App) -> Task<Message> {
    if app.palette_open {
        app.palette_open = false;
        return focus_active_composer(app);
    }
    let entries = app
        .active_workspace()
        .map(|ws| palette::recents(ws, app.active_channel.as_deref()))
        .unwrap_or_default();
    app.palette = Some(PaletteState {
        entries,
        ..PaletteState::default()
    });
    app.palette_open = true;
    let team = app.active_team.clone();
    let avatars = team
        .map(|team| load_palette_avatar_previews(app, &team))
        .unwrap_or_else(Task::none);
    Task::batch([operation::focus(ui::palette::INPUT_ID), avatars])
}

pub(super) fn palette_query_changed(app: &mut App, query: String) -> Task<Message> {
    let Some(ws) = app.active_workspace() else {
        return Task::none();
    };
    let remote = std::collections::BTreeMap::new();
    let entries = if query.trim().is_empty() {
        palette::recents(ws, app.active_channel.as_deref())
    } else {
        palette::rank(ws, &query, &remote)
    };
    let Some(state) = app.palette.as_mut() else {
        return Task::none();
    };
    state.query = query.clone();
    state.remote_channels = remote;
    state.entries = entries;
    state.selected = 0;
    state.remote_seq += 1;
    let seq = state.remote_seq;

    let local_avatars = match app.active_team.clone() {
        Some(team) => load_palette_avatar_previews(app, &team),
        None => Task::none(),
    };

    if query.trim().chars().count() < 2 {
        return local_avatars;
    }
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    let Some(ws_session) = session.workspaces.get(&team) else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let ws_session = ws_session.clone();
    let users_task = {
        let transport = transport.clone();
        let client = client.clone();
        let ws_session = ws_session.clone();
        let team = team.clone();
        let query = query.clone();
        Task::perform(
            async move { api::fetch_users_search(&transport, &client, &ws_session, query).await },
            move |result| {
                Message::Discovery(crate::app::DiscoveryMessage::PaletteRemoteUsersLoaded {
                    team: team.clone(),
                    seq,
                    result,
                })
            },
        )
    };
    let channels_task = Task::perform(
        async move { api::fetch_channels_search(&transport, &client, &ws_session, query).await },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::PaletteRemoteChannelsLoaded {
                team: team.clone(),
                seq,
                result,
            })
        },
    );
    Task::batch([local_avatars, users_task, channels_task])
}

pub(super) fn palette_moved(app: &mut App, delta: isize) {
    let Some(state) = app.palette.as_mut() else {
        return;
    };
    let len = state.entries.len();
    if len == 0 {
        return;
    }
    let cur = state.selected as isize;
    let next = (cur + delta).rem_euclid(len as isize);
    state.selected = next as usize;
}

pub(super) fn palette_activate_selected(app: &mut App) -> Task<Message> {
    let index = app.palette.as_ref().map(|s| s.selected).unwrap_or(0);
    palette_activate(app, index)
}

pub(super) fn palette_activate(app: &mut App, index: usize) -> Task<Message> {
    let Some(entry) = app
        .palette
        .as_ref()
        .and_then(|s| s.entries.get(index).cloned())
    else {
        return Task::none();
    };
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    app.main_view = crate::state::MainView::Home;
    match entry.target {
        PaletteTarget::Channel(id) => {
            if let Some(remote) = app
                .palette
                .as_ref()
                .and_then(|s| s.remote_channels.get(&id).cloned())
            {
                if let Some(ws) = app.workspaces.get_mut(&team) {
                    ws.channels.entry(id.clone()).or_insert(remote);
                }
            }
            app.palette_open = false;
            app.palette = None;
            Task::done(Message::Conversation(
                crate::app::ConversationMessage::ChannelSelected(id),
            ))
        }
        PaletteTarget::User { dm: Some(id), .. } => {
            app.palette_open = false;
            app.palette = None;
            Task::done(Message::Conversation(
                crate::app::ConversationMessage::ChannelSelected(id),
            ))
        }
        PaletteTarget::User { user, dm: None } => {
            app.palette_open = false;
            app.palette = None;
            let Some((transport, session)) = app.live() else {
                return Task::none();
            };
            let Some(ws_session) = session.workspaces.get(&team) else {
                return Task::none();
            };
            let transport = transport.clone();
            let client = app.client.clone();
            let ws_session = ws_session.clone();
            let dm_user = user.clone();
            Task::perform(
                async move { api::open_dm(&transport, &client, &ws_session, dm_user).await },
                move |result| {
                    Message::Discovery(crate::app::DiscoveryMessage::DmOpened {
                        team: team.clone(),
                        user: user.clone(),
                        result,
                    })
                },
            )
        }
    }
}

pub(super) fn palette_remote_users_loaded(
    app: &mut App,
    team: TeamId,
    seq: u64,
    result: Result<Vec<crate::slack::models::User>, SlackError>,
) -> Task<Message> {
    if app.active_team.as_deref() != Some(&team) {
        return Task::none();
    }
    let users = match result {
        Ok(users) => users,
        Err(e) => {
            tracing::debug!(error = %e, "palette user search failed");
            return Task::none();
        }
    };
    if app.palette.as_ref().map(|s| s.remote_seq) != Some(seq) {
        return Task::none();
    }
    if let Some(ws) = app.workspaces.get_mut(&team) {
        for user in users {
            merge_searched_user(ws, user);
        }
    }
    mark_workspace_dirty(app, &team);
    rerank_palette(app, &team);
    load_palette_avatar_previews(app, &team)
}

pub(super) fn palette_remote_channels_loaded(
    app: &mut App,
    team: TeamId,
    seq: u64,
    result: Result<Vec<Channel>, SlackError>,
) -> Task<Message> {
    if app.active_team.as_deref() != Some(&team) {
        return Task::none();
    }
    let channels = match result {
        Ok(channels) => channels,
        Err(e) => {
            tracing::debug!(error = %e, "palette channel search failed");
            return Task::none();
        }
    };
    if app.palette.as_ref().map(|s| s.remote_seq) != Some(seq) {
        return Task::none();
    }

    let mut known_updates = Vec::new();
    let mut remote_only = Vec::new();
    if let Some(ws) = app.workspaces.get(&team) {
        for channel in channels {
            if channel.is_archived || channel.is_im {
                continue;
            }
            if ws.channels.contains_key(&channel.id) {
                known_updates.push(channel);
            } else {
                remote_only.push(channel);
            }
        }
    } else {
        remote_only = channels
            .into_iter()
            .filter(|c| !c.is_archived && !c.is_im)
            .collect();
    }

    if let Some(ws) = app.workspaces.get_mut(&team) {
        for channel in known_updates {
            if let Some(existing) = ws.channels.get_mut(&channel.id) {
                if !channel.previous_names.is_empty() {
                    existing.previous_names = channel.previous_names;
                }
                if existing.name.as_ref().is_none_or(|n| n.trim().is_empty()) {
                    if channel.name.as_ref().is_some_and(|n| !n.trim().is_empty()) {
                        existing.name = channel.name;
                    }
                }
            }
        }
    }
    if let Some(state) = app.palette.as_mut() {
        for channel in remote_only {
            state.remote_channels.insert(channel.id.clone(), channel);
        }
    }
    mark_workspace_dirty(app, &team);
    rerank_palette(app, &team);
    Task::none()
}

pub(super) fn rerank_palette(app: &mut App, team: &str) {
    let Some(state) = app.palette.as_ref() else {
        return;
    };
    let query = state.query.clone();
    let remote = state.remote_channels.clone();
    let active = app.active_channel.clone();
    let Some(ws) = app.workspaces.get(team) else {
        return;
    };
    let entries = if query.trim().is_empty() {
        palette::recents(ws, active.as_deref())
    } else {
        palette::rank(ws, &query, &remote)
    };
    if let Some(state) = app.palette.as_mut() {
        state.entries = entries;
        if state.selected >= state.entries.len() {
            state.selected = 0;
        }
    }
}

pub(super) fn merge_searched_user(ws: &mut Workspace, incoming: crate::slack::models::User) {
    match ws.users.get_mut(&incoming.id) {
        Some(existing) if crate::state::user_avatar_url(existing).is_none() => {
            if crate::state::user_avatar_url(&incoming).is_some() {
                existing.profile = incoming.profile;
            }
        }
        Some(_) => {}
        None => {
            ws.users.insert(incoming.id.clone(), incoming);
        }
    }
}

pub(super) fn load_palette_avatar_previews(app: &mut App, team: &str) -> Task<Message> {
    let Some(state) = app.palette.as_ref() else {
        return Task::none();
    };
    let users: Vec<UserId> = state
        .entries
        .iter()
        .filter_map(|entry| match &entry.target {
            PaletteTarget::User { user, .. } => Some(user.clone()),
            PaletteTarget::Channel(_) => None,
        })
        .collect();
    if users.is_empty() {
        return Task::none();
    }
    if let Some(crate::state::RealtimeStatus::Connected(conn)) =
        app.workspaces.get(team).map(|ws| &ws.rt)
    {
        conn.send(crate::slack::realtime::presence_query_frame(&users));
    }
    load_user_avatar_previews(app, team, users)
}

pub(super) fn dm_opened(
    app: &mut App,
    team: TeamId,
    user: UserId,
    result: Result<ChannelId, SlackError>,
) -> Task<Message> {
    let channel = match result {
        Ok(channel) => channel,
        Err(e) => {
            app.toast(format!("could not open DM: {e}"));
            return Task::none();
        }
    };
    if let Some(ws) = app.workspaces.get_mut(&team) {
        ws.channels
            .entry(channel.clone())
            .or_insert_with(|| Channel {
                id: channel.clone(),
                is_im: true,
                user: Some(user.clone()),
                ..Default::default()
            });
        if !ws.dm_order.iter().any(|id| id == &channel) {
            ws.dm_order.push(channel.clone());
        }
    }
    mark_workspace_dirty(app, &team);
    Task::done(Message::Conversation(
        crate::app::ConversationMessage::ChannelSelected(channel),
    ))
}

pub(super) fn schedule_profile_hover_dismiss(app: &mut App) -> Task<Message> {
    app.profile_generation = app.profile_generation.wrapping_add(1);
    let generation = app.profile_generation;
    if let Some(hover) = app.profile_hover.as_mut() {
        hover.generation = generation;
    }
    Task::perform(
        async move { tokio::time::sleep(Duration::from_millis(140)).await },
        move |_| {
            Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverDismissReady(
                generation,
            ))
        },
    )
}

pub(super) fn profile_pressed(app: &mut App, user: UserId) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    app.profile_hover = None;
    app.profile_open = true;
    app.profile_pane = Some(ProfilePaneState {
        user: user.clone(),
        loading: true,
        error: None,
    });
    if let Some(crate::state::RealtimeStatus::Connected(connection)) =
        app.workspaces.get(&team).map(|workspace| &workspace.rt)
    {
        connection.send(crate::slack::realtime::presence_query_frame(
            std::slice::from_ref(&user),
        ));
    }

    let Some((transport, session)) = app.live() else {
        if let Some(profile) = app.profile_pane.as_mut() {
            profile.loading = false;
        }
        return load_profile_image(app, &team, &user);
    };
    let Some(workspace) = session.workspaces.get(&team).cloned() else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    let schema_workspace = workspace.clone();
    let request_user = user.clone();
    let result_team = team.clone();
    let result_user = user.clone();
    let profile_task = Task::perform(
        async move { api::fetch_user_profile(&transport, &client, &workspace, request_user).await },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::ProfileLoaded {
                team: result_team.clone(),
                user: result_user.clone(),
                result,
            })
        },
    );
    let extras_transport = app.transport.as_ref().unwrap().clone();
    let extras_client = app.client.clone();
    let extras_workspace = schema_workspace.clone();
    let extras_team = team.clone();
    let extras_user = user.clone();
    let requested_extras_user = user.clone();
    let extras_task = Task::perform(
        async move {
            api::fetch_user_profile_extras(
                &extras_transport,
                &extras_client,
                &extras_workspace,
                requested_extras_user,
            )
            .await
        },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::ProfileExtrasLoaded {
                team: extras_team.clone(),
                user: extras_user.clone(),
                result,
            })
        },
    );
    if app.profile_fields.contains_key(&team) {
        return Task::batch([profile_task, extras_task]);
    }
    let transport = app.transport.as_ref().unwrap().clone();
    let client = app.client.clone();
    let fields_team = team.clone();
    let fields_task = Task::perform(
        async move { api::fetch_team_profile_fields(&transport, &client, &schema_workspace).await },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::ProfileFieldsLoaded {
                team: fields_team.clone(),
                result,
            })
        },
    );
    Task::batch([profile_task, extras_task, fields_task])
}

pub(super) fn profile_loaded(
    app: &mut App,
    team: TeamId,
    user: UserId,
    result: Result<crate::slack::models::UserProfile, SlackError>,
) -> Task<Message> {
    if app.active_team.as_deref() != Some(team.as_str())
        || app.profile_pane.as_ref().map(|pane| pane.user.as_str()) != Some(user.as_str())
    {
        return Task::none();
    }
    if let Some(pane) = app.profile_pane.as_mut() {
        pane.loading = false;
    }
    match result {
        Ok(profile) => {
            if let Some(workspace) = app.workspaces.get_mut(&team) {
                let entry = workspace.users.entry(user.clone()).or_insert_with(|| {
                    crate::slack::models::User {
                        id: user.clone(),
                        ..Default::default()
                    }
                });
                entry.profile = Some(profile);
            }
            mark_workspace_dirty(app, &team);
        }
        Err(error) => {
            tracing::debug!(%team, %user, %error, "profile details failed");
            if let Some(pane) = app.profile_pane.as_mut() {
                pane.error = Some("Some profile details could not be loaded.".into());
            }
        }
    }
    load_profile_image(app, &team, &user)
}

pub(super) fn load_profile_image(app: &mut App, team: &str, user: &str) -> Task<Message> {
    if app.profile_previews.contains_key(user) {
        return Task::none();
    }
    let Some(url) = app
        .workspaces
        .get(team)
        .and_then(|workspace| workspace.users.get(user))
        .and_then(crate::state::user_profile_image_url)
        .map(str::to_owned)
    else {
        return Task::none();
    };
    let Some(transport) = app.transport.clone() else {
        return Task::none();
    };
    app.profile_previews
        .insert(user.to_owned(), FilePreview::Loading);
    let user_agent = crate::slack::xparams::Identity::from_capture().user_agent;
    let user = user.to_owned();
    Task::perform(
        async move { transport.get_bytes(&url, &user_agent).await },
        move |result| {
            Message::Workspace(crate::app::WorkspaceMessage::ProfileImageLoaded {
                user: user.clone(),
                result,
            })
        },
    )
}

pub(super) fn open_profile_dm(app: &mut App, user: UserId) -> Task<Message> {
    let Some(team) = app.active_team.clone() else {
        return Task::none();
    };
    if let Some(channel) = app.workspaces.get(&team).and_then(|workspace| {
        workspace
            .channels
            .values()
            .find(|channel| channel.is_im && channel.user.as_deref() == Some(user.as_str()))
            .map(|channel| channel.id.clone())
    }) {
        app.profile_open = false;
        return Task::done(Message::Conversation(
            crate::app::ConversationMessage::ChannelSelected(channel),
        ));
    }
    let Some((transport, session)) = app.live() else {
        return Task::none();
    };
    let Some(workspace) = session.workspaces.get(&team).cloned() else {
        return Task::none();
    };
    let transport = transport.clone();
    let client = app.client.clone();
    app.profile_open = false;
    let request_user = user.clone();
    let result_team = team.clone();
    let result_user = user.clone();
    Task::perform(
        async move { api::open_dm(&transport, &client, &workspace, request_user).await },
        move |result| {
            Message::Discovery(crate::app::DiscoveryMessage::DmOpened {
                team: result_team.clone(),
                user: result_user.clone(),
                result,
            })
        },
    )
}
