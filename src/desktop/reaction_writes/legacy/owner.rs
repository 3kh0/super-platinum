use super::*;
pub(super) struct Owner {
    account: Option<String>,
    epoch: u64,
    cookie: String,
    pub transport: Arc<Transport>,
    pub client: SlackClient,
    pub session: WorkspaceSession,
}
impl Owner {
    pub(super) fn capture(
        s: &ShellState,
        transport: Arc<Transport>,
        client: SlackClient,
        session: WorkspaceSession,
    ) -> Option<Self> {
        if session.team_id.is_empty() || session.user_id.is_empty() || session.token.is_empty() {
            return None;
        }
        let owner = Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            cookie: s.core.session.as_ref()?.d_cookie.clone(),
            transport,
            client,
            session,
        };
        owner.current(s).then_some(owner)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        s.core.active_account == self.account
            && s.core.account_epoch == self.epoch
            && s.core.client.same_configuration(&self.client)
            && s.core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && self.transport.d_cookie() == self.cookie
            && s.core.session.as_ref().is_some_and(|session| {
                session.d_cookie == self.cookie
                    && session
                        .workspaces
                        .get(&self.session.team_id)
                        .is_some_and(|w| {
                            w.team_id == self.session.team_id
                                && w.user_id == self.session.user_id
                                && w.token == self.session.token
                                && w.url == self.session.url
                                && w.enterprise_id == self.session.enterprise_id
                        })
            })
            && s.core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|w| w.self_user_id == self.session.user_id)
    }
    pub(super) fn actor(&self) -> Actor {
        Actor {
            account: self.account.clone().unwrap_or_default(),
            team: self.session.team_id.clone(),
            user: self.session.user_id.clone(),
        }
    }
}
