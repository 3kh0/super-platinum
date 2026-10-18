use super::*;
use serde_json::Value;
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport, file_share_policy::PostingFacts, models::User},
};
#[derive(Clone)]
pub(super) struct Owner {
    pub key: Key,
    pub epoch: u64,
    pub generation: u64,
    pub transport: Option<Arc<Transport>>,
    pub fixture: Option<super::fixture::Capability>,
    pub client: SlackClient,
    pub session: WorkspaceSession,
    pub(super) cookie: String,
    pub member: User,
    pub plan: Option<String>,
    pub cached: Value,
    pub rt: u64,
}
impl Owner {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        if crate::fixture::is_fixture() || shell.channel_tabs.fixture.is_some() {
            return super::fixture::capture(shell);
        }
        let key = key(shell)?;
        let credentials = shell.core.session.as_ref()?;
        let session = credentials.workspaces.get(&key.team)?.clone();
        let w = shell.core.workspaces.get(&key.team)?;
        if session.user_id != key.user
            || shell.self_account.user_id != key.user
            || session.team_id != key.team
        {
            return None;
        }
        let member = w.users.get(&key.user)?.clone();
        let cached = serde_json::to_value(w.channels.get(&key.channel)?).ok()?;
        Some(Self {
            key,
            epoch: shell.core.account_epoch,
            generation: shell.channel_generation,
            transport: Some(shell.core.transport.clone()?),
            fixture: None,
            client: shell.core.client.clone(),
            session,
            cookie: credentials.d_cookie.clone(),
            member,
            plan: w.team_plan.clone(),
            cached,
            rt: w.rt_generation,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell).is_some_and(|n| {
            n.key == self.key
                && n.epoch == self.epoch
                && n.generation == self.generation
                && n.cookie == self.cookie
                && n.session.token == self.session.token
                && n.session.url == self.session.url
                && n.session.enterprise_id == self.session.enterprise_id
                && match (&n.transport, &self.transport, &n.fixture, &self.fixture) {
                    (Some(a), Some(b), None, None) => Arc::ptr_eq(a, b),
                    (None, None, Some(a), Some(b)) => a.same(b),
                    _ => false,
                }
                && n.cached == self.cached
                && n.rt == self.rt
                && serde_json::to_value(&n.member).ok() == serde_json::to_value(&self.member).ok()
                && n.plan == self.plan
        })
    }
    pub fn account_current(&self, shell: &ShellState) -> bool {
        if let Some(capability) = &self.fixture {
            return capability.current(shell);
        }
        let Some(credentials) = shell.core.session.as_ref() else {
            return false;
        };
        let Some(session) = credentials.workspaces.get(&self.key.team) else {
            return false;
        };
        shell.core.active_account.as_ref() == Some(&self.key.account)
            && shell.core.account_epoch == self.epoch
            && credentials.d_cookie == self.cookie
            && session.token == self.session.token
            && session.url == self.session.url
            && session.user_id == self.key.user
            && session.enterprise_id == self.session.enterprise_id
            && shell.core.transport.as_ref().is_some_and(|t| {
                self.transport
                    .as_ref()
                    .is_some_and(|own| Arc::ptr_eq(t, own))
            })
    }
    pub async fn execute(
        &self,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, super_platinum_core::slack::Error> {
        match (&self.transport, &self.fixture) {
            (Some(t), None) => t.execute(request).await,
            (None, Some(f)) => f.execute(request).await,
            _ => Err(super_platinum_core::slack::Error::TransportNotConfigured),
        }
    }
    /// Fresh positive channel-manager proof, scoped to the original UI owner.
    /// Presentation permission is never reused as authority for a later write.
    pub async fn read_permission(
        &self,
        state: Signal<ShellState>,
        raw: &Value,
    ) -> Result<Permission, super_platinum_core::slack::Error> {
        use super_platinum_core::{
            channel_tabs::{ChannelPermissionRead, ManagerRead},
            slack::Error,
        };
        if !self.current(&state.peek()) {
            return Err(Error::Api("tabs_owner_changed".into()));
        }
        let local = self.permission(raw);
        if local != Permission::Unavailable {
            return Ok(local);
        }
        let mut facts = PostingFacts::new(self.key.channel.clone());
        facts.observe(raw, Some(&self.member));
        let Ok(read) = ManagerRead::capture(raw, &self.member, &facts) else {
            let Ok(read) = ChannelPermissionRead::capture(raw, &self.member, &facts, &self.session)
            else {
                return Ok(local);
            };
            let discovery = read
                .discovery(&self.client, &self.session)
                .map_err(|_| Error::Api("tabs_owner_changed".into()))?;
            let endpoint = self.execute(discovery).await?;
            if !self.current(&state.peek()) {
                return Err(Error::Api("tabs_owner_changed".into()));
            }
            let request = read
                .request(&self.client, &self.session, &endpoint)
                .map_err(|_| Error::Api("tabs_endpoint_response_unconfirmed".into()))?;
            let response = self.execute(request).await?;
            if !self.current(&state.peek()) {
                return Err(Error::Api("tabs_owner_changed".into()));
            }
            return read
                .permission(&response)
                .map_err(|_| Error::Api("tabs_permission_response_unconfirmed".into()));
        };
        let request = read
            .request(&self.client, &self.session)
            .map_err(|_| Error::Api("tabs_owner_changed".into()))?;
        let response = self.execute(request).await?;
        if !self.current(&state.peek()) {
            return Err(Error::Api("tabs_owner_changed".into()));
        }
        read.permission(&response)
            .map_err(|_| Error::Api("tabs_manager_response_unconfirmed".into()))
    }
    pub fn permission(&self, raw: &Value) -> Permission {
        let mut facts = PostingFacts::new(self.key.channel.clone());
        facts.observe(raw, Some(&self.member));
        super_platinum_core::channel_tabs::permission(
            raw,
            &self.member,
            &facts,
            self.session.enterprise_id.is_some() || self.session.team_id.starts_with('E'),
            self.plan.as_deref(),
        )
    }
}
