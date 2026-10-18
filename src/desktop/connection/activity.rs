//! Owner-bound native Activity picker path selection. This deliberately maps
//! the existing native connection contract, not fabricated Slack Redux stores.
use crate::{directory_member_fetches::Context, state::ShellState};
use std::{sync::Arc, time::Instant};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Transport, transport::Health},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Path {
    Local,
    Remote,
}

/// Secret-bearing ownership remains private and is never Debug/serialized.
pub(crate) struct Observation {
    owner: Context,
    route_observed_at: Instant,
    query: String,
    path: Path,
}
impl Observation {
    /// The fixed AjOk caller supplies Query::text(Remote), before the second
    /// local formatting stage. No whitespace trim occurs at this boundary.
    /// The desktop adopts Sonic/client-v2 empty-query policy explicitly.
    pub(crate) fn capture(
        shell: &ShellState,
        epoch: u64,
        transport: &Arc<Transport>,
        owner: &WorkspaceSession,
        first_formatted_query: &str,
    ) -> Option<Self> {
        if first_formatted_query.len() > 4096
            || first_formatted_query.chars().any(|ch| ch == '\0')
            || !owner_current(shell, epoch, transport, owner)
        {
            return None;
        }
        let workspace = shell.core.workspaces.get(&owner.team_id)?;
        let route_observed_at = shell.connection.routed_at?;
        // This existing shared helper retains the complete authentication
        // session, account identity and exact transport/RT owner privately.
        let context = Context::capture(shell, &owner.team_id).ok()?;
        let path = native_path(
            shell.connection.routable,
            transport.health(),
            workspace.rt.is_connected(),
            first_formatted_query,
        );
        Some(Self {
            owner: context,
            route_observed_at,
            query: first_formatted_query.into(),
            path,
        })
    }
    pub(crate) fn path(&self) -> Path {
        self.path
    }
    /// Re-evaluate the actual raw native decision before remote dispatch and
    /// after awaits. A newer positive route sample or Unknown->Online health
    /// from this very request is allowed; a changed account/RT owner is not.
    pub(crate) fn current(&self, shell: &ShellState, first_formatted_query: &str) -> bool {
        if first_formatted_query != self.query
            || !self.owner.current(shell)
            || !owner_current(
                shell,
                self.owner.epoch,
                &self.owner.transport,
                &self.owner.session,
            )
            || !shell
                .connection
                .routed_at
                .is_some_and(|at| at >= self.route_observed_at)
        {
            return false;
        }
        shell
            .core
            .workspaces
            .get(&self.owner.session.team_id)
            .is_some_and(|workspace| {
                native_path(
                    shell.connection.routable,
                    self.owner.transport.health(),
                    workspace.rt.is_connected(),
                    first_formatted_query,
                ) == self.path
            })
    }
}

fn owner_current(
    shell: &ShellState,
    epoch: u64,
    transport: &Arc<Transport>,
    owner: &WorkspaceSession,
) -> bool {
    shell.signed_in
        && !shell.loading
        && shell.core.account_epoch == epoch
        && shell.core.active_team.as_ref() == Some(&owner.team_id)
        && shell
            .core
            .transport
            .as_ref()
            .is_some_and(|actual| Arc::ptr_eq(actual, transport))
        && shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&owner.team_id))
            .is_some_and(|actual| crate::emoji_preferences::same_session(actual, owner))
        && shell
            .core
            .workspaces
            .get(&owner.team_id)
            .is_some_and(|w| w.team_id == owner.team_id && w.self_user_id == owner.user_id)
}
fn native_path(routable: bool, health: Health, realtime_connected: bool, query: &str) -> Path {
    // Exactly the established native connection::evaluate `live` conjunction.
    // Unknown is not a positive HTTP result; an actual connected owner socket
    // and observed route still make it live under that existing native policy.
    // No network probe, retry, timer or Health update is performed here.
    if routable && health != Health::Offline && realtime_connected && !query.is_empty() {
        Path::Remote
    } else {
        Path::Local
    }
}

#[cfg(test)]
mod tests;
