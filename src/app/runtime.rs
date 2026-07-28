use super::*;

impl App {
    pub(super) fn empty() -> Self {
        let settings = config::load_settings();
        let settings_color_drafts = config::ColorRole::ALL
            .into_iter()
            .filter_map(|role| {
                settings
                    .colors
                    .get(role)
                    .map(|color| (role, color.as_hex()))
            })
            .collect();
        ui::theme::apply(&settings);
        App {
            screen: Screen::Login,
            accounts: BTreeMap::new(),
            active_account: None,
            account_epoch: 0,
            session: None,
            cache: None,
            client: SlackClient::default(),
            transport: None,
            active_team: None,
            active_channel: None,
            active_thread: None,
            thread_open: false,
            main_view: crate::state::MainView::Home,
            activity: ActivityState::default(),
            dms: DmsState::default(),
            workspaces: BTreeMap::new(),
            threads: HashMap::new(),
            composer: Content::new(),
            thread_composer: Content::new(),
            composer_attachments: Vec::new(),
            thread_composer_attachments: Vec::new(),
            pending_file_messages: Vec::new(),
            attachment_seq: 0,
            editing: None,
            edit_content: Content::new(),
            hovered_message: None,
            profile_pane: None,
            profile_open: false,
            profile_hover: None,
            profile_generation: 0,
            profile_fields: HashMap::new(),
            cursor_position: None,
            search_input: String::new(),
            search: None,
            palette: None,
            palette_open: false,
            errors: Vec::new(),
            send_seq: 0,
            last_typing: HashMap::new(),
            last_active_channels: HashMap::new(),
            file_previews: HashMap::new(),
            image_viewer: None,
            image_viewer_generation: 0,
            avatar_previews: HashMap::new(),
            profile_previews: HashMap::new(),
            emoji_previews: HashMap::new(),
            emoji_animation_started: Instant::now(),
            emoji_hydrated: HashSet::new(),
            channel_hydrated: HashSet::new(),
            avatar_profile_hydrated: HashSet::new(),
            text_selection: None,
            pending_scroll_to: None,
            thread_unread_marker: None,
            chat_paused: HashMap::new(),
            pending_marks: HashSet::new(),
            mark_blocked: HashSet::new(),
            cache_dirty: HashMap::new(),
            cache_saving: HashMap::new(),
            settings,
            settings_color_drafts,
            settings_color_errors: HashMap::new(),
            show_settings: false,
            settings_open: false,
            show_account_menu: false,
            account_menu_open: false,
            sidebar_resizing: false,
            sidebar_resize_prev_x: None,
            scrollbar_visible_until: None,
        }
    }

    pub(super) fn boot() -> (Self, Task<Message>) {
        let mut app = App::empty();
        let task = app.load_session();
        let epoch = app.account_epoch;
        (
            app,
            task.map(move |message| Message::AccountScoped(epoch, Box::new(message))),
        )
    }

    pub(super) fn load_session(&mut self) -> Task<Message> {
        match config::load_accounts() {
            Ok(Some(accounts)) => {
                let active_account = accounts.active_account;
                self.accounts = accounts.sessions;
                self.activate_account(active_account)
            }
            Ok(None) => {
                self.reset_account_runtime();
                self.accounts.clear();
                self.screen = Screen::Login;
                Task::none()
            }
            Err(e) => {
                self.reset_account_runtime();
                self.toast(format!("could not load session: {e}"));
                self.screen = Screen::Login;
                Task::none()
            }
        }
    }

    pub(super) fn activate_account(&mut self, account_id: config::AccountId) -> Task<Message> {
        let Some(session) = self.accounts.get(&account_id).cloned() else {
            self.toast("saved account not found");
            return Task::none();
        };
        let transport = match Transport::new(session.d_cookie.clone()) {
            Ok(transport) => Arc::new(transport),
            Err(e) => {
                self.toast(format!("transport init failed: {e}"));
                return Task::none();
            }
        };
        let adopt_legacy_cache = self.accounts.len() == 1;
        let (cache, cache_error) = match Cache::open_default(&account_id, adopt_legacy_cache) {
            Ok(cache) => (Some(cache), None),
            Err(e) => (None, Some(e.to_string())),
        };

        self.reset_account_runtime();
        self.active_account = Some(account_id);
        self.transport = Some(transport);
        if let Some(error) = cache_error {
            self.toast(format!("cache unavailable: {error}"));
        }
        let mut warm = false;
        for ws in session.workspaces.values() {
            let workspace = cache
                .as_ref()
                .and_then(|cache| match cache.load_workspace(ws) {
                    Ok(workspace) => workspace,
                    Err(e) => {
                        tracing::warn!(team = %ws.team_id, error = %e, "cache load failed");
                        None
                    }
                })
                .unwrap_or_else(|| Workspace::from_session(ws));
            warm |= !workspace.channels.is_empty();
            self.workspaces.insert(ws.team_id.clone(), workspace);
        }
        self.active_team = session.workspaces.keys().next().cloned();
        if warm {
            self.screen = Screen::Main;
            if let Some(team) = self.active_team.clone() {
                self.active_channel = preferred_channel(self, &team);
            }
        } else {
            self.screen = Screen::Loading;
        }
        self.cache = cache;
        self.session = Some(session);
        self.boot_all()
    }

    pub(super) fn reset_account_runtime(&mut self) {
        for attachment in self
            .composer_attachments
            .iter()
            .chain(&self.thread_composer_attachments)
            .chain(
                self.pending_file_messages
                    .iter()
                    .flat_map(|pending| &pending.attachments),
            )
        {
            if let Some(cancel) = &attachment.upload_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        self.account_epoch = self.account_epoch.wrapping_add(1);
        self.active_account = None;
        self.session = None;
        self.cache = None;
        self.transport = None;
        self.active_team = None;
        self.active_channel = None;
        self.active_thread = None;
        self.thread_open = false;
        self.main_view = crate::state::MainView::Home;
        self.activity = ActivityState::default();
        self.dms = DmsState::default();
        self.workspaces.clear();
        self.threads.clear();
        self.composer = Content::new();
        self.thread_composer = Content::new();
        self.composer_attachments.clear();
        self.thread_composer_attachments.clear();
        self.pending_file_messages.clear();
        self.attachment_seq = 0;
        self.editing = None;
        self.edit_content = Content::new();
        self.hovered_message = None;
        self.profile_pane = None;
        self.profile_open = false;
        self.profile_hover = None;
        self.profile_generation = self.profile_generation.wrapping_add(1);
        self.profile_fields.clear();
        self.cursor_position = None;
        self.text_selection = None;
        self.search_input.clear();
        self.search = None;
        self.palette = None;
        self.palette_open = false;
        self.errors.clear();
        self.send_seq = 0;
        self.last_typing.clear();
        self.last_active_channels.clear();
        self.file_previews.clear();
        self.image_viewer = None;
        self.image_viewer_generation = self.image_viewer_generation.wrapping_add(1);
        self.avatar_previews.clear();
        self.profile_previews.clear();
        self.emoji_previews.clear();
        self.emoji_animation_started = Instant::now();
        self.emoji_hydrated.clear();
        self.channel_hydrated.clear();
        self.avatar_profile_hydrated.clear();
        self.pending_scroll_to = None;
        self.thread_unread_marker = None;
        self.chat_paused.clear();
        self.pending_marks.clear();
        self.mark_blocked.clear();
        self.cache_dirty.clear();
        self.cache_saving.clear();
        self.show_settings = false;
        self.settings_open = false;
        self.show_account_menu = false;
        self.account_menu_open = false;
        self.sidebar_resizing = false;
        self.sidebar_resize_prev_x = None;
        self.scrollbar_visible_until = None;
        ui::theme::set_scrollbars_visible(false);
        self.screen = Screen::Login;
    }

    pub(super) fn boot_all(&self) -> Task<Message> {
        let Some((transport, session)) = self.live() else {
            return Task::none();
        };
        let tasks = session.workspaces.values().flat_map(|ws| {
            let boot_transport = transport.clone();
            let boot_client = self.client.clone();
            let boot_ws = ws.clone();
            let team = ws.team_id.clone();
            let boot = Task::perform(
                async move { api::fetch_user_boot(&boot_transport, &boot_client, &boot_ws).await },
                move |result| Message::Workspace(crate::app::WorkspaceMessage::BootLoaded(team.clone(), result)),
            );

            let counts_transport = transport.clone();
            let counts_client = self.client.clone();
            let counts_ws = ws.clone();
            let team = ws.team_id.clone();
            let counts = Task::perform(
                async move { api::fetch_counts(&counts_transport, &counts_client, &counts_ws).await },
                move |result| Message::Workspace(crate::app::WorkspaceMessage::CountsLoaded(team.clone(), result)),
            );

            let dms_transport = transport.clone();
            let dms_client = self.client.clone();
            let dms_ws = ws.clone();
            let team = ws.team_id.clone();
            let dms = Task::perform(
                async move { api::fetch_sidebar_dms(&dms_transport, &dms_client, &dms_ws).await },
                move |result| Message::Workspace(crate::app::WorkspaceMessage::SidebarDmsLoaded(team.clone(), result)),
            );

            let sections_transport = transport.clone();
            let sections_client = self.client.clone();
            let sections_ws = ws.clone();
            let team = ws.team_id.clone();
            let sections = Task::perform(
                async move {
                    api::fetch_channel_sections(&sections_transport, &sections_client, &sections_ws)
                        .await
                },
                move |result| Message::Workspace(crate::app::WorkspaceMessage::ChannelSectionsLoaded(team.clone(), result)),
            );
            [boot, counts, dms, sections]
        });
        Task::batch(tasks)
    }

    pub(super) fn load_history(&self, team: &str, channel: &ChannelId) -> Task<Message> {
        if let Some(anchor) = self.unread_anchor(team, channel) {
            return self.load_history_around(team, channel, anchor);
        }
        self.load_history_page(team, channel, HistoryLoadKind::Latest, None, None)
    }

    pub(super) fn unread_anchor(&self, team: &str, channel: &ChannelId) -> Option<MessageTs> {
        let ws = self.workspaces.get(team)?;
        let unread = ws
            .channels
            .get(channel)
            .map(|c| ws.unread_total(c) > 0)
            .unwrap_or_else(|| {
                ws.messages
                    .get(channel)
                    .is_some_and(|cm| cm.unread_count > 0 || cm.mention_count > 0)
            });
        if !unread {
            return None;
        }
        let anchor = ws.messages.get(channel)?.last_read.clone()?;
        (crate::state::ts_key(&anchor).0 > 0).then_some(anchor)
    }

    pub(super) fn load_history_around(
        &self,
        team: &str,
        channel: &ChannelId,
        anchor: MessageTs,
    ) -> Task<Message> {
        let Some((transport, session)) = self.live() else {
            return Task::none();
        };
        let Some(ws) = session.workspaces.get(team) else {
            return Task::none();
        };
        let transport = transport.clone();
        let client = self.client.clone();
        let ws = ws.clone();
        let team = team.to_owned();
        let channel = channel.clone();
        let fetch_channel = channel.clone();
        Task::perform(
            async move {
                let before = api::fetch_history(
                    &transport,
                    &client,
                    &ws,
                    HistoryArgs {
                        channel: fetch_channel.clone(),
                        latest: Some(anchor.clone()),
                        limit: Some(50),
                        inclusive: true,
                        ..Default::default()
                    },
                )
                .await?;
                let after = api::fetch_history(
                    &transport,
                    &client,
                    &ws,
                    HistoryArgs {
                        channel: fetch_channel,
                        oldest: Some(anchor),
                        limit: Some(50),
                        inclusive: true,
                        ..Default::default()
                    },
                )
                .await?;
                Ok(LoadedHistory {
                    page: merge_history_pages(before, after),
                    replace_cached: false,
                })
            },
            move |result| {
                Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
                    team.clone(),
                    channel.clone(),
                    HistoryLoadKind::Around,
                    result,
                ))
            },
        )
    }

    pub(super) fn load_history_since(
        &self,
        team: &str,
        channel: &ChannelId,
        oldest: MessageTs,
    ) -> Task<Message> {
        const MAX_CATCH_UP_MESSAGES: usize = 200;

        let Some((transport, session)) = self.live() else {
            return Task::none();
        };
        let Some(ws) = session.workspaces.get(team) else {
            return Task::none();
        };
        let transport = transport.clone();
        let client = self.client.clone();
        let ws = ws.clone();
        let team = team.to_owned();
        let channel = channel.clone();
        let fetch_channel = channel.clone();
        Task::perform(
            async move {
                let mut messages = Vec::new();
                let mut cursor = None;
                let replace_cached = loop {
                    let page = api::fetch_history(
                        &transport,
                        &client,
                        &ws,
                        HistoryArgs {
                            channel: fetch_channel.clone(),
                            cursor,
                            oldest: Some(oldest.clone()),
                            limit: Some(50),
                            ..Default::default()
                        },
                    )
                    .await?;
                    messages.extend(page.messages);
                    let next = page
                        .response_metadata
                        .and_then(|metadata| metadata.next_cursor)
                        .filter(|cursor| !cursor.is_empty());
                    if next.is_none() {
                        break false;
                    }
                    if messages.len() >= MAX_CATCH_UP_MESSAGES {
                        messages.truncate(MAX_CATCH_UP_MESSAGES);
                        break true;
                    }
                    cursor = next;
                };
                Ok(LoadedHistory {
                    page: HistoryPage {
                        messages,
                        has_more: replace_cached,
                        ..Default::default()
                    },
                    replace_cached,
                })
            },
            move |result| {
                Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
                    team.clone(),
                    channel.clone(),
                    HistoryLoadKind::Since,
                    result,
                ))
            },
        )
    }

    pub(super) fn load_older_history(
        &self,
        team: &str,
        channel: &ChannelId,
        latest: MessageTs,
    ) -> Task<Message> {
        self.load_history_page(team, channel, HistoryLoadKind::Older, Some(latest), None)
    }

    pub(super) fn load_history_page(
        &self,
        team: &str,
        channel: &ChannelId,
        kind: HistoryLoadKind,
        latest: Option<MessageTs>,
        oldest: Option<MessageTs>,
    ) -> Task<Message> {
        let Some((transport, session)) = self.live() else {
            return Task::none();
        };
        let Some(ws) = session.workspaces.get(team) else {
            return Task::none();
        };
        let transport = transport.clone();
        let client = self.client.clone();
        let ws = ws.clone();
        let team = team.to_owned();
        let channel = channel.clone();
        let args = HistoryArgs {
            channel: channel.clone(),
            latest,
            oldest,
            limit: Some(50),
            ..Default::default()
        };
        Task::perform(
            async move {
                api::fetch_history(&transport, &client, &ws, args)
                    .await
                    .map(|page| LoadedHistory {
                        page,
                        replace_cached: false,
                    })
            },
            move |result| {
                Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
                    team.clone(),
                    channel.clone(),
                    kind,
                    result,
                ))
            },
        )
    }

    pub(super) fn mark_channel_read(
        &self,
        team: &str,
        channel: &ChannelId,
        ts: MessageTs,
    ) -> Task<Message> {
        let Some((transport, session)) = self.live() else {
            return Task::none();
        };
        let Some(ws) = session.workspaces.get(team) else {
            return Task::none();
        };
        let transport = transport.clone();
        let client = self.client.clone();
        let ws = ws.clone();
        let team = team.to_owned();
        let channel = channel.clone();
        let send_channel = channel.clone();
        let mark_ts = ts.clone();
        Task::perform(
            async move { api::mark_channel(&transport, &client, &ws, send_channel, ts).await },
            move |result| {
                Message::Workspace(crate::app::WorkspaceMessage::ChannelMarked(
                    team.clone(),
                    channel.clone(),
                    mark_ts.clone(),
                    result,
                ))
            },
        )
    }

    pub(super) fn mark_thread_read(
        &self,
        team: &str,
        channel: &ChannelId,
        root_ts: MessageTs,
        ts: MessageTs,
    ) -> Task<Message> {
        let Some((transport, session)) = self.live() else {
            return Task::none();
        };
        let Some(ws) = session.workspaces.get(team) else {
            return Task::none();
        };
        let transport = transport.clone();
        let client = self.client.clone();
        let ws = ws.clone();
        let team = team.to_owned();
        let channel = channel.clone();
        let send_channel = channel.clone();
        let send_root_ts = root_ts.clone();
        let mark_ts = ts.clone();
        Task::perform(
            async move {
                api::mark_thread(&transport, &client, &ws, send_channel, send_root_ts, ts).await
            },
            move |result| {
                Message::Conversation(crate::app::ConversationMessage::ThreadMarked {
                    team: team.clone(),
                    channel: channel.clone(),
                    root_ts: root_ts.clone(),
                    ts: mark_ts.clone(),
                    result,
                })
            },
        )
    }

    pub(super) fn load_thread(
        &self,
        team: &str,
        channel: &ChannelId,
        root_ts: &MessageTs,
        unread_range: Option<(MessageTs, MessageTs)>,
    ) -> Task<Message> {
        let Some((transport, session)) = self.live() else {
            return Task::none();
        };
        let Some(ws) = session.workspaces.get(team) else {
            return Task::none();
        };
        let transport = transport.clone();
        let client = self.client.clone();
        let ws = ws.clone();
        let team = team.to_owned();
        let channel = channel.clone();
        let root_ts = root_ts.clone();
        let fetch_channel = channel.clone();
        let fetch_ts = root_ts.clone();
        let (args, unread_anchor) = thread_replies_args(fetch_channel, fetch_ts, unread_range);
        Task::perform(
            async move { api::fetch_replies(&transport, &client, &ws, args).await },
            move |result| {
                Message::Conversation(crate::app::ConversationMessage::ThreadLoaded {
                    team: team.clone(),
                    channel: channel.clone(),
                    root_ts: root_ts.clone(),
                    unread_anchor: unread_anchor.clone(),
                    result,
                })
            },
        )
    }

    pub(super) fn active_workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active_team.as_ref()?)
    }

    pub(super) fn active_workspace_mut(&mut self) -> Option<&mut Workspace> {
        let team = self.active_team.clone()?;
        self.workspaces.get_mut(&team)
    }

    pub(super) fn live(&self) -> Option<(&Arc<Transport>, &Session)> {
        Some((self.transport.as_ref()?, self.session.as_ref()?))
    }

    pub(super) fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        tracing::warn!(%text, "toast");
        self.errors.push(Toast { text });
    }
}

pub(super) fn merge_history_pages(mut before: HistoryPage, after: HistoryPage) -> HistoryPage {
    let mut seen: HashSet<_> = before
        .messages
        .iter()
        .filter_map(|message| message.ts.clone())
        .collect();
    before.messages.extend(
        after
            .messages
            .into_iter()
            .filter(|message| message.ts.as_ref().is_none_or(|ts| seen.insert(ts.clone()))),
    );
    before.has_more |= after.has_more;
    before.latest_updates.extend(after.latest_updates);
    before.unchanged_messages.extend(after.unchanged_messages);
    before
}

pub fn run() -> iced::Result {
    iced::application(App::boot, update, view)
        .subscription(subscription)
        .theme(theme)
        .title("Snack")
        .window(window_settings())
        .run()
}

fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        icon: app_icon(),
        ..iced::window::Settings::default()
    }
}

fn app_icon() -> Option<iced::window::Icon> {
    iced::window::icon::from_file_data(include_bytes!("../../assets/icons/icon-256.png"), None).ok()
}

fn theme(_app: &App) -> iced::Theme {
    ui::theme::snack_theme()
}
