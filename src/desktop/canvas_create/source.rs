use super::{Source, domain};
use crate::state::ShellState;
use dioxus::prelude::*;
use std::{future::Future, pin::Pin, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, Transport},
};

// Intentionally no Debug/Serialize: this live transport binding contains secrets.
// It is separate from durable stable Owner and is never persisted.
pub(in crate::canvas_create) struct LiveSource {
    state: Signal<ShellState>,
    visit: String,
    owner: domain::Owner,
    epoch: u64,
    cookie: String,
    client: SlackClient,
    session: WorkspaceSession,
    transport: Arc<Transport>,
}
impl LiveSource {
    pub(in crate::canvas_create) fn capture(state: Signal<ShellState>) -> Option<Self> {
        if crate::fixture::is_fixture() {
            return None;
        }
        let shell = state.peek();
        let account = shell.core.active_account.as_deref()?;
        let team = shell.core.active_team.as_deref()?;
        let credentials = shell.core.session.as_ref()?;
        let session = credentials.workspaces.get(team)?.clone();
        let workspace = shell.core.workspaces.get(team)?;
        if session.team_id != team
            || workspace.team_id != team
            || workspace.self_user_id != session.user_id
            || shell.self_account.user_id != session.user_id
        {
            return None;
        }
        Some(Self {
            visit: shell
                .canvas_create
                .as_ref()
                .filter(|_| shell.overlay == Some(crate::state::Overlay::CanvasCreate))?
                .visit
                .clone(),
            state,
            owner: domain::Owner::new(account, team, &session.user_id).ok()?,
            epoch: shell.core.account_epoch,
            cookie: credentials.d_cookie.clone(),
            client: shell.core.client.clone(),
            session,
            transport: shell.core.transport.clone()?,
        })
    }
}
impl Source for LiveSource {
    fn owner(&self) -> &domain::Owner {
        &self.owner
    }
    fn current(&self) -> bool {
        Self::capture(self.state).is_some_and(|fresh| {
            fresh.visit == self.visit
                && fresh.owner == self.owner
                && fresh.epoch == self.epoch
                && fresh.cookie == self.cookie
                && fresh.session.token == self.session.token
                && fresh.session.url == self.session.url
                && fresh.session.enterprise_id == self.session.enterprise_id
                && Arc::ptr_eq(&fresh.transport, &self.transport)
        })
    }
    fn client(&self) -> &SlackClient {
        &self.client
    }
    fn workspace(&self) -> &WorkspaceSession {
        &self.session
    }
    fn execute(
        &self,
        request: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, Error>> + '_>> {
        // Preserve Canvas's existing transport semantics. No new retries, no
        // execute_draft_mutation substitution, and no idempotency parameter.
        Box::pin(self.transport.execute(request))
    }
}
