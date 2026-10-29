//! Renderer-owned Files filter intent; query edits grant no file authority.
use super::{ChannelTab, MainView, Overlay, ShellState};
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    search::{SearchKind, SearchSort},
    search_query::{
        SearchFilters,
        file_types::{FileCategory, FileTypeEdit},
    },
    slack::{SlackClient, Transport},
};

#[derive(Clone)]
struct Owner {
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
    fn capture(s: &ShellState) -> Option<Self> {
        if s.overlay != Some(Overlay::Search) || s.search_pager.kind != SearchKind::Files {
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
    fn current(&self, s: &ShellState) -> bool {
        self.session.team_id == self.team
            && s.overlay == Some(Overlay::Search)
            && s.search_pager.kind == SearchKind::Files
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
}
struct Origin {
    owner: Owner,
    key: String,
}
impl Origin {
    fn new(owner: Owner) -> Arc<Self> {
        Arc::new(Self {
            owner,
            key: uuid::Uuid::new_v4().to_string(),
        })
    }
}
#[derive(Clone)]
pub(crate) struct Draft {
    origin: Arc<Origin>,
    types: FileTypeEdit,
}
#[derive(Clone)]
pub(crate) struct Source {
    origin: Arc<Origin>,
}
impl Source {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let d = s.search_file_type_filters.as_ref()?;
        let source = Self {
            origin: d.origin.clone(),
        };
        source.current(s).then_some(source)
    }
    fn current(&self, s: &ShellState) -> bool {
        self.origin.owner.current(s)
            && !s.search_history_open
            && s.search_filters.is_some()
            && s.search_file_type_filters
                .as_ref()
                .is_some_and(|d| Arc::ptr_eq(&d.origin, &self.origin))
    }
    pub(crate) fn key(&self) -> &str {
        &self.origin.key
    }
}
/// Old generic handlers cannot reach a new Files-owned draft through a shared
/// input or Apply function. Non-Files callers retain their existing contract.
pub(crate) fn allows(s: &ShellState, source: Option<&Source>) -> bool {
    match (source, s.search_file_type_filters.as_ref()) {
        (Some(source), Some(_)) => source.current(s),
        (None, None) => s.search_pager.kind != SearchKind::Files,
        _ => false,
    }
}
pub(crate) fn retire(s: &mut ShellState) {
    if s.search_file_type_filters.take().is_some() {
        s.search_filters = None;
        s.search_filter_error = None;
    }
}
#[derive(Clone)]
pub(crate) struct Toggle {
    owner: Option<Owner>,
    query: String,
    input: u64,
    dialog: u64,
    revision: u64,
    page: u32,
    kind: SearchKind,
    sort: SearchSort,
    account: Option<String>,
    epoch: u64,
    team: Option<String>,
    history: bool,
    open: bool,
    observed: Option<Arc<Origin>>,
}
impl Toggle {
    pub(crate) fn capture(s: &ShellState) -> Self {
        Self {
            owner: Owner::capture(s),
            query: s.search_query.clone(),
            input: s.search_input_revision,
            dialog: s.search_open_generation,
            revision: s.search_pager.revision(),
            page: s.search_pager.page,
            kind: s.search_pager.kind,
            sort: s.search_pager.sort,
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone(),
            history: s.search_history_open,
            open: s.search_filters.is_some(),
            observed: s
                .search_file_type_filters
                .as_ref()
                .map(|d| d.origin.clone()),
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        s.overlay == Some(Overlay::Search)
            && s.search_query == self.query
            && s.search_input_revision == self.input
            && s.search_open_generation == self.dialog
            && s.search_pager.revision() == self.revision
            && s.search_pager.page == self.page
            && s.search_pager.kind == self.kind
            && s.search_pager.sort == self.sort
            && s.core.active_account == self.account
            && s.core.account_epoch == self.epoch
            && s.core.active_team == self.team
            && s.search_history_open == self.history
            && s.search_filters.is_some() == self.open
            && self.owner.as_ref().is_none_or(|o| o.current(s))
            && match (&self.observed, s.search_file_type_filters.as_ref()) {
                (None, None) => true,
                (Some(o), Some(d)) => Arc::ptr_eq(o, &d.origin),
                _ => false,
            }
    }
}
pub(crate) fn toggle(s: &mut ShellState, source: Toggle) {
    if !source.current(s) {
        return;
    }
    if source.kind != SearchKind::Files {
        // Preserve the generic panel behavior, but a stale generic toggle cannot
        // close or replace an origin-bound Files draft.
        if s.search_file_type_filters.is_some() {
            return;
        }
        s.search_filter_error = None;
        if s.search_history_open {
            s.search_history_open = false;
            if s.search_filters.is_none() {
                s.search_filters = Some(SearchFilters::from_query(&s.search_query));
            }
        } else {
            s.search_filters = if s.search_filters.is_some() {
                None
            } else {
                Some(SearchFilters::from_query(&s.search_query))
            };
        }
        return;
    }
    let close = source.open && !source.history;
    // This is one serial local transition. The captured credentials remain
    // unchanged; only its dialog generation is rebased after our own cancel.
    s.cancel_search_open();
    s.search_filter_error = None;
    s.search_history_open = false;
    if close {
        s.search_filters = None;
        return;
    }
    s.search_filters = Some(SearchFilters::from_query(&s.search_query));
    if let Some(mut owner) = source.owner {
        owner.dialog = s.search_open_generation;
        if owner.current(s) {
            s.search_file_type_filters = Some(Draft {
                origin: Origin::new(owner),
                types: FileTypeEdit::capture(&s.search_query),
            });
        }
    }
}
pub(crate) fn selected(s: &ShellState, kind: FileCategory) -> bool {
    s.search_file_type_filters
        .as_ref()
        .map(|d| d.types.selected(kind))
        .unwrap_or_else(|| FileTypeEdit::capture(&s.search_query).selected(kind))
}
pub(crate) fn set(s: &mut ShellState, source: &Source, kind: FileCategory, value: bool) {
    if !source.current(s) {
        return;
    }
    s.search_filter_error = None;
    s.search_file_type_filters
        .as_mut()
        .unwrap()
        .types
        .set(kind, value);
}
pub(crate) fn clear(s: &mut ShellState, source: &Source, all_fields: bool) {
    if !source.current(s) {
        return;
    }
    let d = s.search_file_type_filters.as_mut().unwrap();
    d.types.clear_selected();
    d.origin = Origin::new(d.origin.owner.clone());
    if all_fields {
        s.search_filters = Some(SearchFilters::default());
    }
    s.search_filter_error = None;
}
pub(crate) struct Submission {
    owner: Owner,
}
impl Submission {
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.search_filters.is_none()
            && s.search_file_type_filters.is_none()
            && !s.search_history_open
    }
}
pub(crate) fn apply(s: &mut ShellState, source: &Source) -> Option<Submission> {
    if !source.current(s) {
        return None;
    }
    let d = s.search_file_type_filters.as_ref().unwrap();
    let filters = s.search_filters.as_ref().unwrap();
    let result = d
        .types
        .apply(&s.search_query)
        .and_then(|query| filters.apply(&query));
    match result {
        Ok(query) => {
            let mut owner = source.origin.owner.clone();
            s.replace_search_query(query);
            s.search_file_type_filters = None;
            s.search_filters = None;
            s.search_filter_error = None;
            owner.query = s.search_query.clone();
            owner.input = s.search_input_revision;
            owner.dialog = s.search_open_generation;
            owner.revision = s.search_pager.revision();
            owner.page = s.search_pager.page;
            owner.current(s).then_some(Submission { owner })
        }
        Err(error) => {
            s.search_filter_error = Some(error);
            None
        }
    }
}
#[cfg(test)]
pub(crate) mod tests;
