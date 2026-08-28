use super::ShellState;
use crate::model::*;

impl ShellState {
    pub(super) fn reset_timeline_window(&mut self) {
        self.timeline_end = self.messages.len();
        self.timeline_start = self.timeline_end.saturating_sub(100);
    }

    pub fn set_timeline_window(
        &mut self,
        first: usize,
        last: usize,
        measurements: impl IntoIterator<Item = (String, f64)>,
        stick_to_bottom: bool,
    ) {
        self.row_heights.extend(measurements);
        self.stick_to_bottom = stick_to_bottom;
        // A reader who has reached the newest message has outrun the unread
        // divider we were still holding; applying it later reads as a random
        // teleport. A `Latest` anchor is left alone — it asks for the bottom,
        // which is where they already are, and dropping it early loses the
        // scroll a cold conversation still owes.
        if stick_to_bottom
            && !matches!(
                self.core.pending_scroll_to,
                Some((_, super_platinum_core::domain::PendingScrollTarget::Latest))
            )
        {
            self.core.pending_scroll_to = None;
        }
        if self.selection_pinned || self.messages.is_empty() {
            return;
        }
        self.timeline_start = first.saturating_sub(12).min(self.messages.len());
        self.timeline_end = last.saturating_add(13).min(self.messages.len());
        if self.timeline_end <= self.timeline_start {
            self.reset_timeline_window();
        }
    }

    /// Opens a conversation in the surface the request came from.
    ///
    /// `ChannelOpen::Global` is the quick switcher, a search hit, a channel
    /// mention, "message this person" — navigation that belongs to no surface.
    /// Slack lands all of it in Home with the channel sidebar (CDP-verified),
    /// rather than cramming the channel into whichever list panel happened to
    /// be open, where it would have had no composer and left the row that is
    /// still highlighted pointing at something else entirely.
    pub fn select_channel(&mut self, index: usize, open: ChannelOpen) {
        let Some(channel) = self.channels.get(index) else {
            return;
        };
        let channel_id = channel.id.clone();
        let unread = channel.unread;
        if open == ChannelOpen::Global {
            // Record the surface being left before anything moves, so that rail
            // tab still has its own conversation to come back to.
            self.remember_surface();
            self.close_thread();
            self.main_view = MainView::Home;
        }
        self.active_channel = index;
        self.channel_switch_started = Some(std::time::Instant::now());
        self.channel_generation = self.channel_generation.wrapping_add(1);
        self.core.active_channel = Some(channel_id.clone());
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
        {
            workspace.remember_visit(&channel_id);
        }
        self.surfaces.insert(
            self.main_view,
            SurfaceTarget {
                channel: channel_id.clone(),
                thread_root: self.thread_root.clone(),
            },
        );
        // Pin the divider before anything marks the conversation read. The
        // cached projection is not reused here: it was built for the previous
        // visit and carries that visit's divider.
        self.unread_anchor = self
            .core
            .active_team
            .as_ref()
            .and_then(|team| self.core.workspaces.get(team))
            .and_then(|workspace| workspace.messages.get(&channel_id))
            .and_then(|messages| messages.last_read.clone())
            .map(|last_read| (channel_id.clone(), last_read));
        self.messages = self
            .core
            .active_team
            .as_ref()
            .and_then(|team| self.core.workspaces.get(team))
            .map(|workspace| {
                crate::channel_vm::project_messages_for_channel(
                    workspace,
                    &channel_id,
                    &self.media,
                    self.divider_at(&channel_id),
                )
            })
            .unwrap_or_default();
        self.messages_by_channel
            .insert(channel_id.clone(), self.messages.clone());
        // Prefer first-unread anchoring when the conversation has unreads.
        if unread
            && let Some(message) = self
                .messages
                .iter()
                .find(|message| message.show_unread_divider)
        {
            let anchor = message.ts.clone();
            if let Some(idx) = self
                .messages
                .iter()
                .position(|message| message.ts == anchor)
            {
                self.stick_to_bottom = false;
                self.timeline_start = idx.saturating_sub(24);
                self.timeline_end = (idx + 80).min(self.messages.len());
                self.core.pending_scroll_to = Some((
                    channel_id,
                    super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(anchor),
                ));
                return;
            }
        }
        // No unread anchor: the conversation opens on its newest message. The
        // flag alone is not enough — the scroll container keeps the offset of
        // the channel left behind, so a shorter transcript opens parked below
        // its last row, showing an empty pane. The anchor is what actually
        // moves it; `refresh_selected_channel` spends it as soon as the rows
        // exist, and again after history lands for a cold conversation.
        self.stick_to_bottom = true;
        self.core.pending_scroll_to = Some((
            channel_id,
            super_platinum_core::domain::PendingScrollTarget::Latest,
        ));
        self.reset_timeline_window();
    }

    pub fn select_workspace(&mut self, index: usize) -> bool {
        let Some(team) = self
            .workspaces
            .get(index)
            .map(|workspace| workspace.id.clone())
        else {
            return false;
        };
        let Some(workspace) = self.core.workspaces.get(&team) else {
            return false;
        };
        self.active_workspace = index;
        self.core.active_team = Some(team);
        self.core.active_channel = workspace
            .last_active_channel
            .clone()
            .or_else(|| workspace.channels.keys().next().cloned());
        self.main_view = MainView::Home;
        self.refresh_from_core();
        true
    }
}
