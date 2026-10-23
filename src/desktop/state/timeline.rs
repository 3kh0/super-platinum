use super::ShellState;
use crate::model::*;
mod emoji;

impl ShellState {
    /// Exact shared mounting condition for the conversation pane (including tabs).
    pub(crate) fn conversation_visible(&self) -> bool {
        !matches!(
            self.main_view,
            MainView::Unreads | MainView::Threads | MainView::Files | MainView::Scheduled
        ) && self.detail_open()
            && !(matches!(self.main_view, MainView::Activity | MainView::Later)
                && self.thread_root.is_some())
    }
    /// Complete synthetic source; unlike `messages`, this is not a viewport slice.
    /// Native fixture checks use it to detect source mutation across window changes.
    pub(crate) fn fixture_message_source(&self) -> Option<&[MessageVm]> {
        self.fixture_channel_messages.as_deref()
    }

    /// Whether indexed rows refer to canonical history rather than fixture VMs.
    pub(crate) fn channel_window_is_canonical(&self) -> bool {
        self.fixture_channel_messages.is_none()
    }

    /// Rebuild rich content only for the current absolute metadata window.
    pub(crate) fn reproject_channel_window(&mut self) {
        let standard = self
            .core
            .active_team
            .as_deref()
            .and_then(|team| crate::app_home::channel_fetches::emoji_standard(self, team));
        let start = self.timeline_start.min(self.channel_rows.len());
        let end = self.timeline_end.max(start).min(self.channel_rows.len());
        let end = if self.selection_pinned {
            end
        } else {
            end.min(start.saturating_add(100))
        };
        self.timeline_start = start;
        self.timeline_end = end;
        self.channel_projection_generation = self.channel_projection_generation.wrapping_add(1);
        self.messages = if let Some(messages) = &self.fixture_channel_messages {
            messages[start..end].to_vec()
        } else {
            self.core
                .active_team
                .as_ref()
                .and_then(|team| self.core.workspaces.get(team))
                .zip(self.channels.get(self.active_channel))
                .map(|(workspace, channel)| {
                    crate::channel_vm::project_channel_window_with_standard(
                        workspace,
                        &channel.id,
                        &self.media,
                        &self.channel_rows,
                        start..end,
                        standard.as_ref().map(|observed| observed.standard()),
                    )
                })
                .unwrap_or_default()
        };
        crate::thread_unread::annotate(self);
    }

    /// Explicit synthetic-fixture path. Production refreshes always derive rows
    /// from raw workspace history and discard this override.
    pub(crate) fn install_fixture_messages(&mut self, messages: Vec<MessageVm>) {
        self.channel_rows = messages
            .iter()
            .enumerate()
            .map(|(source_index, message)| crate::channel_vm::ChannelRow {
                id: message.id.clone(),
                ts: message.ts.clone(),
                pending: message.pending,
                source_index,
                compact: message.compact,
                date_label: message.date_label.clone(),
                show_unread_divider: message.show_unread_divider,
            })
            .collect();
        self.rebuild_channel_geometry();
        self.fixture_channel_messages = Some(messages);
        self.reset_timeline_window();
    }

    /// Persist direct fixture edits to the currently mounted rich rows.
    pub(crate) fn sync_fixture_messages(&mut self) {
        let mut all = self.fixture_channel_messages.take().unwrap_or_default();
        if all.is_empty()
            || self
                .messages
                .iter()
                .any(|message| !all.iter().any(|old| old.id == message.id))
        {
            // Fixture construction can replace the entire synthetic timeline.
            all = self.messages.clone();
        } else {
            for message in &self.messages {
                if let Some(old) = all.iter_mut().find(|old| old.id == message.id) {
                    *old = message.clone();
                }
            }
        }
        self.install_fixture_messages(all);
    }

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
        self.row_heights
            .get(id)
            .copied()
            .unwrap_or(crate::timeline_geometry::DEFAULT_ROW_HEIGHT)
    }

    pub(crate) fn rebuild_channel_geometry(&mut self) {
        // Unmeasured rows keep a fixed estimate. Measuring a large image must
        // not change every unknown row above the reader through an average.
        self.channel_geometry = crate::timeline_geometry::Geometry::from_heights(
            self.channel_rows
                .iter()
                .map(|row| self.estimated_row_height(&row.id)),
        );
    }

    /// Fast wheel/scrollbar jumps can land wholly inside a virtual spacer.
    /// Select a neighborhood from the same heights used to paint that spacer.
    pub(crate) fn estimated_timeline_window(&self, top: f64, height: f64) -> (usize, usize) {
        self.channel_geometry.estimated_window(top, height)
    }

    /// A refresh may prepend history, append new messages, or remove a row.
    /// Keep the same surviving rows mounted while the reader is in history.
    pub(crate) fn restore_timeline_window(&mut self, ids: &[String]) {
        let ids = ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        let mut positions = self
            .channel_rows
            .iter()
            .enumerate()
            .filter_map(|(index, message)| ids.contains(message.id.as_str()).then_some(index));
        if let Some(first) = positions.next() {
            self.timeline_start = first;
            let end = positions.last().unwrap_or(first) + 1;
            self.timeline_end = if self.selection_pinned {
                end
            } else {
                end.min(first.saturating_add(100))
            };
        } else {
            self.reset_timeline_bounds();
        }
    }

    pub(crate) fn reset_timeline_window(&mut self) {
        self.reset_timeline_bounds();
        self.reproject_channel_window();
    }

    pub(super) fn reset_timeline_bounds(&mut self) {
        let index = self
            .core
            .pending_scroll_to
            .as_ref()
            .and_then(|(channel, target)| {
                if self.core.active_channel.as_ref() != Some(channel) {
                    return None;
                }
                match target {
                    super_platinum_core::domain::PendingScrollTarget::Message(ts) => self
                        .channel_rows
                        .iter()
                        .position(|message| &message.ts == ts),
                    super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(cursor) => {
                        crate::read_preferences::opening::first_after(self, cursor)
                    }
                    super_platinum_core::domain::PendingScrollTarget::Latest => None,
                }
            });
        if let Some(index) = index {
            self.timeline_start = index.saturating_sub(24);
            self.timeline_end = (index + 76).min(self.channel_rows.len());
            return;
        }
        self.timeline_end = self.channel_rows.len();
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
        let stick_to_bottom = stick_to_bottom
            && !matches!(
                self.core.pending_scroll_to,
                Some((
                    _,
                    super_platinum_core::domain::PendingScrollTarget::Message(_)
                        | super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(_)
                ))
            );
        if self.stick_to_bottom != stick_to_bottom
            || measurements
                .iter()
                .any(|(id, height)| self.row_heights.get(id) != Some(height))
        {
            return true;
        }
        if self.selection_pinned || self.channel_rows.is_empty() {
            return false;
        }
        let start = first.saturating_sub(12).min(self.channel_rows.len());
        let end = last
            .saturating_add(13)
            .min(self.channel_rows.len())
            .min(start.saturating_add(100));
        if end <= start {
            self.timeline_start != self.channel_rows.len().saturating_sub(100)
                || self.timeline_end != self.channel_rows.len()
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
        let mut geometry_changed = false;
        for (id, height) in measurements {
            if height.is_finite() && height > 0.0 && self.row_heights.get(&id) != Some(&height) {
                self.row_heights.insert(id, height);
                geometry_changed = true;
            }
        }
        // An empty/cold mount also reports bottom. Only actual user navigation
        // cancels an unread/saved opening; a geometry sample cannot do so.
        self.stick_to_bottom = stick_to_bottom
            && !matches!(
                self.core.pending_scroll_to,
                Some((
                    _,
                    super_platinum_core::domain::PendingScrollTarget::Message(_)
                        | super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(_)
                ))
            );
        // A newly mounted/empty timeline can report its bottom before the
        // requested saved/search message exists. Keep its render neighborhood
        // and anchor until scroll_to_target confirms the actual row landed.
        if matches!(
            self.core.pending_scroll_to,
            Some((
                _,
                super_platinum_core::domain::PendingScrollTarget::Message(_)
                    | super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(_)
                    | super_platinum_core::domain::PendingScrollTarget::Latest
            ))
        ) {
            if geometry_changed {
                self.rebuild_channel_geometry();
            }
            self.reset_timeline_window();
            return;
        }
        if self.selection_pinned || self.channel_rows.is_empty() {
            if geometry_changed {
                self.rebuild_channel_geometry();
            }
            return;
        }
        self.timeline_start = first.saturating_sub(12).min(self.channel_rows.len());
        self.timeline_end = last
            .saturating_add(13)
            .min(self.channel_rows.len())
            .min(self.timeline_start.saturating_add(100));
        if self.timeline_end <= self.timeline_start {
            self.reset_timeline_window();
        }
        self.reproject_channel_window();
        // Keep the neighborhood of the current viewport for reverse scrolling,
        // plus the newest rows for returning to the bottom. IDs are timestamps,
        // not channel-qualified, so never keep heights across conversations.
        if self.row_heights.len() > 600 {
            let previous_len = self.row_heights.len();
            let start = self.timeline_start.saturating_sub(200);
            let end = self
                .timeline_end
                .saturating_add(200)
                .min(self.channel_rows.len());
            let nearby = self.channel_rows[start..end.min(start.saturating_add(500))]
                .iter()
                .chain(self.channel_rows[self.channel_rows.len().saturating_sub(100)..].iter())
                .map(|message| message.id.as_str())
                .collect::<std::collections::HashSet<_>>();
            self.row_heights
                .retain(|id, _| nearby.contains(id.as_str()));
            geometry_changed |= previous_len != self.row_heights.len();
        }
        if geometry_changed {
            self.rebuild_channel_geometry();
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
        crate::read_preferences::opening::cancel(self);
        self.cancel_profile_dm();
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
        self.selection_pinned = false;
        self.channel_generation = self.channel_generation.wrapping_add(1);
        self.loading_older = false;
        self.core.active_channel = Some(channel_id.clone());
        crate::app_home::workflows::navigation_history::record(self, &channel_id);
        crate::canvas::checklist::close_visit(self);
        crate::channel_tabs::selection::ordinary_open(self);
        self.sync_composer_drafts();
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
        {
            workspace.remember_visit(&channel_id);
            // Metadata lookup can append a conversation after the last sidebar
            // projection. Re-evaluate active-preview placement over the existing
            // row indices without refreshing the timeline or changing membership.
            self.sidebar_sections = crate::channel_vm::project_sidebar_sections(
                workspace,
                &self.channels,
                Some(&channel_id),
                &mut self.sidebar_memory,
            );
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
        self.fixture_channel_messages = None;
        self.channel_rows = self
            .core
            .active_team
            .as_ref()
            .and_then(|team| self.core.workspaces.get(team))
            .map(|workspace| {
                crate::channel_vm::channel_rows(
                    workspace,
                    &channel_id,
                    self.divider_at(&channel_id),
                )
            })
            .unwrap_or_default();
        self.rebuild_channel_geometry();
        crate::read_preferences::opening::apply(self, unread);
    }

    pub fn select_workspace(&mut self, index: usize) -> bool {
        let Some(team) = self
            .workspaces
            .get(index)
            .map(|workspace| workspace.id.clone())
        else {
            return false;
        };
        if crate::canvas_create::departure::request(
            self,
            crate::canvas_create::departure::Destination::Workspace(team),
        ) {
            return false;
        }
        crate::read_preferences::opening::cancel(self);
        self.close_profile();
        self.cancel_palette_open();
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
            self.channel_rows.clear();
            self.channel_geometry = Default::default();
            self.messages.clear();
            self.fixture_channel_messages = None;
            self.surfaces.remove(&MainView::Activity);
            self.search_open_generation = self.search_open_generation.wrapping_add(1);
            self.search_opening = None;
            self.search_pager.reset();
            self.search_results.clear();
            self.search_filters = None;
            self.search_filter_error = None;
        }
        self.active_workspace = index;
        self.channel_tab = ChannelTab::Messages;
        self.selection_pinned = false;
        self.channel_generation = self.channel_generation.wrapping_add(1);
        self.loading_older = false;
        self.core
            .threads_view
            .ensure_scope(self.core.account_epoch, &team);
        self.core
            .activity
            .ensure_scope(self.core.account_epoch, &team);
        self.core.active_team = Some(team);
        self.core.active_channel = workspace
            .last_active_channel
            .clone()
            .or_else(|| workspace.channels.keys().next().cloned());
        self.main_view = MainView::Home;
        self.refresh_from_core();
        crate::canvas::checklist::close_visit(self);
        crate::channel_tabs::selection::ordinary_open(self);
        let unread = self
            .channels
            .get(self.active_channel)
            .is_some_and(|channel| channel.unread);
        crate::read_preferences::opening::apply(self, unread);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaRegistry;

    #[test]
    fn switching_workspaces_clears_activity_rows_and_conversation_restore() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let mut workspace = shell.core.workspaces["T1"].clone();
        workspace.team_id = "OTHER".into();
        shell.core.workspaces.insert("OTHER".into(), workspace);
        shell.workspaces.push(WorkspaceVm {
            id: "OTHER".into(),
            name: "Other workspace".into(),
            initials: "O".into(),
        });
        shell.surfaces.insert(
            MainView::Activity,
            SurfaceTarget {
                channel: "C2".into(),
                thread_root: None,
            },
        );
        let old_request = shell.core.activity.begin(false).unwrap();
        assert!(shell.select_workspace(shell.workspaces.len() - 1));
        assert!(shell.core.activity.items.is_empty());
        assert!(!shell.core.activity.loading);
        assert!(!shell.core.activity.accepts(&old_request));
        assert!(!shell.surfaces.contains_key(&MainView::Activity));
        assert!(
            shell
                .core
                .activity
                .matches_scope(shell.core.account_epoch, "OTHER")
        );
    }

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
        let messages = std::mem::take(&mut shell.messages);
        shell.install_fixture_messages(messages);
        shell.row_heights.insert("jump-10".into(), 600.0);
        shell.rebuild_channel_geometry();
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
        let held = shell.channel_rows[40..65]
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
            shell
                .messages
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
        assert_eq!(shell.messages.last().unwrap().id, "300.000001");
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
        assert_eq!(shell.timeline_end, shell.channel_rows.len());
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
        let messages = std::mem::take(&mut shell.messages);
        shell.install_fixture_messages(messages);
        shell.core.pending_scroll_to = Some((
            "C2".into(),
            super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter("30.000001".into()),
        ));
        shell.reset_timeline_window();
        assert!(shell.timeline_start <= 30 && shell.timeline_end > 30);
        assert!(shell.timeline_end - shell.timeline_start <= 100);
    }

    #[test]
    fn opening_latest_keeps_its_row_mounted_until_navigation_finishes() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let template = shell.messages[0].clone();
        shell.messages = (0..240)
            .map(|n| {
                let mut row = template.clone();
                row.id = format!("{n}.0");
                row.ts = row.id.clone();
                row
            })
            .collect();
        let messages = std::mem::take(&mut shell.messages);
        shell.install_fixture_messages(messages);
        shell.core.pending_scroll_to = Some((
            "C2".into(),
            super_platinum_core::domain::PendingScrollTarget::Latest,
        ));
        shell.reset_timeline_window();
        // The short conversation we just left reports its old top before the
        // long conversation has painted. It must not evict the latest target.
        shell.set_timeline_window(0, 0, [], false);
        assert_eq!((shell.timeline_start, shell.timeline_end), (140, 240));
        assert!(shell.core.pending_scroll_to.is_some());
        shell.core.pending_scroll_to = None;
        shell.set_timeline_window(0, 0, [], false);
        assert_eq!((shell.timeline_start, shell.timeline_end), (0, 13));
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
        let messages = std::mem::take(&mut shell.messages);
        shell.install_fixture_messages(messages);
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
    #[test]
    fn pinned_refresh_preserves_every_selected_row_across_internal_insertions() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let message = |ts: String| {
            serde_json::from_value(serde_json::json!({
                "type":"message", "user":"U1", "ts":ts, "text":"Pinned history"
            }))
            .unwrap()
        };
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages = (100..300).map(|n| message(format!("{n}.000001"))).collect();
        shell.core.pending_scroll_to = None;
        shell.refresh_from_core();
        shell.timeline_start = 100;
        shell.timeline_end = 200;
        shell.reproject_channel_window();
        let selected = shell
            .messages
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>();
        shell.pin_selection();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages
            .insert(151, message("250.000002".into()));
        shell.refresh_from_core();
        assert_eq!(shell.messages.len(), 101);
        assert!(
            selected
                .iter()
                .all(|id| shell.messages.iter().any(|row| &row.id == id))
        );
        shell.unpin_selection();
        assert_eq!(shell.messages.len(), 100);
    }
}
