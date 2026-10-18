//! Owned persistence work, detached from live socket and UI state.
use super::{Cache, Workspace};
use crate::{config::WorkspaceSession, error::AppError};

pub struct WorkspaceSnapshot {
    workspace: Workspace,
    metadata_only: bool,
}

impl WorkspaceSnapshot {
    pub fn full(workspace: &Workspace) -> Self {
        Self {
            workspace: workspace.clone(),
            metadata_only: false,
        }
    }

    /// Only fields consumed by Cache::save_workspace outside its transcript
    /// loop. In particular, do not clone rich messages merely to save groups.
    pub fn metadata(source: &Workspace) -> Self {
        let mut workspace = Workspace::from_session(&WorkspaceSession {
            team_id: source.team_id.clone(),
            enterprise_id: None,
            user_id: source.self_user_id.clone(),
            name: source.name.clone(),
            url: source.url.clone(),
            token: String::new(),
        });
        workspace.locale = source.locale.clone();
        workspace.team_plan = source.team_plan.clone();
        workspace.posting_metadata = source.posting_metadata.external_cache_snapshot();
        workspace.draft_presence = source.draft_presence.clone();
        workspace.deleted_files = source.deleted_files.clone();
        workspace.thread_counts = source.thread_counts.clone();
        workspace.notifications = source.notifications.clone();
        workspace.read_preferences = source.read_preferences.clone();
        workspace.emoji_preferences = source.emoji_preferences.clone();
        workspace.clear_unreads_preference = source.clear_unreads_preference.clone();
        workspace.sidebar = source.sidebar.clone();
        workspace.vip_users = source.vip_users.clone();
        workspace.priority_sidebar_section = source.priority_sidebar_section;
        workspace.starred_order = source.starred_order.clone();
        workspace.last_active_channel = source.last_active_channel.clone();
        workspace.recent_channels = source.recent_channels.clone();
        workspace.frecency = source.frecency.clone();
        workspace.channels = source.channels.clone();
        workspace.users = source.users.clone();
        workspace.usergroups = source.usergroups.clone();
        Self {
            workspace,
            metadata_only: true,
        }
    }

    pub fn team_id(&self) -> &str {
        &self.workspace.team_id
    }

    /// The newest metadata must not discard an earlier unsaved transcript.
    /// The queue keeps accounts separate; reject cross-team merging here too.
    pub fn retain_older_transcripts(&mut self, older: Self) {
        if self.metadata_only && !older.metadata_only && self.team_id() == older.team_id() {
            self.workspace.messages = older.workspace.messages;
            self.metadata_only = false;
        }
    }

    pub fn save(&self, cache: &Cache) -> Result<(), AppError> {
        // Empty messages means the existing channel_state/messages tables are
        // untouched, not cleared. Full snapshots retain the existing semantics.
        cache.save_workspace(&self.workspace)
    }
}

#[cfg(test)]
mod tests;
