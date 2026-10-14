//! Local Activity reads have distinct badge semantics in the official client.
use super::{CountsOwner, Transport, WorkspaceSession};
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{CoreAppState, domain::ThreadKey};

/// A cursor acknowledgement belongs to the credentials and socket generation
/// that issued it, even if the user has navigated to a different surface.
pub(crate) struct CursorOwner {
    owner: CountsOwner,
    generation: u64,
}

impl CursorOwner {
    pub(crate) fn capture(
        core: &CoreAppState,
        transport: &Arc<Transport>,
        session: &WorkspaceSession,
    ) -> Option<Self> {
        Some(Self {
            owner: CountsOwner::capture(core, transport, session)?,
            generation: core.workspaces.get(&session.team_id)?.rt_generation,
        })
    }

    pub(crate) fn current(&self, core: &CoreAppState) -> bool {
        self.owner.current(core)
            && core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|workspace| workspace.rt_generation == self.generation)
    }
}

/// `confirm_read` returns false for both rejection and an accepted partial
/// read. Inspect the accepted cursor, not its fully-read boolean.
pub(crate) fn confirm_channel_read(
    core: &mut CoreAppState,
    team: &str,
    channel: &str,
    cursor: &str,
    previous: Option<&str>,
    revision: u64,
) -> bool {
    let Some(workspace) = core.workspaces.get_mut(team) else {
        return false;
    };
    if !workspace.accepts_read_ack(channel, cursor, previous, revision) {
        return false;
    }
    workspace.confirm_read(channel, cursor, previous);
    if workspace.read_cursor(channel) != Some(cursor) {
        return false;
    }
    mark_channel_read(core, team, channel, cursor);
    true
}

pub(crate) fn confirm_thread_read(
    core: &mut CoreAppState,
    key: &ThreadKey,
    cursor: &str,
    revision: u64,
) -> bool {
    if !core.confirm_thread_mark(key, cursor, revision) {
        return false;
    }
    mark_thread_read(core, &key.0, &key.1, &key.2, cursor);
    true
}

pub(crate) fn mark_item_read(core: &mut CoreAppState, key: &str) -> Option<(String, String)> {
    let team = core.active_team.clone()?;
    if !core.activity.matches_scope(core.account_epoch, &team) {
        return None;
    }
    let previous_unread = core.activity.unread_count();
    let target = core.activity.mark_item_read(key)?;
    if let Some(workspace) = core.workspaces.get_mut(&team)
        && workspace.activity_inbox_enabled() != Some(true)
    {
        let unread = core
            .activity
            .total_after_read(workspace.activity_unread_count, previous_unread);
        workspace.set_activity_unread_count(unread);
    }
    // QDAL.m changes the item, not unreadActivityV2BadgeCounts. Keep the
    // canonical map until a cursor transition or authoritative counts arrives.
    Some(target)
}

pub(crate) fn mark_channel_read(core: &mut CoreAppState, team: &str, channel: &str, cursor: &str) {
    if core.active_team.as_deref() != Some(team)
        || !core.activity.matches_scope(core.account_epoch, team)
    {
        return;
    }
    let previous_unread = core.activity.unread_count();
    let deltas = core.activity.mark_channel_read_categories(channel, cursor);
    reconcile_counts(core, team, previous_unread, deltas);
}

pub(crate) fn mark_thread_read(
    core: &mut CoreAppState,
    team: &str,
    channel: &str,
    root: &str,
    cursor: &str,
) {
    if core.active_team.as_deref() != Some(team)
        || !core.activity.matches_scope(core.account_epoch, team)
    {
        return;
    }
    let previous_unread = core.activity.unread_count();
    let deltas = core
        .activity
        .mark_thread_read_categories(channel, root, cursor);
    reconcile_counts(core, team, previous_unread, deltas);
}

fn reconcile_counts(
    core: &mut CoreAppState,
    team: &str,
    previous_unread: u32,
    deltas: BTreeMap<String, u32>,
) {
    if let Some(workspace) = core.workspaces.get_mut(team) {
        if workspace.activity_inbox_enabled() == Some(true) {
            // MdxY counts root entries by category, never bundled message units.
            if !deltas.is_empty() {
                workspace.mark_activity_categories_read(&deltas);
            }
        } else if previous_unread != core.activity.unread_count() {
            let unread = core
                .activity
                .total_after_read(workspace.activity_unread_count, previous_unread);
            workspace.set_activity_unread_count(unread);
        }
    }
}

#[cfg(test)]
mod tests;
