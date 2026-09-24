use super::ShellState;
use crate::model::*;

impl ShellState {
    pub(super) fn reset_timeline_window(&mut self) {
        self.timeline_end = self.messages.len();
        self.timeline_start = self.timeline_end.saturating_sub(100);
    }

    /// Avoid notifying the renderer when a scroll reports the same window and
    /// measured heights. Keep the pending unread anchor in the comparison too.
    pub fn timeline_window_changed(
        &self,
        first: usize,
        last: usize,
        measurements: &[(String, f64)],
        stick_to_bottom: bool,
    ) -> bool {
        if self.stick_to_bottom != stick_to_bottom
            || (stick_to_bottom
                && self.core.pending_scroll_to.is_some()
                && !matches!(
                    self.core.pending_scroll_to,
                    Some((_, super_platinum_core::domain::PendingScrollTarget::Latest))
                ))
            || measurements
                .iter()
                .any(|(id, height)| self.row_heights.get(id) != Some(height))
        {
            return true;
        }
        if self.selection_pinned || self.messages.is_empty() {
            return false;
        }
        let start = first.saturating_sub(12).min(self.messages.len());
        let end = last.saturating_add(13).min(self.messages.len());
        if end <= start {
            self.timeline_start != self.messages.len().saturating_sub(100)
                || self.timeline_end != self.messages.len()
        } else {
            self.timeline_start != start || self.timeline_end != end
        }
    }

    pub fn set_timeline_window(
        &mut self,
        first: usize,
        last: usize,
        measurements: impl IntoIterator<Item = (String, f64)>,
        stick_to_bottom: bool,
    ) {
        self.row_heights.extend(
            measurements
                .into_iter()
                .filter(|(_, height)| height.is_finite() && *height > 0.0),
        );
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
        // Keep the neighborhood of the current viewport for reverse scrolling,
        // plus the newest rows for returning to the bottom. IDs are timestamps,
        // not channel-qualified, so never keep heights across conversations.
        if self.row_heights.len() > 600 {
            let start = self.timeline_start.saturating_sub(200);
            let end = self
                .timeline_end
                .saturating_add(200)
                .min(self.messages.len());
            let nearby = self.messages[start..end.min(start.saturating_add(500))]
                .iter()
                .chain(self.messages[self.messages.len().saturating_sub(100)..].iter())
                .map(|message| message.id.as_str())
                .collect::<std::collections::HashSet<_>>();
            self.row_heights
                .retain(|id, _| nearby.contains(id.as_str()));
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
        if self.active_channel != index
            || self.core.active_channel.as_deref() != Some(channel_id.as_str())
        {
            self.row_heights.clear();
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
        if self.core.active_team.as_deref() != Some(team.as_str()) {
            self.row_heights.clear();
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaRegistry;

    #[test]
    fn unchanged_measurements_do_not_require_a_write() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let rows = vec![(shell.messages[0].id.clone(), 72.0)];
        shell.set_timeline_window(0, 0, rows.clone(), false);
        assert!(!shell.timeline_window_changed(0, 0, &rows, false));
        assert!(shell.timeline_window_changed(0, 0, &[(rows[0].0.clone(), 74.0)], false));
        assert!(shell.timeline_window_changed(1, 1, &rows, false));
    }

    #[test]
    fn heights_are_bounded_and_cleared_for_another_conversation() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let original = shell.messages[0].clone();
        shell.messages = (0..800)
            .map(|index| {
                let mut message = original.clone();
                message.id = format!("row-{index}");
                message
            })
            .collect();
        shell.set_timeline_window(
            400,
            410,
            (0..800).map(|index| (format!("row-{index}"), 70.0)),
            false,
        );
        assert!(shell.row_heights.len() <= 600);
        assert!(shell.row_heights.contains_key("row-399"));
        assert!(shell.row_heights.contains_key("row-799"));
        let other = if shell.active_channel == 0 { 1 } else { 0 };
        shell.select_channel(other, ChannelOpen::Global);
        assert!(shell.row_heights.is_empty());
    }
}
