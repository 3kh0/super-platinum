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
        // A reader who has reached the newest message has outrun any anchor we
        // were still holding; applying it later reads as a random teleport.
        if stick_to_bottom {
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

    pub fn select_channel(&mut self, index: usize) {
        let Some(channel) = self.channels.get(index) else {
            return;
        };
        let channel_id = channel.id.clone();
        let unread = channel.unread;
        self.active_channel = index;
        self.channel_switch_started = Some(std::time::Instant::now());
        self.core.active_channel = Some(channel_id.clone());
        if let Some(team) = self.core.active_team.clone()
            && let Some(workspace) = self.core.workspaces.get_mut(&team)
        {
            workspace.remember_visit(&channel_id);
        }
        // Keep DMs / Activity list panels open when opening a conversation.
        if !matches!(self.main_view, MainView::Dms | MainView::Activity) {
            self.main_view = MainView::Home;
        }
        if self.main_view == MainView::Activity {
            self.activity_detail_open = true;
        }
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
        // No unread anchor: the conversation opens on its newest message. Without
        // this the flag carries over from the channel left behind, and leaving a
        // scrolled-up channel opens the next one at the top of its window.
        self.stick_to_bottom = true;
        self.core.pending_scroll_to = None;
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
