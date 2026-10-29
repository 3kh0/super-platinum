//! DM list filtering and transient navigation revisions belong to the shell.
use super::ShellState;
use super_platinum_core::{
    slack::models::{Channel, DmEntry},
    state::Workspace,
};
fn embedded_channel(entry: &DmEntry) -> Option<&Channel> {
    entry.channel.as_ref().filter(|c| c.id == entry.id)
}
pub(crate) fn entry_unread(workspace: Option<&Workspace>, entry: &DmEntry) -> u32 {
    if let Some(ws) = workspace {
        if let Some(channel) = ws.channels.get(&entry.id).filter(|c| c.id == entry.id) {
            return ws.unread_total(channel);
        }
        // An authoritative zero-count bag also wins over stale feed metadata.
        if let Some(bag) = ws.messages.get(&entry.id) {
            return bag.mention_count.max(bag.unread_count);
        }
    }
    embedded_channel(entry)
        .and_then(|c| c.unread_count_display.or(c.unread_count))
        .unwrap_or(0)
}
fn usable_identity(channel: &Channel) -> bool {
    // The core IM label reads an explicit peer before it reaches name fallback.
    if channel.is_im
        && let Some(peer) = super_platinum_core::state::dm_user_id(channel)
    {
        return !peer.trim().is_empty();
    }
    channel
        .name
        .as_deref()
        .is_some_and(|name| !name.trim().is_empty())
}
fn row_channel<'a>(workspace: Option<&'a Workspace>, entry: &'a DmEntry) -> Option<&'a Channel> {
    workspace
        .and_then(|ws| ws.channels.get(&entry.id))
        .filter(|c| c.id == entry.id && usable_identity(c))
        .or_else(|| embedded_channel(entry).filter(|c| usable_identity(c)))
}
impl ShellState {
    pub(crate) fn set_dm_query(&mut self, value: String) {
        if self.dm_query != value {
            self.dm_input_revision = self.dm_input_revision.wrapping_add(1);
            self.dm_open = None;
        }
        self.dm_query = value;
    }
    pub(crate) fn set_dm_unread_only(&mut self, value: bool) {
        if self.dm_unread_only != value {
            self.dm_input_revision = self.dm_input_revision.wrapping_add(1);
            self.dm_open = None;
        }
        self.dm_unread_only = value;
    }
    pub(crate) fn dm_row_label(&self, entry: &DmEntry) -> String {
        let workspace = self
            .core
            .active_team
            .as_ref()
            .and_then(|t| self.core.workspaces.get(t));
        let channel = row_channel(workspace, entry);
        workspace
            .and_then(|ws| channel.map(|c| super_platinum_core::state::channel_display_name(ws, c)))
            .or_else(|| channel.and_then(|c| c.name.clone()))
            .unwrap_or_else(|| "Direct message".into())
    }
    pub(crate) fn dm_row_visible(&self, entry: &DmEntry) -> bool {
        let workspace = self
            .core
            .active_team
            .as_ref()
            .and_then(|t| self.core.workspaces.get(t));
        (!self.dm_unread_only || entry_unread(workspace, entry) > 0)
            && (self.dm_query.is_empty()
                || self
                    .dm_row_label(entry)
                    .to_lowercase()
                    .contains(&self.dm_query.to_lowercase()))
    }
}

#[cfg(test)]
mod tests;
