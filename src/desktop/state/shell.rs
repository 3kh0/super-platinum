use std::collections::HashMap;

use crate::channel_vm::{project_channels, project_messages_for_channel};
use crate::media::MediaRegistry;
pub(crate) use crate::message_vm::message_vm;
pub use crate::model::*;

pub struct ShellState {
    pub core: super_platinum_core::CoreAppState,
    pub media: MediaRegistry,
    pub media_epoch: u64,
    pub background_uri: Option<String>,
    pub signed_in: bool,
    pub loading: bool,
    pub accounts: Vec<AccountVm>,
    pub workspaces: Vec<WorkspaceVm>,
    pub active_workspace: usize,
    pub channels: Vec<ChannelVm>,
    pub sidebar_sections: Vec<SidebarSectionVm>,
    pub active_channel: usize,
    pub messages: Vec<MessageVm>,
    pub messages_by_channel: HashMap<String, Vec<MessageVm>>,
    pub timeline_start: usize,
    pub timeline_end: usize,
    pub row_heights: HashMap<String, f64>,
    pub message_arrivals: HashMap<String, std::time::Instant>,
    pub selection_pinned: bool,
    /// Bumped while file uploads are in flight so progress UI re-renders.
    pub upload_ui_epoch: u64,
    pub stick_to_bottom: bool,
    pub loading_older: bool,
    pub chat_paused: bool,
    pub main_view: MainView,
    pub dm_query: String,
    pub dm_unread_only: bool,
    pub activity_tab: ActivityTab,
    /// When true, Activity main column shows the opened channel/thread.
    pub activity_detail_open: bool,
    pub profile_hover: Option<String>,
    pub overlay: Option<Overlay>,
    pub palette_query: String,
    pub palette_selected: usize,
    pub search_query: String,
    pub search_results: Vec<SearchResultVm>,
    pub search_loading: bool,
    pub thread_root: Option<String>,
    pub thread_messages: Vec<MessageVm>,
    pub profile_user: Option<String>,
    pub viewer: Option<ViewerVm>,
    pub toast: Option<String>,
    pub performance: PerformanceVm,
    pub channel_switch_started: Option<std::time::Instant>,
    pub realtime_insert_started: Option<std::time::Instant>,
}

impl ShellState {
    pub fn refresh_from_core(&mut self) {
        let Some(team) = self.core.active_team.clone() else {
            return;
        };
        let Some(workspace) = self.core.workspaces.get(&team) else {
            return;
        };
        let preferred_active = self.core.active_channel.clone();
        let (mut channels, sidebar_sections, default_active, _) =
            project_channels(workspace, &self.media);
        let active_channel = preferred_active
            .as_ref()
            .and_then(|id| channels.iter().position(|channel| &channel.id == id))
            .unwrap_or(default_active);
        // Rebuild sections with the preferred active for visibility.
        let sidebar_sections = if preferred_active.is_some() {
            let index_by_id: HashMap<&str, usize> = channels
                .iter()
                .enumerate()
                .map(|(index, channel)| (channel.id.as_str(), index))
                .collect();
            super_platinum_core::state::grouped_sidebar_sections(
                workspace,
                preferred_active.as_deref(),
            )
            .into_iter()
            .filter_map(|section| {
                let channel_indices = section
                    .channel_ids
                    .iter()
                    .filter_map(|id| index_by_id.get(id.as_str()).copied())
                    .collect::<Vec<_>>();
                if channel_indices.is_empty() {
                    return None;
                }
                Some(SidebarSectionVm {
                    id: section.id,
                    kind: section.kind,
                    title: section.title,
                    channel_indices,
                })
            })
            .collect()
        } else {
            sidebar_sections
        };
        // Ensure active conversation remains addressable even if filtered out of sections.
        if let Some(active_id) = preferred_active.as_ref()
            && !channels.iter().any(|channel| &channel.id == active_id)
            && let Some(channel) = workspace.channels.get(active_id)
        {
            channels.push(crate::channel_vm::channel_vm(
                workspace,
                channel,
                &self.media,
            ));
        }
        let active_channel = preferred_active
            .as_ref()
            .and_then(|id| channels.iter().position(|channel| &channel.id == id))
            .unwrap_or(active_channel.min(channels.len().saturating_sub(1)));
        let active_messages = channels.get(active_channel).map(|channel| {
            (
                channel.id.clone(),
                project_messages_for_channel(workspace, &channel.id, &self.media),
            )
        });
        let mut messages_by_channel = std::mem::take(&mut self.messages_by_channel);
        if let Some((channel, messages)) = &active_messages {
            messages_by_channel.insert(channel.clone(), messages.clone());
        }
        self.active_channel = active_channel;
        self.sidebar_sections = sidebar_sections;
        self.channels = channels;
        self.messages = active_messages
            .map(|(_, messages)| messages)
            .unwrap_or_default();
        self.messages_by_channel = messages_by_channel;
        if let (Some(channel), Some(root)) =
            (self.core.active_channel.as_ref(), self.thread_root.as_ref())
            && let Some(messages) =
                self.core
                    .threads
                    .get(&(team.clone(), channel.clone(), root.clone()))
        {
            self.thread_messages = messages
                .messages
                .iter()
                .map(|message| crate::message_vm::message_vm(workspace, message, &self.media))
                .collect();
        }
        self.reset_timeline_window();
        if let Some(channel) = self.channels.get(active_channel) {
            self.core.active_channel = Some(channel.id.clone());
        }
    }
}

pub(crate) fn keep_recent_messages(state: &mut ShellState, count: usize) {
    let len = state.messages.len();
    if len > count {
        state.messages = state.messages.split_off(len - count);
    }
    state.reset_timeline_window();
}

pub(crate) fn account_vms(accounts: &super_platinum_core::config::Accounts) -> Vec<AccountVm> {
    accounts
        .sessions
        .iter()
        .map(|(id, session)| AccountVm {
            id: id.clone(),
            label: session
                .workspaces
                .values()
                .map(|workspace| workspace.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            active: id == &accounts.active_account,
        })
        .collect()
}
