use super::super::*;

pub(super) fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Workspace(crate::app::WorkspaceMessage::BootLoaded(team, result)) => {
            match result {
                Ok(boot) => {
                    let mut self_user = None;
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        ws.apply_boot(boot);
                        self_user = Some(ws.self_user_id.clone());
                        tracing::info!(%team, channels = ws.channels.len(), "boot ok");
                    }
                    mark_workspace_dirty(app, &team);
                    app.screen = Screen::Main;
                    let mut tasks = vec![
                        hydrate_sidebar_channels(app, &team),
                        hydrate_sidebar_dm_users(app, &team),
                    ];
                    if let Some(self_user) = self_user {
                        tasks.push(load_user_avatar_previews(app, &team, vec![self_user]));
                    }
                    if app.active_team.as_deref() == Some(&team) {
                        tasks.push(load_activity(app, None));
                    }
                    if app.active_team.as_deref() == Some(&team) && app.active_channel.is_none() {
                        if let Some(channel) = preferred_channel(app, &team) {
                            app.active_channel = Some(channel.clone());
                            if let Some(ws) = app.workspaces.get_mut(&team) {
                                ws.last_active_channel = Some(channel.clone());
                            }
                            app.pending_scroll_to =
                                channel_open_scroll_target(app, &team, &channel)
                                    .map(|target| (channel.clone(), target));
                            tasks.push(refresh_channel_history(app, &team, &channel));
                        }
                    }
                    if let Some(channel) = app
                        .active_channel
                        .clone()
                        .filter(|_| app.active_team.as_deref() == Some(&team))
                    {
                        tasks.extend([
                            mark_latest_visible(app, &team, &channel),
                            hydrate_visible_missing_users(app, &team, &channel),
                            load_visible_file_previews(app, &team, &channel),
                            load_visible_avatar_previews(app, &team, &channel),
                            hydrate_visible_emojis(app, &team, &channel),
                            load_visible_emoji_previews(app, &team, &channel),
                        ]);
                    }
                    Task::batch(tasks)
                }
                Err(e) => {
                    app.toast(format!("boot failed for {team}: {e}"));
                    if is_auth_error(&e) {
                        app.screen = Screen::Login;
                    }
                    Task::none()
                }
            }
        }

        Message::Workspace(crate::app::WorkspaceMessage::CountsLoaded(team, result)) => {
            match result {
                Ok(counts) => {
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        ws.apply_counts(counts);
                    }
                    mark_workspace_dirty(app, &team);
                    return Task::batch([
                        hydrate_sidebar_channels(app, &team),
                        hydrate_sidebar_dm_users(app, &team),
                    ]);
                }
                Err(e) => tracing::warn!(%team, error = %e, "counts failed"),
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ChannelSectionsLoaded(team, result)) => {
            match result {
                Ok(page) => {
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        ws.apply_channel_sections(page);
                    }
                    mark_workspace_dirty(app, &team);
                }
                Err(e) => tracing::warn!(%team, error = %e, "channel sections failed"),
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::SidebarDmsLoaded(team, result)) => {
            match result {
                Ok(dms) => {
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        ws.apply_sidebar_dms(dms);
                    }
                    mark_workspace_dirty(app, &team);
                    return hydrate_sidebar_dm_users(app, &team);
                }
                Err(e) => tracing::warn!(%team, error = %e, "sidebar.dms failed"),
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
            team,
            channel,
            kind,
            result,
        )) => {
            match result {
                Ok(loaded) => {
                    let replace_cached = loaded.replace_cached;
                    let page = loaded.page;
                    let has_more = page.has_more;
                    let messages: Vec<_> = page
                        .messages
                        .into_iter()
                        .map(crate::state::visible_message)
                        .collect();
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        let cm = ws.messages.entry(channel.clone()).or_default();
                        let n = messages.len();
                        if replace_cached {
                            cm.messages.retain(|message| {
                                message.ts.as_deref().is_some_and(|ts| {
                                    cm.pending.iter().any(|pending| pending == ts)
                                })
                            });
                            cm.has_more_older = true;
                        }
                        for msg in messages.clone() {
                            if crate::state::is_channel_timeline_visible(&msg) {
                                cm.upsert(msg);
                            }
                        }
                        cm.loaded = true;
                        cm.history_failed = false;
                        if kind != HistoryLoadKind::Older {
                            cm.history_refreshing = false;
                        }
                        if matches!(
                            kind,
                            HistoryLoadKind::Latest
                                | HistoryLoadKind::Around
                                | HistoryLoadKind::Older
                        ) {
                            cm.has_more_older = has_more;
                        }
                        if kind == HistoryLoadKind::Older {
                            cm.history_loading_older = false;
                        }
                        tracing::info!(%channel, messages = n, "history loaded");
                    }
                    mark_workspace_dirty(app, &team);
                    let is_active = app.active_team.as_deref() == Some(team.as_str())
                        && app.active_channel.as_deref() == Some(channel.as_str());
                    if !is_active {
                        return Task::none();
                    }
                    let mut tasks = vec![
                        hydrate_missing_users(app, &team, &messages),
                        hydrate_message_channels(app, &team, &messages),
                        hydrate_emojis(app, &team, &messages),
                        load_avatar_previews(app, &team, messages),
                        load_visible_file_previews(app, &team, &channel),
                        load_visible_emoji_previews(app, &team, &channel),
                        scroll_to_pending(app, &channel),
                    ];
                    if kind != HistoryLoadKind::Older {
                        tasks.push(mark_latest_visible(app, &team, &channel));
                    }
                    return Task::batch(tasks);
                }
                Err(e) => {
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        let cm = ws.messages.entry(channel.clone()).or_default();
                        cm.history_failed = true;
                        if kind == HistoryLoadKind::Older {
                            cm.history_loading_older = false;
                        } else {
                            cm.history_refreshing = false;
                        }
                    }
                    if kind == HistoryLoadKind::Older
                        && app
                            .pending_scroll_to
                            .as_ref()
                            .is_some_and(|(pending_channel, _)| pending_channel == &channel)
                    {
                        app.pending_scroll_to = None;
                    }
                    app.toast(format!("history failed for {channel}: {e}"));
                }
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ChannelScrolled {
            channel,
            y,
            bottom_gap,
        }) => channel_scrolled(app, channel, y, bottom_gap),

        Message::Workspace(crate::app::WorkspaceMessage::ChatResumePressed(channel)) => {
            app.chat_paused.remove(&channel);
            operation::scroll_to(
                ui::channel::scrollable_id(&channel),
                AbsoluteOffset { x: 0.0, y: 0.0 },
            )
        }

        Message::Workspace(crate::app::WorkspaceMessage::ChannelMarked(
            team,
            channel,
            ts,
            result,
        )) => {
            let target = ReadTarget::Conversation {
                team: team.clone(),
                channel: channel.clone(),
            };
            app.pending_marks.remove(&(target.clone(), ts.clone()));
            match result {
                Ok(()) => {
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        apply_channel_marked(ws, &channel, &ts, 0, 0);
                    }
                    reconcile_activity_read(app, &team, &channel, None);
                    mark_workspace_dirty(app, &team);
                }
                Err(e) => {
                    if is_permanent_mark_error(&e) {
                        app.mark_blocked.insert(target);
                        tracing::warn!(
                            %team,
                            %channel,
                            error = %e,
                            "mark failed permanently; blocking further attempts this session"
                        );
                    } else {
                        tracing::warn!(%team, %channel, error = %e, "mark failed");
                    }
                }
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::EditPressed { channel, ts }) => {
            let current = find_message_text(app, &channel, &ts).unwrap_or_default();
            app.edit_content = Content::with_text(&current);
            app.editing = Some((channel, ts));
            operation::focus(ui::composer::EDIT_INPUT_ID)
        }

        Message::Workspace(crate::app::WorkspaceMessage::EditCancelled) => {
            app.editing = None;
            app.edit_content = Content::new();
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::EditSubmit) => edit_submit(app),

        Message::Workspace(crate::app::WorkspaceMessage::CopyMessage(text)) => {
            iced::clipboard::write(text).discard()
        }

        Message::Workspace(crate::app::WorkspaceMessage::TextSelectionStarted(point)) => {
            app.text_selection = Some(TextSelection {
                anchor: point.clone(),
                focus: point,
                dragging: true,
            });
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::TextSelectionDragged(point)) => {
            if let Some(selection) = app
                .text_selection
                .as_mut()
                .filter(|selection| selection.dragging && selection.anchor.surface == point.surface)
            {
                selection.focus = point;
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::TextSelectionEnded) => {
            if let Some(selection) = app.text_selection.as_mut() {
                if selection.anchor == selection.focus {
                    app.text_selection = None;
                } else {
                    selection.dragging = false;
                }
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::TextSelectionCopyRequested) => {
            match selected_text(app) {
                Some(text) if !text.is_empty() => iced::clipboard::write(text).discard(),
                _ => Task::none(),
            }
        }

        Message::Workspace(crate::app::WorkspaceMessage::MessageHovered { in_thread, ts }) => {
            app.hovered_message = Some((in_thread, ts));
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::MessageUnhovered) => {
            app.hovered_message = None;
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfilePressed(user)) => {
            profile_pressed(app, user)
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileDismissed) => {
            app.profile_open = false;
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfilePaneDismissed) => {
            if !app.profile_open {
                app.profile_pane = None;
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileSeeAllConversations(user)) => {
            let name = app
                .active_workspace()
                .map(|workspace| workspace.display_name(&user))
                .unwrap_or(user);
            app.profile_open = false;
            app.main_view = crate::state::MainView::Dms;
            app.dms.filter = name;
            app.active_thread = None;
            app.thread_open = false;
            if app.dms.loaded || app.dms.loading {
                Task::none()
            } else {
                load_dms(app, None)
            }
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverEntered { user, key }) => {
            app.profile_generation = app.profile_generation.wrapping_add(1);
            let generation = app.profile_generation;
            app.profile_hover = Some(ProfileHoverState {
                user: user.clone(),
                key: key.clone(),
                generation,
                visible: false,
                source_hovered: true,
                card_hovered: false,
                position: app.cursor_position,
            });
            Task::perform(
                async move { tokio::time::sleep(Duration::from_millis(900)).await },
                move |_| {
                    Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverReady {
                        user: user.clone(),
                        key: key.clone(),
                        generation,
                    })
                },
            )
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverExited { user, key }) => {
            let matches = app
                .profile_hover
                .as_ref()
                .is_some_and(|hover| hover.user == user && hover.key == key);
            if !matches {
                return Task::none();
            }
            if let Some(hover) = app.profile_hover.as_mut() {
                hover.source_hovered = false;
            }
            schedule_profile_hover_dismiss(app)
        }

        Message::Workspace(crate::app::WorkspaceMessage::CursorMoved(position)) => {
            app.cursor_position = Some(position);
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverReady {
            user,
            key,
            generation,
        }) => {
            if let Some(hover) = app.profile_hover.as_mut().filter(|hover| {
                hover.user == user
                    && hover.key == key
                    && hover.generation == generation
                    && hover.source_hovered
            }) {
                // MouseArea enter can be delivered before the global cursor event.
                // Anchor at reveal time, when the latest window-space point is known.
                hover.position = app.cursor_position;
                hover.visible = hover.position.is_some();
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileHoverDismissReady(generation)) => {
            if app.profile_hover.as_ref().is_some_and(|hover| {
                hover.generation == generation && !hover.source_hovered && !hover.card_hovered
            }) {
                app.profile_hover = None;
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileCardEntered) => {
            app.profile_generation = app.profile_generation.wrapping_add(1);
            if let Some(hover) = app.profile_hover.as_mut() {
                hover.generation = app.profile_generation;
                hover.card_hovered = true;
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileCardExited) => {
            if let Some(hover) = app.profile_hover.as_mut() {
                hover.card_hovered = false;
            }
            schedule_profile_hover_dismiss(app)
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileLoaded { team, user, result }) => {
            profile_loaded(app, team, user, result)
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileExtrasLoaded {
            team,
            user,
            result,
        }) => {
            if let Ok(extras) = result
                && let Some(member) = app
                    .workspaces
                    .get_mut(&team)
                    .and_then(|workspace| workspace.users.get_mut(&user))
            {
                member.im_mpim_ids = extras.im_mpim_ids;
                member.has_more_mpims = extras.has_more_mpims;
                mark_workspace_dirty(app, &team);
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileFieldsLoaded { team, result }) => {
            match result {
                Ok(fields) => {
                    app.profile_fields.insert(team, fields);
                }
                Err(error) => {
                    tracing::debug!(%team, %error, "workspace profile schema failed");
                }
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileImageLoaded { user, result }) => {
            match result {
                Ok(bytes) => {
                    app.profile_previews
                        .insert(user, FilePreview::Loaded(ImageHandle::from_bytes(bytes)));
                }
                Err(error) => {
                    tracing::debug!(%user, %error, "profile image failed");
                    app.profile_previews.insert(user, FilePreview::Failed);
                }
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ProfileMessagePressed(user)) => {
            open_profile_dm(app, user)
        }

        Message::Workspace(crate::app::WorkspaceMessage::MessageEdited {
            team,
            channel,
            ts,
            result,
        }) => {
            match result {
                Ok(sent) => {
                    apply_message_edit(app, &team, &channel, &ts, sent.message.text);
                    mark_workspace_dirty(app, &team);
                }
                Err(e) => {
                    app.toast(format!("edit failed: {e}"));
                    if is_auth_error(&e) {
                        app.screen = Screen::Login;
                    }
                }
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::DeletePressed { channel, ts }) => {
            delete_pressed(app, channel, ts)
        }

        Message::Workspace(crate::app::WorkspaceMessage::MessageDeleted {
            team,
            channel,
            ts,
            result,
        }) => {
            match result {
                Ok(()) => {
                    remove_message_everywhere(app, &team, &channel, &ts);
                    mark_workspace_dirty(app, &team);
                }
                Err(e) => {
                    app.toast(format!("delete failed: {e}"));
                    if is_auth_error(&e) {
                        app.screen = Screen::Login;
                    }
                }
            }
            Task::none()
        }

        Message::Workspace(crate::app::WorkspaceMessage::ReactionPressed { channel, ts, name }) => {
            toggle_reaction(app, channel, ts, name)
        }

        Message::Workspace(crate::app::WorkspaceMessage::ReactionUpdated {
            team,
            channel,
            ts,
            user,
            name,
            added,
            result,
        }) => {
            if let Err(e) = result {
                app.toast(format!("reaction failed: {e}"));
                if let Some(cm) = app
                    .workspaces
                    .get_mut(&team)
                    .and_then(|ws| ws.messages.get_mut(&channel))
                {
                    cm.apply_reaction(&ts, &user, &name, !added);
                }
                apply_thread_reaction(&mut app.threads, &team, &channel, &ts, &user, &name, !added);
                if is_auth_error(&e) {
                    app.screen = Screen::Login;
                }
                mark_workspace_dirty(app, &team);
            }
            Task::none()
        }

        _ => unreachable!("message routed to the wrong workspace reducer"),
    }
}
