//! Local Activity reads have distinct badge semantics in the official client.
use super_platinum_core::CoreAppState;

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

pub(crate) fn mark_channel_read(core: &mut CoreAppState, team: &str, channel: &str) {
    if core.active_team.as_deref() != Some(team)
        || !core.activity.matches_scope(core.account_epoch, team)
    {
        return;
    }
    let previous_unread = core.activity.unread_count();
    let deltas = core.activity.mark_channel_read_categories(channel);
    if deltas.is_empty() {
        return;
    }
    if let Some(workspace) = core.workspaces.get_mut(team) {
        if workspace.activity_inbox_enabled() == Some(true) {
            // MdxY counts root entries by category, never bundled message units.
            workspace.mark_activity_categories_read(&deltas);
        } else {
            let unread = core
                .activity
                .total_after_read(workspace.activity_unread_count, previous_unread);
            workspace.set_activity_unread_count(unread);
        }
    }
}
