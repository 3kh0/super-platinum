//! Account-owned DM feed reads and row navigation; no conversation creation.
use crate::state::ShellState;
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport},
};
mod feed;
mod navigation;
pub(crate) use feed::load;
pub(crate) use navigation::{Frame, Opening, Source, cancel, is_opening, open};

// Shared once per render/request. Never Debug, serialize, persist or report it.
struct Owner {
    account: String,
    epoch: u64,
    team: String,
    session: WorkspaceSession,
    cookie: String,
    transport: Arc<Transport>,
    client: SlackClient,
}
impl Owner {
    fn capture(shell: &ShellState) -> Option<Arc<Self>> {
        if !shell.signed_in || shell.loading {
            return None;
        }
        let team = shell.core.active_team.clone()?;
        let session = shell.core.session.as_ref()?;
        let owner = Arc::new(Self {
            account: shell.core.active_account.clone()?,
            epoch: shell.core.account_epoch,
            session: session.workspaces.get(&team)?.clone(),
            cookie: session.d_cookie.clone(),
            transport: shell.core.transport.clone()?,
            client: shell.core.client.clone(),
            team,
        });
        owner.current(shell).then_some(owner)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.session.team_id == self.team
            && shell.core.active_account.as_ref() == Some(&self.account)
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport) && t.d_cookie() == self.cookie)
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.self_user_id == self.session.user_id)
            && shell.core.session.as_ref().is_some_and(|s| {
                s.d_cookie == self.cookie
                    && s.workspaces.get(&self.team).is_some_and(|w| {
                        w.team_id == self.session.team_id
                            && w.user_id == self.session.user_id
                            && w.token == self.session.token
                            && w.url == self.session.url
                            && w.enterprise_id == self.session.enterprise_id
                    })
            })
    }
}

mod fixture;
pub(crate) use fixture::{
    install as install_fixture, install_initial as install_initial_fixture,
    install_paging as install_paging_fixture,
};

mod paging;
pub(crate) use paging::{Source as PageSource, load_more, load_more_if};

mod initial;
pub(crate) use initial::{Source as InitialSource, load as load_initial, phase as initial_phase};

pub(crate) use fixture::install_facts as install_facts_fixture;
