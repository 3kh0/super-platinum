use super::super::*;

pub(super) fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Runtime(crate::app::RuntimeMessage::Realtime(team, generation, event)) => {
            let cacheable = app
                .workspaces
                .get(&team)
                .is_some_and(|ws| ws.rt_generation == generation)
                && workspace_cacheable_event(&event);
            let activity_pushed = matches!(event, RtEvent::ActivityUpdated(_));
            let dm_message = match &event {
                RtEvent::Message(msg) if msg.ts.is_some() => {
                    msg.channel.clone().map(|channel| (channel, msg.clone()))
                }
                _ => None,
            };
            let notification = apply_realtime(app, &team, generation, event);
            if cacheable {
                mark_workspace_dirty(app, &team);
            }
            let mut tasks = Vec::new();
            if let Some(notification) = notification {
                tasks.push(show_desktop_notification_task(notification));
            }
            if app.active_team.as_deref() == Some(&team) {
                if let Some(channel) = app.active_channel.clone() {
                    tasks.push(hydrate_visible_missing_users(app, &team, &channel));
                    tasks.push(hydrate_visible_channels(app, &team, &channel));
                    tasks.push(mark_latest_visible(app, &team, &channel));
                    tasks.push(load_visible_file_previews(app, &team, &channel));
                    tasks.push(load_visible_avatar_previews(app, &team, &channel));
                    tasks.push(hydrate_visible_emojis(app, &team, &channel));
                    tasks.push(load_visible_emoji_previews(app, &team, &channel));
                }
                if activity_pushed {
                    tasks.push(hydrate_activity_messages(app));
                    tasks.push(refresh_counts(app));
                }
                if let Some((channel, msg)) = dm_message {
                    let is_dm = app
                        .workspaces
                        .get(&team)
                        .and_then(|ws| ws.channels.get(&channel))
                        .map(|c| c.is_im || c.is_mpim)
                        .unwrap_or(false)
                        || app.dms.entries.iter().any(|e| e.id == channel);
                    if app.dms.loaded && is_dm {
                        tasks.push(hydrate_missing_users(
                            app,
                            &team,
                            std::slice::from_ref(&msg),
                        ));
                        tasks.push(load_avatar_previews(app, &team, vec![msg.clone()]));
                        app.dms.touch(&channel, msg);
                    }
                }
            }
            Task::batch(tasks)
        }

        Message::Runtime(crate::app::RuntimeMessage::RtConnected(team, generation, connection)) => {
            if let Some(ws) = app.workspaces.get_mut(&team) {
                ws.rt_generation = generation;
                ws.rt = RealtimeStatus::Connected(connection);
            }
            if app.active_team.as_deref() == Some(&team) {
                if let Some(channel) = app.active_channel.clone() {
                    return refresh_channel_history(app, &team, &channel);
                }
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::RtDisconnected(team, generation)) => {
            if let Some(ws) = app.workspaces.get_mut(&team) {
                if generation >= ws.rt_generation {
                    ws.rt = RealtimeStatus::Disconnected;
                }
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SignInPressed) => authenticate(app, false),

        Message::Runtime(crate::app::RuntimeMessage::AddAccountPressed) => {
            app.account_menu_open = false;
            authenticate(app, true)
        }

        Message::Runtime(crate::app::RuntimeMessage::AuthenticationFinished(true)) => {
            app.load_session()
        }

        Message::Runtime(crate::app::RuntimeMessage::AuthenticationFinished(false)) => {
            app.toast("Slack sign-in was not completed");
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::RetryAuth) => app.load_session(),

        Message::Runtime(crate::app::RuntimeMessage::MainViewSelected(view)) => {
            app.main_view = view;
            if view == crate::state::MainView::Activity {
                let mut tasks = vec![refresh_counts(app)];
                if !app.activity.loaded && !app.activity.loading {
                    tasks.push(load_activity(app, None));
                }
                return Task::batch(tasks);
            }
            if view == crate::state::MainView::Dms {
                let mut tasks = vec![refresh_counts(app)];
                if !app.dms.loaded && !app.dms.loading {
                    tasks.push(load_dms(app, None));
                }
                return Task::batch(tasks);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::ActivityScrolled { remaining }) => {
            activity_scrolled(app, remaining)
        }

        Message::Runtime(crate::app::RuntimeMessage::ActivityUnreadOnlyToggled) => {
            app.activity.unread_only = !app.activity.unread_only;
            app.activity.next_cursor = None;
            Task::batch([load_activity(app, None), refresh_counts(app)])
        }

        Message::Runtime(crate::app::RuntimeMessage::ActivityLoaded {
            team,
            cursor,
            seq,
            result,
        }) => {
            if app.active_team.as_deref() != Some(team.as_str()) || app.activity.load_seq != seq {
                return Task::none();
            }
            app.activity.loading = false;
            match result {
                Ok(page) => {
                    let next_cursor = page
                        .response_metadata
                        .and_then(|metadata| metadata.next_cursor)
                        .filter(|cursor| !cursor.is_empty());
                    if cursor.is_none() {
                        app.activity.items.clear();
                    }
                    for item in page.items {
                        app.activity.upsert(item);
                    }
                    app.activity.next_cursor = next_cursor;
                    app.activity.loaded = true;
                    let continue_cursor = should_auto_load_activity(&app.activity)
                        .then(|| app.activity.next_cursor.clone())
                        .flatten()
                        .filter(|next| cursor.as_deref() != Some(next.as_str()));
                    let authors: Vec<UserId> = app
                        .activity
                        .items
                        .iter()
                        .filter_map(|i| i.author().map(str::to_owned))
                        .collect();
                    let mut tasks = vec![
                        hydrate_activity_messages(app),
                        hydrate_users_by_id(app, &team, authors),
                    ];
                    if let Some(cursor) = continue_cursor {
                        tasks.push(load_activity(app, Some(cursor)));
                    }
                    Task::batch(tasks)
                }
                Err(e) => {
                    app.toast(format!("activity feed failed: {e}"));
                    Task::none()
                }
            }
        }

        Message::Runtime(crate::app::RuntimeMessage::ActivityMessagesLoaded(team, result)) => {
            if app.active_team.as_deref() != Some(team.as_str()) {
                return Task::none();
            }
            match result {
                Ok(page) => {
                    let mut messages = Vec::new();
                    for (channel, data) in page.messages_data {
                        for msg in data.messages {
                            if let Some(ts) = msg.ts.clone() {
                                app.activity
                                    .hydrated
                                    .insert((channel.clone(), ts), msg.clone());
                                messages.push(msg);
                            }
                        }
                    }
                    Task::batch([
                        hydrate_missing_users(app, &team, &messages),
                        hydrate_emojis(app, &team, &messages),
                        load_emoji_previews(app, &team, &messages),
                        load_avatar_previews(app, &team, messages),
                    ])
                }
                Err(e) => {
                    tracing::warn!(error = %e, "activity messages hydration failed");
                    Task::none()
                }
            }
        }

        Message::Runtime(crate::app::RuntimeMessage::ActivitySelected(key)) => {
            app.activity.selected = Some(key.clone());
            let target = app
                .activity
                .items
                .iter()
                .find(|i| i.key == key)
                .and_then(|a| {
                    let channel = a.channel()?.to_owned();
                    let unread_range = a.is_unread.then(|| {
                        a.min_unread_ts()
                            .zip(a.latest_ts())
                            .map(|(oldest, latest)| (oldest.to_owned(), latest.to_owned()))
                    });
                    let unread_range = unread_range.flatten();
                    Some((channel, a.thread_ts().map(str::to_owned), unread_range))
                });
            let Some((channel, thread_ts, unread_range)) = target else {
                return Task::none();
            };
            match thread_ts {
                Some(root_ts) => {
                    let mut task = super::update_inner(
                        app,
                        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
                            channel.clone(),
                        )),
                    );
                    task = task.chain(super::update_inner(
                        app,
                        Message::Conversation(crate::app::ConversationMessage::ThreadOpened {
                            channel,
                            ts: root_ts,
                            unread_range,
                        }),
                    ));
                    task
                }
                None => {
                    app.active_thread = None;
                    app.thread_open = false;
                    super::update_inner(
                        app,
                        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
                            channel,
                        )),
                    )
                }
            }
        }

        Message::Runtime(crate::app::RuntimeMessage::DmsScrolled { remaining }) => {
            dms_scrolled(app, remaining)
        }

        Message::Runtime(crate::app::RuntimeMessage::DmsUnreadOnlyToggled(enabled)) => {
            app.dms.unread_only = enabled;
            refresh_counts(app)
        }

        Message::Runtime(crate::app::RuntimeMessage::DmsFilterChanged(filter)) => {
            app.dms.filter = filter;
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::DmsLoaded {
            team,
            cursor,
            seq,
            result,
        }) => {
            if app.active_team.as_deref() != Some(team.as_str()) || app.dms.load_seq != seq {
                return Task::none();
            }
            app.dms.loading = false;
            match result {
                Ok(page) => {
                    let next_cursor = page
                        .response_metadata
                        .and_then(|metadata| metadata.next_cursor)
                        .filter(|cursor| !cursor.is_empty());
                    if cursor.is_none() {
                        app.dms.entries.clear();
                    }
                    for entry in page.dms {
                        app.dms.upsert(entry);
                    }
                    app.dms.next_cursor = next_cursor;
                    app.dms.loaded = true;
                    let channels: Vec<Channel> = app
                        .dms
                        .entries
                        .iter()
                        .filter_map(|entry| entry.channel.clone())
                        .collect();
                    if let Some(ws) = app.workspaces.get_mut(&team) {
                        ws.apply_channels_info(channels);
                    }
                    mark_workspace_dirty(app, &team);
                    let mut seen = HashSet::new();
                    let counterparts: Vec<UserId> = app
                        .dms
                        .entries
                        .iter()
                        .filter_map(|entry| entry.channel.as_ref())
                        .filter_map(crate::state::dm_user_id)
                        .filter(|user| !user.trim().is_empty())
                        .filter(|user| seen.insert((*user).to_owned()))
                        .map(str::to_owned)
                        .collect();
                    let messages: Vec<SlackMessage> = app
                        .dms
                        .entries
                        .iter()
                        .filter_map(|entry| entry.message.clone())
                        .collect();
                    Task::batch([
                        hydrate_users_by_id(app, &team, counterparts.clone()),
                        load_user_avatar_previews(app, &team, counterparts),
                        hydrate_missing_users(app, &team, &messages),
                        hydrate_emojis(app, &team, &messages),
                        load_emoji_previews(app, &team, &messages),
                        load_avatar_previews(app, &team, messages),
                    ])
                }
                Err(e) => {
                    app.toast(format!("dms failed: {e}"));
                    Task::none()
                }
            }
        }

        Message::Runtime(crate::app::RuntimeMessage::AccountMenuToggled) => {
            if app.account_menu_open {
                app.account_menu_open = false;
            } else {
                app.show_account_menu = true;
                app.account_menu_open = true;
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::AccountMenuDismissed) => {
            if !app.account_menu_open {
                app.show_account_menu = false;
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::AccountSelected(account_id)) => {
            switch_account(app, account_id)
        }

        Message::Runtime(crate::app::RuntimeMessage::SelfPresenceSelected(presence)) => {
            set_self_presence(app, presence)
        }

        Message::Runtime(crate::app::RuntimeMessage::SelfPresenceUpdated {
            team,
            presence,
            previous,
            result,
        }) => {
            if let Err(e) = result {
                if let Some(ws) = app.workspaces.get_mut(&team) {
                    if let Some(previous) = previous {
                        ws.set_presence(ws.self_user_id.clone(), previous);
                    } else {
                        let self_user = ws.self_user_id.clone();
                        ws.presence.remove(&self_user);
                    }
                }
                app.toast(format!("presence update failed: {e}"));
                if is_auth_error(&e) {
                    app.screen = Screen::Login;
                }
                mark_workspace_dirty(app, &team);
            } else {
                tracing::debug!(%team, ?presence, "presence updated");
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SignOutPressed) => sign_out(app),

        Message::Runtime(crate::app::RuntimeMessage::SettingsOpened) => {
            app.account_menu_open = false;
            app.settings_color_errors.clear();
            app.show_settings = true;
            app.settings_open = true;
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsClosed) => {
            app.settings_open = false;
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsDismissed) => {
            if !app.settings_open {
                app.show_settings = false;
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::AgentRequest { id, command }) => {
            crate::app::agent::handle(app, id, command)
        }
        Message::Runtime(crate::app::RuntimeMessage::AgentScreenshotCaptured {
            id,
            path,
            result,
        }) => crate::app::agent::handle_screenshot(id, path, result),

        Message::Runtime(crate::app::RuntimeMessage::SettingsPresetSelected(preset)) => {
            app.settings.preset = preset;
            apply_settings(app);
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsRoleColorChanged(role, value)) => {
            app.settings_color_drafts.insert(role, value.clone());
            let value = value.trim();
            if value.is_empty() {
                app.settings.colors.set(role, None);
                app.settings_color_errors.remove(&role);
                apply_settings(app);
            } else {
                match value.to_owned().try_into() {
                    Ok(color) => {
                        app.settings.colors.set(role, Some(color));
                        app.settings_color_errors.remove(&role);
                        apply_settings(app);
                    }
                    Err(error) => {
                        app.settings_color_errors.insert(role, error);
                    }
                }
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsPresetColorsRestored) => {
            app.settings.colors = config::RoleColorOverrides::default();
            app.settings_color_drafts.clear();
            app.settings_color_errors.clear();
            apply_settings(app);
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBackgroundPickerOpened) => {
            Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Choose a background")
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                        .pick_file()
                        .await
                        .map(|file| file.path().to_owned())
                },
                Message::SettingsBackgroundPicked,
            )
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBackgroundPicked(Some(path))) => {
            Task::perform(import_background(path), Message::SettingsBackgroundImported)
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBackgroundPicked(None)) => {
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBackgroundImported(result)) => {
            match result {
                Ok(background) => {
                    let previous = app.settings.background.replace(background.clone());
                    if apply_settings(app) {
                        if let Some(previous) = previous {
                            remove_managed_background(&previous);
                        }
                    } else {
                        app.settings.background = previous;
                        ui::theme::apply(&app.settings);
                        remove_managed_background(&background);
                    }
                }
                Err(error) => app.toast(error),
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBackgroundFitChanged(fit)) => {
            if let Some(background) = app.settings.background.as_mut() {
                background.fit = fit;
                apply_settings(app);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBackgroundDimChanged(value)) => {
            if let Some(background) = app.settings.background.as_mut() {
                background.dim = value.clamp(0.0, 0.90);
                apply_settings(app);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsSurfaceOpacityChanged(value)) => {
            if let Some(background) = app.settings.background.as_mut() {
                background.surface_opacity = value.clamp(0.65, 1.0);
                apply_settings(app);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBackgroundRemoved) => {
            let previous = app.settings.background.take();
            if apply_settings(app) {
                if let Some(previous) = previous {
                    remove_managed_background(&previous);
                }
            } else {
                app.settings.background = previous;
                ui::theme::apply(&app.settings);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsGapChanged(value)) => {
            app.settings.gap = value;
            apply_settings(app);
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsRadiusChanged(value)) => {
            app.settings.panel_radius = value;
            apply_settings(app);
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsBorderChanged(value)) => {
            app.settings.border_thickness = value;
            apply_settings(app);
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SettingsReset) => {
            let previous = app.settings.clone();
            app.settings = config::Settings::default();
            app.settings_color_drafts.clear();
            app.settings_color_errors.clear();
            if apply_settings(app) {
                if let Some(background) = previous.background {
                    remove_managed_background(&background);
                }
            } else {
                app.settings = previous;
                app.settings_color_drafts = config::ColorRole::ALL
                    .into_iter()
                    .filter_map(|role| {
                        app.settings
                            .colors
                            .get(role)
                            .map(|color| (role, color.as_hex()))
                    })
                    .collect();
                ui::theme::apply(&app.settings);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SidebarResizeStarted) => {
            app.sidebar_resizing = true;
            app.sidebar_resize_prev_x = None;
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SidebarResizeMoved(x)) => {
            if app.sidebar_resizing {
                if let Some(prev) = app.sidebar_resize_prev_x {
                    let next = (app.settings.sidebar_width + (x - prev))
                        .clamp(config::SIDEBAR_WIDTH_MIN, config::SIDEBAR_WIDTH_MAX);
                    app.settings.sidebar_width = next;
                }
                app.sidebar_resize_prev_x = Some(x);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::SidebarResizeEnded) => {
            if app.sidebar_resizing {
                app.sidebar_resizing = false;
                app.sidebar_resize_prev_x = None;
                if let Err(e) = config::save_settings(&app.settings) {
                    app.toast(format!("could not save settings: {e}"));
                }
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::ScrollActivity) => {
            app.scrollbar_visible_until = Some(Instant::now() + Duration::from_millis(700));
            ui::theme::set_scrollbars_visible(true);
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::AnimationTick) => {
            if app
                .scrollbar_visible_until
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                app.scrollbar_visible_until = None;
                ui::theme::set_scrollbars_visible(false);
            }
            Task::none()
        }

        Message::Runtime(crate::app::RuntimeMessage::Tick) => {
            let now = Instant::now();
            if let Some(ws) = app.active_workspace_mut() {
                ws.prune_typing(now, Duration::from_secs(4));
            }
            flush_due_cache(app, now)
        }
        _ => unreachable!("message routed to the wrong runtime reducer"),
    }
}
