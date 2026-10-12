use super::ShellState;
use crate::model::*;

impl ShellState {
    pub(crate) fn timeline_scope(&self) -> String {
        format!(
            "{}-{}-{}-{}",
            self.core.account_epoch,
            self.core.active_team.as_deref().unwrap_or_default(),
            self.core.active_channel.as_deref().unwrap_or_default(),
            self.channel_generation
        )
    }

    pub(crate) fn estimated_row_height(&self, id: &str) -> f64 {
        self.row_heights.get(id).copied().unwrap_or(66.0)
    }

    /// Fast wheel/scrollbar jumps can land wholly inside a virtual spacer.
    /// Select a neighborhood from the same heights used to paint that spacer.
    pub(crate) fn estimated_timeline_window(&self, top: f64, height: f64) -> (usize, usize) {
        let top = if top.is_finite() { top.max(0.0) } else { 0.0 };
        let bottom = top
            + if height.is_finite() {
                height.max(0.0)
            } else {
                0.0
            };
        let last = self.messages.len().saturating_sub(1);
        let mut first = None;
        let mut offset = 0.0;
        for (index, message) in self.messages.iter().enumerate() {
            offset += self.estimated_row_height(&message.id);
            if first.is_none() && offset > top {
                first = Some(index);
            }
            if offset >= bottom {
                return (first.unwrap_or(index), index);
            }
        }
        (first.unwrap_or(last), last)
    }

    /// A refresh may prepend history, append new messages, or remove a row.
    /// Keep the same surviving rows mounted while the reader is in history.
    pub(crate) fn restore_timeline_window(&mut self, ids: &[String]) {
        let ids = ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        let mut positions = self
            .messages
            .iter()
            .enumerate()
            .filter_map(|(index, message)| ids.contains(message.id.as_str()).then_some(index));
        if let Some(first) = positions.next() {
            self.timeline_start = first;
            self.timeline_end = positions.last().unwrap_or(first) + 1;
        } else {
            self.reset_timeline_window();
        }
    }

    pub(crate) fn reset_timeline_window(&mut self) {
        if let Some((
            channel,
            super_platinum_core::domain::PendingScrollTarget::Message(ts)
            | super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(ts),
        )) = &self.core.pending_scroll_to
            && self.core.active_channel.as_ref() == Some(channel)
            && let Some(index) = self.messages.iter().position(|message| &message.ts == ts)
        {
            self.timeline_start = index.saturating_sub(24);
            self.timeline_end = (index + 76).min(self.messages.len());
            return;
        }
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
                && matches!(
                    self.core.pending_scroll_to,
                    Some((
                        _,
                        super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(_)
                    ))
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
            && matches!(
                self.core.pending_scroll_to,
                Some((
                    _,
                    super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(_)
                ))
            )
        {
            self.core.pending_scroll_to = None;
        }
        // A newly mounted/empty timeline can report its bottom before the
        // requested saved/search message exists. Keep its render neighborhood
        // and anchor until scroll_to_target confirms the actual row landed.
        if matches!(
            self.core.pending_scroll_to,
            Some((
                _,
                super_platinum_core::domain::PendingScrollTarget::Message(_)
            ))
        ) {
            self.reset_timeline_window();
            return;
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
        self.checkpoint_composers();
        let Some(channel) = self.channels.get(index) else {
            return;
        };
        self.channel_tab = ChannelTab::Messages;
        let channel_id = channel.id.clone();
        let unread = channel.unread;
        if open != ChannelOpen::Global && self.core.active_channel.as_ref() != Some(&channel_id) {
            self.close_thread();
        }
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
        self.loading_older = false;
        self.core.active_channel = Some(channel_id.clone());
        self.sync_composer_drafts();
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
        self.checkpoint_composers();
        self.close_thread();
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
            self.messages.clear();
        }
        self.active_workspace = index;
        self.channel_tab = ChannelTab::Messages;
        self.channel_generation = self.channel_generation.wrapping_add(1);
        self.loading_older = false;
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
    fn navigation_releases_the_previous_history_load() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell.loading_older = true;
        let previous = shell.channel_generation;
        shell.select_channel(shell.active_channel, ChannelOpen::Global);
        assert!(!shell.loading_older);
        assert_ne!(shell.channel_generation, previous);

        shell.loading_older = true;
        let previous = shell.channel_generation;
        assert!(shell.select_workspace(shell.active_workspace));
        assert!(!shell.loading_older);
        assert_ne!(shell.channel_generation, previous);
    }

    #[test]
    fn a_jump_into_a_spacer_selects_the_estimated_neighborhood_directly() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let template = shell.messages[0].clone();
        shell.messages = (0..200)
            .map(|n| {
                let mut message = template.clone();
                message.id = format!("jump-{n}");
                message
            })
            .collect();
        shell.row_heights.insert("jump-10".into(), 600.0);
        let (first, last) = shell.estimated_timeline_window(100.0 * 66.0 + 534.0, 132.0);
        assert_eq!((first, last), (100, 101));
        shell.core.pending_scroll_to = None;
        shell.timeline_start = 180;
        shell.timeline_end = 200;
        shell.set_timeline_window(first, last, [], false);
        assert_eq!((shell.timeline_start, shell.timeline_end), (88, 114));
        assert_eq!(shell.estimated_timeline_window(f64::NAN, 0.0), (0, 0));
    }

    #[test]
    fn refresh_keeps_the_reading_window_through_history_insertions_and_deletions() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let make_message = |n: usize| {
            serde_json::from_value(serde_json::json!({
            "type":"message", "user":"U1", "ts":format!("{n}.000001"), "text":"Synthetic history"
        })).unwrap()
        };
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages = (100..300).map(make_message).collect();
        shell.refresh_from_core();
        shell.stick_to_bottom = false;
        shell.core.pending_scroll_to = None;
        shell.timeline_start = 40;
        shell.timeline_end = 65;
        let held = shell.messages[40..65]
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>();
        shell.refresh_from_core();
        assert_eq!((shell.timeline_start, shell.timeline_end), (40, 65));
        let bag = shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap();
        bag.messages.splice(0..0, (90..100).map(make_message));
        bag.messages
            .retain(|m| m.ts.as_deref() != Some(held[0].as_str()));
        bag.messages.push(make_message(999));
        shell.refresh_from_core();
        assert_eq!(
            shell.messages[shell.timeline_start..shell.timeline_end]
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>(),
            held[1..]
        );
        assert!(!shell.stick_to_bottom);
    }

    #[test]
    fn bottom_refresh_keeps_its_window_and_includes_bounded_arrivals() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let message = |n: usize| {
            serde_json::from_value(serde_json::json!({
                "type":"message", "user":"U1", "ts":format!("{n}.000001"), "text":"Synthetic history"
            })).unwrap()
        };
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages = (100..300).map(message).collect();
        shell.refresh_from_core();
        shell.stick_to_bottom = true;
        shell.core.pending_scroll_to = None;
        shell.timeline_start = 182;
        shell.timeline_end = 200;
        shell.refresh_from_core();
        assert_eq!((shell.timeline_start, shell.timeline_end), (182, 200));
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages
            .push(message(300));
        shell.refresh_from_core();
        assert_eq!((shell.timeline_start, shell.timeline_end), (182, 201));
        assert_eq!(shell.messages[shell.timeline_end - 1].id, "300.000001");
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages
            .extend((301..601).map(message));
        shell.refresh_from_core();
        assert_eq!(shell.timeline_end - shell.timeline_start, 100);
        assert_eq!(shell.timeline_end, shell.messages.len());
    }

    #[test]
    fn unread_navigation_renders_its_target_outside_the_newest_window() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let template = shell.messages[0].clone();
        shell.messages = (0..240)
            .map(|n| {
                let mut message = template.clone();
                message.id = format!("{n}.000001");
                message.ts = message.id.clone();
                message
            })
            .collect();
        shell.core.pending_scroll_to = Some((
            "C2".into(),
            super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter("30.000001".into()),
        ));
        shell.reset_timeline_window();
        assert!(shell.timeline_start <= 30 && shell.timeline_end > 30);
        assert!(shell.timeline_end - shell.timeline_start <= 100);
    }

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
