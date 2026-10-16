use super::*;
use super_platinum_core::{config::WorkspaceSession, slack::Transport};
pub(super) struct Owner {
    #[cfg(feature = "original-download-native-fixture")]
    pub fixture: Option<super::fixture::Input>,
    pub key: (Option<String>, String, String),
    epoch: u64,
    workspace: Option<WorkspaceSession>,
    cookie: Option<String>,
    pub transport: Option<Arc<Transport>>,
    media: crate::media::MediaRegistry,
    id: MediaAssetId,
    pub source: Option<String>,
}
impl Owner {
    pub fn capture(shell: &ShellState, id: MediaAssetId) -> Option<(Self, Option<Arc<[u8]>>)> {
        let team = shell.core.active_team.clone()?;
        let workspace = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&team))
            .cloned();
        let bytes = shell.media.ready_download_bytes(&id);
        let source = shell.media.original_download_url(&id);
        #[cfg(feature = "original-download-native-fixture")]
        let fixture = super::fixture::capture(shell, &id);
        #[cfg(feature = "original-download-native-fixture")]
        let fixture_admitted = fixture.is_some();
        #[cfg(not(feature = "original-download-native-fixture"))]
        let fixture_admitted = false;
        if !fixture_admitted
            && bytes.is_none()
            && (source.is_none() || workspace.is_none() || shell.core.transport.is_none())
        {
            return None;
        }
        let key = (
            shell.core.active_account.clone(),
            team,
            workspace
                .as_ref()
                .map(|w| w.user_id.clone())
                .unwrap_or_default(),
        );
        Some((
            Self {
                #[cfg(feature = "original-download-native-fixture")]
                fixture,
                key,
                epoch: shell.core.account_epoch,
                workspace,
                cookie: shell.core.session.as_ref().map(|s| s.d_cookie.clone()),
                transport: shell.core.transport.clone(),
                media: shell.media.clone(),
                id,
                source,
            },
            bytes,
        ))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_account == self.key.0
            && shell.core.active_team.as_deref() == Some(self.key.1.as_str())
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && shell.core.session.as_ref().map(|s| &s.d_cookie) == self.cookie.as_ref()
            && workspace_matches(
                self.workspace.as_ref(),
                shell
                    .core
                    .session
                    .as_ref()
                    .and_then(|s| s.workspaces.get(&self.key.1)),
            )
            && self.media.same_registry(&shell.media)
            && shell.media.original_download_url(&self.id) == self.source
    }
}
fn workspace_matches(a: Option<&WorkspaceSession>, b: Option<&WorkspaceSession>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.team_id == b.team_id
                && a.user_id == b.user_id
                && a.enterprise_id == b.enterprise_id
                && a.url == b.url
                && a.token == b.token
        }
        _ => false,
    }
}
