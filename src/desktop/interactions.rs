use crate::state::ShellState;

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
        self.select_channel(index);
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
        self.activity_detail_open = true;
        self.core.activity.selected = Some(key);
        if thread_ts.is_none() {
            self.close_thread();
        }
        // An item Slack gave us no message for still selects: the row lights up
        // and the pane stays on its empty state.
        let (Some(channel), Some(ts)) = (channel, ts) else {
            return false;
        };
        self.open_search_result(channel, ts)
    }

    pub fn open_search_result(&mut self, channel: &str, ts: &str) -> bool {
        let Some(index) = self
            .channels
            .iter()
            .position(|candidate| candidate.id == channel)
        else {
            return false;
        };
        self.select_channel(index);
        self.core.pending_scroll_to = Some((
            channel.to_owned(),
            super_platinum_core::domain::PendingScrollTarget::Message(ts.to_owned()),
        ));
        self.overlay = None;
        true
    }
}
