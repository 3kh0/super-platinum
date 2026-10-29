//! Optional display names for explicit peers/authors in the accepted DM feed.
use super::super::super::user_names;
use super::super::Owner;
use super::*;
use std::collections::BTreeSet;
fn unresolved(shell: &ShellState, team: &str) -> BTreeSet<String> {
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return BTreeSet::new();
    };
    let mut ids = BTreeSet::new();
    for entry in &shell.core.dms.entries {
        if let Some(channel) = workspace
            .channels
            .get(&entry.id)
            .or_else(|| entry.channel.as_ref().filter(|c| c.id == entry.id))
            && (channel.is_im || channel.is_mpim)
            && let Some(peer) = super_platinum_core::state::dm_user_id(channel)
        {
            ids.insert(peer.to_owned());
        }
        if let Some(author) = entry.message.as_ref().and_then(|m| m.user.as_ref()) {
            ids.insert(author.clone());
        }
    }
    ids.retain(|id| user_names::needs_name(workspace, id));
    ids
}
pub(super) async fn hydrate_with<L, LFut>(
    mut state: Signal<ShellState>,
    scope: &Request,
    owner: &Arc<Owner>,
    mut lookup: L,
) where
    L: FnMut(Vec<String>) -> LFut,
    LFut: Future<Output = Result<Vec<User>, Error>>,
{
    let current = |shell: &ShellState| scope.current(shell) && !shell.core.dms.loading;
    let ids = {
        let Ok(shell) = state.try_read() else {
            return;
        };
        if !current(&shell) {
            return;
        }
        unresolved(&shell, &owner.team)
            .into_iter()
            .collect::<Vec<_>>()
    };
    for chunk in ids.chunks(100) {
        let snapshots = {
            let Ok(shell) = state.try_read() else {
                return;
            };
            if !current(&shell) {
                return;
            }
            let wanted = unresolved(&shell, &owner.team);
            let Some(workspace) = shell.core.workspaces.get(&owner.team) else {
                return;
            };
            let ids = chunk
                .iter()
                .filter(|id| wanted.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            user_names::capture(workspace, &ids)
        };
        if snapshots.is_empty() {
            continue;
        }
        let Ok(users) = lookup(snapshots.keys().cloned().collect()).await else {
            return;
        };
        let Ok(mut shell) = state.try_write() else {
            return;
        };
        if !current(&shell) {
            return;
        }
        let wanted = unresolved(&shell, &owner.team);
        let Some(workspace) = shell.core.workspaces.get_mut(&owner.team) else {
            return;
        };
        user_names::apply(workspace, &snapshots, &wanted, users);
    }
}
