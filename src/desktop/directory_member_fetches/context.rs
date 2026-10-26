use super::*;
use super_platinum_core::{config::Session, slack::models::User};

/// Shared account/read authority, intentionally independent of active rail,
/// dialog or consumer query. Contains secrets; no Debug/Serialize implementation.
pub(crate) struct Context {
    pub(crate) account: String,
    pub(crate) session: WorkspaceSession,
    pub(crate) transport: Arc<Transport>,
    pub(crate) client: SlackClient,
    pub(crate) epoch: u64,
    auth: Session,
    rt_generation: u64,
}
impl Context {
    pub(crate) fn capture(shell: &ShellState, team: &str) -> Result<Self, Error> {
        let account = shell.core.active_account.clone().ok_or_else(unavailable)?;
        let auth = shell.core.session.clone().ok_or_else(unavailable)?;
        let session = auth.workspaces.get(team).cloned().ok_or_else(unavailable)?;
        let workspace = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
        if workspace.team_id != session.team_id || workspace.self_user_id != session.user_id {
            return Err(unavailable());
        }
        Ok(Self {
            account,
            session,
            transport: shell.core.transport.clone().ok_or_else(unavailable)?,
            client: shell.core.client.clone(),
            epoch: shell.core.account_epoch,
            auth,
            rt_generation: workspace.rt_generation,
        })
    }
    /// Captured socket generation; reading it grants no current-owner authority.
    pub(crate) fn realtime_generation(&self) -> u64 {
        self.rt_generation
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.account_current(shell)
            && shell
                .core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| w.rt_generation == self.rt_generation)
    }
    pub(crate) fn account_current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_ref() == Some(&self.account)
            && shell.core.account_epoch == self.epoch
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && shell
                .core
                .session
                .as_ref()
                .is_some_and(|s| same_auth(s, &self.auth))
            && shell
                .core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| {
                    w.team_id == self.session.team_id && w.self_user_id == self.session.user_id
                })
    }
    pub(super) fn key(&self) -> (String, String, String) {
        (
            self.account.clone(),
            self.session.team_id.clone(),
            self.session.user_id.clone(),
        )
    }
}
fn same_workspace(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.enterprise_id == b.enterprise_id
        && a.user_id == b.user_id
        && a.name == b.name
        && a.url == b.url
        && a.token == b.token
}
fn same_auth(a: &Session, b: &Session) -> bool {
    a.d_cookie == b.d_cookie
        && a.unified_user_client_enabled == b.unified_user_client_enabled
        && a.workspaces.len() == b.workspaces.len()
        && a.workspaces.iter().all(|(id, w)| {
            b.workspaces
                .get(id)
                .is_some_and(|other| same_workspace(w, other))
        })
}
pub(super) fn native_snapshot(
    users: &HashMap<String, User>,
) -> Result<BTreeMap<String, Value>, Error> {
    users
        .iter()
        .map(|(id, user)| {
            serde_json::to_value(user)
                .map(|v| (id.clone(), v))
                .map_err(|_| unavailable())
        })
        .collect()
}
