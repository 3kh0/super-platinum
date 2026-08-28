use crate::state::{ChannelOpen, MainView, ShellState};

impl ShellState {
    pub fn set_theme_preset(&mut self, value: &str) {
        self.core.settings.preset = match value {
            "blue_steel" => super_platinum_core::config::ThemePreset::BlueSteel,
            "paper_bag" => super_platinum_core::config::ThemePreset::PaperBag,
            _ => super_platinum_core::config::ThemePreset::Countertop,
        };
    }

    pub fn set_density(&mut self, value: &str) {
        self.core.settings.gap = if value == "compact" { 4.0 } else { 8.0 };
    }

    pub fn set_sidebar_width(&mut self, width: f32) {
        self.core.settings.sidebar_width = width.clamp(
            super_platinum_core::config::SIDEBAR_WIDTH_MIN,
            super_platinum_core::config::SIDEBAR_WIDTH_MAX,
        );
    }

    pub fn set_panel_radius(&mut self, value: f32) {
        self.core.settings.panel_radius = value.clamp(0.0, 20.0);
    }

    pub fn set_border_thickness(&mut self, value: f32) {
        self.core.settings.border_thickness = value.clamp(0.0, 4.0);
    }

    pub fn set_role_color(&mut self, role: super_platinum_core::config::ColorRole, value: &str) {
        if let Ok(color) = value.to_owned().try_into() {
            self.core.settings.colors.set(role, Some(color));
        }
    }

    pub fn reset_role_colors(&mut self) {
        self.core.settings.colors = super_platinum_core::config::RoleColorOverrides::default();
    }

    pub fn set_background_fit(&mut self, value: &str) {
        if let Some(background) = self.core.settings.background.as_mut() {
            background.fit = if value == "contain" {
                super_platinum_core::config::BackgroundFit::Contain
            } else {
                super_platinum_core::config::BackgroundFit::Cover
            };
        }
    }

    pub fn set_background_dim(&mut self, value: f32) {
        if let Some(background) = self.core.settings.background.as_mut() {
            background.dim = value.clamp(0.0, 1.0);
        }
    }

    pub fn set_surface_opacity(&mut self, value: f32) {
        if let Some(background) = self.core.settings.background.as_mut() {
            background.surface_opacity = value.clamp(0.0, 1.0);
        }
    }

    pub fn set_settings_section(&mut self, section: crate::state::SettingsSection) {
        self.settings_section = section;
    }

    pub fn toggle_self_menu(&mut self) {
        if self.overlay == Some(crate::state::Overlay::SelfMenu) {
            self.overlay = None;
            self.self_menu_notifications_open = false;
        } else {
            self.overlay = Some(crate::state::Overlay::SelfMenu);
            self.self_menu_notifications_open = false;
        }
    }

    pub fn open_preferences(&mut self) {
        self.self_menu_notifications_open = false;
        self.settings_section = crate::state::SettingsSection::Appearance;
        self.overlay = Some(crate::state::Overlay::Settings);
    }

    pub fn toggle_self_menu_notifications(&mut self) {
        self.self_menu_notifications_open = !self.self_menu_notifications_open;
    }

    pub fn apply_self_presence(&mut self, presence: crate::state::PresenceVm) {
        let user = self.self_account.user_id.clone();
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
        {
            workspace.set_presence(
                user,
                match presence {
                    crate::state::PresenceVm::Active => {
                        super_platinum_core::state::Presence::Active
                    }
                    _ => super_platinum_core::state::Presence::Away,
                },
            );
        }
        self.self_account.presence = presence;
    }

    pub fn apply_self_snooze_minutes(&mut self, minutes: Option<u32>) {
        let now = super_platinum_core::state::now_secs();
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
        {
            match minutes {
                Some(minutes) => {
                    workspace.self_dnd.snooze_enabled = true;
                    workspace.self_dnd.snooze_endtime = Some(now + i64::from(minutes) * 60);
                }
                None => {
                    workspace.self_dnd.snooze_enabled = false;
                    workspace.self_dnd.snooze_endtime = None;
                    workspace.self_dnd.snooze_remaining = None;
                }
            }
            self.self_account.snoozed = workspace.self_snoozed();
        } else {
            self.self_account.snoozed = minutes.is_some();
        }
    }

    pub fn merge_self_dnd(&mut self, dnd: super_platinum_core::slack::models::DndInfo) {
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
        {
            workspace.self_dnd = dnd;
            self.self_account.snoozed = workspace.self_snoozed();
        } else {
            self.self_account.snoozed = dnd.is_snoozed(super_platinum_core::state::now_secs());
        }
    }

    pub fn merge_self_snooze(&mut self, snooze: super_platinum_core::slack::models::DndInfo) {
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
        {
            workspace.self_dnd.snooze_enabled = snooze.snooze_enabled;
            workspace.self_dnd.snooze_endtime = snooze.snooze_endtime;
            workspace.self_dnd.snooze_remaining = snooze.snooze_remaining;
            self.self_account.snoozed = workspace.self_snoozed();
        } else {
            self.self_account.snoozed = snooze.snooze_enabled;
        }
    }

    pub fn clear_self_status(&mut self) {
        let user = self.self_account.user_id.clone();
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
            && let Some(profile) = workspace
                .users
                .get_mut(&user)
                .and_then(|user| user.profile.as_mut())
        {
            profile.status_text = Some(String::new());
            profile.status_emoji = Some(String::new());
            profile.status_expiration = Some(0);
        }
    }

    pub fn toggle_cache_kind(&mut self, kind: super_platinum_core::MediaCacheKind) {
        let index = kind.index();
        self.storage.selected[index] = !self.storage.selected[index];
    }

    pub fn set_cache_limit(&mut self, limit: super_platinum_core::config::CacheSizeLimit) {
        self.core.settings.cache_limit = limit;
        self.media.set_cache_limit(limit.bytes());
    }

    /// Drops cached history for every conversation except the one on screen so
    /// a Storage clear actually shrinks the sqlite file on the next persist.
    pub fn trim_cached_history(&mut self) {
        let active = self.core.active_channel.clone();
        let active_thread = self.core.active_thread.clone();
        for workspace in self.core.workspaces.values_mut() {
            for (id, messages) in workspace.messages.iter_mut() {
                if active.as_deref() == Some(id.as_str()) {
                    continue;
                }
                messages.messages.clear();
                messages.pending.clear();
                messages.loaded = false;
                messages.has_more_older = true;
            }
        }
        if let Some((channel, ts)) = &active_thread {
            self.core
                .threads
                .retain(|(_, thread_channel, thread_ts), _| {
                    thread_channel == channel && thread_ts == ts
                });
        } else {
            self.core.threads.clear();
        }
        match active {
            Some(channel) => self.messages_by_channel.retain(|id, _| id == &channel),
            None => self.messages_by_channel.clear(),
        }
    }

    pub fn notify_typing(&mut self) {
        let (Some(team), Some(channel)) = (
            self.core.active_team.clone(),
            self.core.active_channel.clone(),
        ) else {
            return;
        };
        let now = std::time::Instant::now();
        if self
            .core
            .last_typing
            .get(&(team.clone(), channel.clone()))
            .is_some_and(|last| now.duration_since(*last) < std::time::Duration::from_secs(3))
        {
            return;
        }
        let connection =
            self.core
                .workspaces
                .get(&team)
                .and_then(|workspace| match &workspace.rt {
                    super_platinum_core::state::RealtimeStatus::Connected(connection) => {
                        Some(connection.clone())
                    }
                    super_platinum_core::state::RealtimeStatus::Disconnected => None,
                });
        if let Some(connection) = connection {
            connection.send(super_platinum_core::slack::realtime::user_typing_frame(
                &channel,
            ));
            self.core.last_typing.insert((team, channel), now);
        }
    }

    pub fn palette_matches(&self) -> Vec<usize> {
        let Some(team) = self.core.active_team.as_ref() else {
            return self.palette_matches_fallback();
        };
        let Some(workspace) = self.core.workspaces.get(team) else {
            return self.palette_matches_fallback();
        };
        let entries = super_platinum_core::palette::rank(
            workspace,
            &self.palette_query,
            &std::collections::BTreeMap::new(),
        );
        let mut indices = Vec::new();
        for entry in entries {
            let channel_id = match entry.target {
                super_platinum_core::palette::PaletteTarget::Channel(id) => id,
                super_platinum_core::palette::PaletteTarget::User { dm: Some(id), .. } => id,
                super_platinum_core::palette::PaletteTarget::User { user, dm: None } => {
                    // Fall back to an IM channel for this user if one is loaded.
                    self.channels
                        .iter()
                        .find(|channel| {
                            channel.is_im && channel.user_id.as_deref() == Some(user.as_str())
                        })
                        .map(|channel| channel.id.clone())
                        .unwrap_or_default()
                }
            };
            if channel_id.is_empty() {
                continue;
            }
            if let Some(index) = self
                .channels
                .iter()
                .position(|channel| channel.id == channel_id)
                && !indices.contains(&index)
            {
                indices.push(index);
            }
        }
        if indices.is_empty() {
            if self.palette_query.trim().is_empty() {
                // Empty recents should not dump every loaded conversation.
                return Vec::new();
            }
            return self.palette_matches_fallback();
        }
        indices
    }

    fn palette_matches_fallback(&self) -> Vec<usize> {
        let query = self.palette_query.to_lowercase();
        self.channels
            .iter()
            .enumerate()
            .filter(|(_, channel)| query.is_empty() || channel.name.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn move_palette(&mut self, delta: isize) {
        let count = self.palette_matches().len();
        if count == 0 {
            self.palette_selected = 0;
            return;
        }
        self.palette_selected =
            (self.palette_selected as isize + delta).rem_euclid(count as isize) as usize;
    }

    pub fn submit_palette(&mut self) -> bool {
        let matches = self.palette_matches();
        let Some(index) = matches.get(self.palette_selected).copied() else {
            return false;
        };
        self.select_channel(index, ChannelOpen::Global);
        self.overlay = None;
        true
    }

    /// Opens one Activity item in the right pane. Returns false when the item's
    /// channel is not part of the loaded workspace, so the caller can report it.
    ///
    /// Activity shows a single surface: a thread item opens the thread, and any
    /// other item opens the channel around the message — which means the thread
    /// pane has to go, or it would keep showing the previous item's replies.
    pub fn select_activity_item(
        &mut self,
        key: String,
        channel: Option<&str>,
        ts: Option<&str>,
        thread_ts: Option<&str>,
    ) -> bool {
        self.core.activity.selected = Some(key);
        if thread_ts.is_none() {
            self.close_thread();
        }
        // An item Slack gave us no message for still selects: the row lights up
        // and the pane returns to its empty state rather than keeping the last
        // item's conversation, which the highlight no longer points at.
        let (Some(channel), Some(ts)) = (channel, ts) else {
            self.surfaces.remove(&MainView::Activity);
            return false;
        };
        self.open_message(channel, ts, crate::state::ChannelOpen::InSurface)
    }

    /// Opens a search hit. Search is global navigation, so like Slack's own
    /// client it lands in Home rather than inside whichever list was open.
    pub fn open_search_result(&mut self, channel: &str, ts: &str) -> bool {
        self.open_message(channel, ts, ChannelOpen::Global)
    }

    /// Opens one conversation anchored on a specific message.
    fn open_message(&mut self, channel: &str, ts: &str, open: ChannelOpen) -> bool {
        let Some(index) = self
            .channels
            .iter()
            .position(|candidate| candidate.id == channel)
        else {
            return false;
        };
        self.select_channel(index, open);
        self.core.pending_scroll_to = Some((
            channel.to_owned(),
            super_platinum_core::domain::PendingScrollTarget::Message(ts.to_owned()),
        ));
        self.overlay = None;
        true
    }
}
