//! Original fixed-Messages credentials and renderer/query/page frame.
use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport},
};
#[derive(Clone)]
pub(super) struct Owner {
    account: String,
    epoch: u64,
    team: String,
    user: String,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    cookie: String,
    query: String,
    input: u64,
    dialog: u64,
    revision: u64,
    page: u32,
    sort: SearchSort,
    main: MainView,
    channel: Option<String>,
    generation: u64,
    thread: u64,
    tab: ChannelTab,
}
impl Owner {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        if s.overlay != Some(Overlay::Search) || s.search_pager.kind != SearchKind::Messages {
            return None;
        }
        let team = s.core.active_team.clone()?;
        let sessions = s.core.session.as_ref()?;
        let owner = Self {
            account: s.core.active_account.clone()?,
            epoch: s.core.account_epoch,
            user: s.core.workspaces.get(&team)?.self_user_id.clone(),
            transport: s.core.transport.clone()?,
            client: s.core.client.clone(),
            session: sessions.workspaces.get(&team)?.clone(),
            cookie: sessions.d_cookie.clone(),
            team,
            query: s.search_query.clone(),
            input: s.search_input_revision,
            dialog: s.search_open_generation,
            revision: s.search_pager.revision(),
            page: s.search_pager.page,
            sort: s.search_pager.sort,
            main: s.main_view,
            channel: s.core.active_channel.clone(),
            generation: s.channel_generation,
            thread: s.thread_generation,
            tab: s.channel_tab,
        };
        owner.current(s).then_some(owner)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.session.team_id == self.team
            && s.overlay == Some(Overlay::Search)
            && s.search_pager.kind == SearchKind::Messages
            && s.core.active_account.as_ref() == Some(&self.account)
            && s.core.account_epoch == self.epoch
            && s.core.active_team.as_ref() == Some(&self.team)
            && s.core.workspaces.get(&self.team).is_some_and(|w| {
                w.self_user_id == self.user && w.self_user_id == self.session.user_id
            })
            && s.core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport) && t.d_cookie() == self.cookie)
            && s.core.client.same_configuration(&self.client)
            && s.core.session.as_ref().is_some_and(|sessions| {
                sessions.d_cookie == self.cookie
                    && sessions.workspaces.get(&self.team).is_some_and(|w| {
                        w.team_id == self.session.team_id
                            && w.user_id == self.session.user_id
                            && w.token == self.session.token
                            && w.url == self.session.url
                            && w.enterprise_id == self.session.enterprise_id
                    })
            })
            && s.search_query == self.query
            && s.search_input_revision == self.input
            && s.search_open_generation == self.dialog
            && s.search_pager.revision() == self.revision
            && s.search_pager.page == self.page
            && s.search_pager.sort == self.sort
            && s.main_view == self.main
            && s.core.active_channel == self.channel
            && s.channel_generation == self.generation
            && s.thread_generation == self.thread
            && s.channel_tab == self.tab
    }
    pub(super) fn rebase_dialog(&mut self, s: &ShellState) {
        self.dialog = s.search_open_generation;
    }
    pub(super) fn rebase_query(&mut self, s: &ShellState) {
        self.query = s.search_query.clone();
        self.input = s.search_input_revision;
        self.dialog = s.search_open_generation;
        self.revision = s.search_pager.revision();
        self.page = s.search_pager.page;
    }
}
