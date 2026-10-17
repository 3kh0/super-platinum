use crate::state::ShellState;
use std::sync::Arc;
use super_platinum_core::{config::WorkspaceSession, slack::Transport};

#[derive(Clone)]
pub(in crate::canvas) struct Scope {
    account: Option<String>,
    epoch: u64,
    pub team: String,
    user: String,
    transport: Option<Arc<Transport>>,
    session: Option<WorkspaceSession>,
    cookie: Option<String>,
}
impl Scope {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        if workspace.team_id != *team
            || workspace.self_user_id.is_empty()
            || workspace.self_user_id != shell.self_account.user_id
        {
            return None;
        }
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(team))
            .cloned();
        if session
            .as_ref()
            .is_some_and(|s| s.team_id != *team || s.user_id != workspace.self_user_id)
        {
            return None;
        }
        Some(Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            team: team.clone(),
            user: workspace.self_user_id.clone(),
            transport: shell.core.transport.clone(),
            session,
            cookie: shell
                .core
                .session
                .as_ref()
                .map(|session| session.d_cookie.clone()),
        })
    }
    /// Pending delivery belongs to the stable Slack identity even after its
    /// credentials rotate. This grants no permission to dispatch or adopt data.
    pub(super) fn stable_current(&self, shell: &ShellState) -> bool {
        self.account == shell.core.active_account
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.self_account.user_id == self.user
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|workspace| {
                    workspace.team_id == self.team && workspace.self_user_id == self.user
                })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.account_current(shell)
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.self_account.user_id == self.user
    }
    /// Cleanup may use the original workspace after a same-account workspace
    /// switch, but never after credentials, viewer, account or transport change.
    pub(super) fn account_current(&self, shell: &ShellState) -> bool {
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&self.team));
        self.cookie.as_deref()
            == shell
                .core
                .session
                .as_ref()
                .map(|session| session.d_cookie.as_str())
            && self.account == shell.core.active_account
            && self.epoch == shell.core.account_epoch
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.team_id == self.team && w.self_user_id == self.user)
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&self.session, session) {
                (Some(a), Some(b)) => {
                    a.team_id == b.team_id
                        && a.enterprise_id == b.enterprise_id
                        && a.user_id == b.user_id
                        && a.token == b.token
                        && a.url == b.url
                }
                (None, None) => true,
                _ => false,
            }
    }
    pub fn request(&self, shell: &ShellState) -> Option<crate::scheduled::Context> {
        if !self.current(shell) {
            return None;
        }
        Some(crate::scheduled::Context {
            transport: self.transport.clone()?,
            session: self.session.clone()?,
            client: shell.core.client.clone(),
            epoch: self.epoch,
        })
    }
    pub fn deleted(&self, shell: &ShellState, id: &str) -> bool {
        shell
            .core
            .workspaces
            .get(&self.team)
            .is_some_and(|w| w.deleted_files.contains(id))
    }
}
