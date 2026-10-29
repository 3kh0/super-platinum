//! Optional names for already-published file results. No pager or row mutation.
use super::recovery::{Cursor, Identity};
use super::*;
use std::{collections::BTreeSet, future::Future, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    search::SearchKind,
    slack::{Error, Transport, models::User},
};

pub(super) struct Read {
    identity: Identity,
}
pub(super) struct Scope {
    identity: Identity,
    cursor: Cursor,
}
impl Read {
    pub(super) fn capture(
        shell: &ShellState,
        request: &SearchRequest,
        transport: &Arc<Transport>,
        session: &WorkspaceSession,
    ) -> Option<Self> {
        if request.kind != SearchKind::Files || !super::current(shell, request, transport) {
            return None;
        }
        let identity = Identity::capture(shell)?;
        identity
            .matches_session(transport, session)
            .then_some(Self { identity })
    }
    // The file loader mints this cursor in the same mutation that accepts a page.
    pub(super) fn complete(self, cursor: Cursor) -> Scope {
        Scope {
            identity: self.identity,
            cursor,
        }
    }
}
impl Scope {
    fn current(&self, shell: &ShellState) -> bool {
        self.identity.current(shell)
            && self.cursor.current(shell)
            && shell.search_pager.kind == SearchKind::Files
            && !shell.search_pager.loading
            && !shell.search_pager.failed
    }
}

fn unresolved(shell: &ShellState, team: &str) -> BTreeSet<String> {
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return BTreeSet::new();
    };
    shell
        .search_file_results
        .iter()
        .filter(|file| super::files::visible(shell, file))
        .filter_map(super_platinum_core::state::file_uploader_id)
        .filter(|id| super::user_names::needs_name(workspace, id))
        .map(str::to_owned)
        .collect()
}

pub(super) async fn hydrate_with<F, Fut>(mut state: Signal<ShellState>, scope: Scope, mut lookup: F)
where
    F: FnMut(Vec<String>) -> Fut,
    Fut: Future<Output = Result<Vec<User>, Error>>,
{
    let ids = {
        let Ok(shell) = state.try_read() else {
            return;
        };
        if !scope.current(&shell) {
            return;
        }
        unresolved(&shell, scope.identity.team())
            .into_iter()
            .collect::<Vec<_>>()
    };
    for chunk in ids.chunks(100) {
        let snapshots = {
            let Ok(shell) = state.try_read() else {
                return;
            };
            if !scope.current(&shell) {
                return;
            }
            let wanted = unresolved(&shell, scope.identity.team());
            let Some(workspace) = shell.core.workspaces.get(scope.identity.team()) else {
                return;
            };
            let ids = chunk
                .iter()
                .filter(|id| wanted.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            super::user_names::capture(workspace, &ids)
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
        if !scope.current(&shell) {
            return;
        }
        let wanted = unresolved(&shell, scope.identity.team());
        let Some(workspace) = shell.core.workspaces.get_mut(scope.identity.team()) else {
            return;
        };
        super::user_names::apply(workspace, &snapshots, &wanted, users);
    }
}

#[cfg(test)]
mod tests;
