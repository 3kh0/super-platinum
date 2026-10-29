//! Native numbered People journey, beside the unchanged bounded Edge adapter.
//! Metadata/routing are qualified native protocol policy. Genuine accepted
//! parameters and the original authenticated account still gate every request.
use super::*;
use crate::{
    app_home::channel_fetches::people_parameters::{self, Parameters},
    state::{ChannelTab, MainView},
};
#[cfg(test)]
use std::cell::Cell;
use std::{
    collections::BTreeMap,
    rc::Rc,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use super_platinum_core::{
    search::people_pages::{Activation, Book, Entry, Plan, Sessions, Ticket},
    slack::api::people_pages::{self as api, Page},
};
pub(crate) mod account_filter;
mod load;
mod profiles;
pub(crate) mod saved_sort;
mod sort_refresh;
pub(crate) use profiles::{RowSource, open};
pub(crate) use sort_refresh::{RefreshSource, after_sort_change, run_refresh};

#[derive(Clone, PartialEq, Eq)]
struct Account {
    account: String,
    epoch: u64,
    team: String,
    user: String,
}
impl Account {
    fn capture(s: &ShellState) -> Option<Self> {
        let team = s.core.active_team.clone()?;
        Some(Self {
            account: s.core.active_account.clone()?,
            epoch: s.core.account_epoch,
            user: s.core.workspaces.get(&team)?.self_user_id.clone(),
            team,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        Self::capture(s).as_ref() == Some(self)
    }
}
#[derive(Clone, Copy)]
enum Limit {
    None,
    Until(Instant),
    Forever,
}
struct Rate {
    owner: Account,
    limit: Mutex<Limit>,
}
impl Rate {
    fn cooling(&self) -> bool {
        let Ok(limit) = self.limit.lock() else {
            return true;
        };
        match *limit {
            Limit::None => false,
            Limit::Until(t) => Instant::now() < t,
            Limit::Forever => true,
        }
    }
    fn observe(&self, s: &ShellState, result: &Result<serde_json::Value, Error>) {
        if !self.owner.current(s) {
            return;
        }
        let seconds = match result {
            Err(Error::RateLimited { retry_after_secs }) => Some(retry_after_secs.unwrap_or(30)),
            Err(Error::HttpStatus {
                retry_after_secs: Some(seconds),
                ..
            }) => Some(*seconds),
            _ => None,
        };
        if let Some(seconds) = seconds {
            let limit = Instant::now()
                .checked_add(Duration::from_secs(seconds))
                .map(Limit::Until)
                .unwrap_or(Limit::Forever);
            let Ok(mut current) = self.limit.lock() else {
                return;
            };
            match (*current, limit) {
                (Limit::Forever, _) => {}
                (_, Limit::Forever) => *current = Limit::Forever,
                (Limit::Until(old), Limit::Until(new)) if old >= new => {}
                _ => *current = limit,
            }
        }
    }
}
// Native-owned cold People controls. No restored browser state, shared Message
// filters or recent-search MRU supplies their values or promoted history.
#[derive(Clone)]
struct Controls {
    sort: api::Sort,
    account_types: api::AccountType,
    selected_team: String,
    custom_fields: BTreeMap<String, Vec<String>>,
    timezones: Vec<String>,
    external_teams: Vec<String>,
    profile_filters: [Vec<String>; 5],
    recent_filter_in: Vec<String>,
    recent_filter_from: Vec<String>,
}
impl Controls {
    fn fresh() -> Self {
        Self {
            sort: api::Sort::Relevant,
            account_types: api::AccountType::Any,
            selected_team: String::new(),
            custom_fields: BTreeMap::new(),
            timezones: Vec::new(),
            external_teams: Vec::new(),
            profile_filters: Default::default(),
            recent_filter_in: Vec::new(),
            recent_filter_from: Vec::new(),
        }
    }
    fn supported(&self) -> bool {
        matches!(
            self.sort,
            api::Sort::Relevant | api::Sort::Name | api::Sort::NameDescending
        ) && matches!(
            self.account_types,
            api::AccountType::Any
                | api::AccountType::RegularMembers
                | api::AccountType::Deactivated
        ) && self.selected_team.is_empty()
            && self.custom_fields.is_empty()
            && self.timezones.is_empty()
            && self.external_teams.is_empty()
            && self.profile_filters.iter().all(Vec::is_empty)
            && self.recent_filter_in.is_empty()
            && self.recent_filter_from.is_empty()
    }
}
pub(crate) struct Module {
    account: Account,
    controls: Arc<Controls>,
    sessions: Sessions,
    rate: Arc<Rate>,
    scene: Option<Scene>,
    saved: saved_sort::State,
}
impl Module {
    pub(crate) fn clear_results(&mut self) {
        self.scene = None;
    }
    pub(crate) fn has_results_scope(&self) -> bool {
        self.scene.is_some()
    }
}
struct Scene {
    frame: Arc<Frame>,
    book: Book,
    metadata: BTreeMap<u32, Metadata>,
}
struct Metadata {
    page: Arc<Page>,
    users: BTreeMap<String, Arc<User>>,
}
struct Frame {
    owner: Identity,
    controls: Arc<Controls>,
    parameters: Parameters,
    rate: Arc<Rate>,
    query: String,
    dialog: u64,
    revision: u64,
    sort: super_platinum_core::search::SearchSort,
    channel: String,
    generation: u64,
    tab: ChannelTab,
}
impl Frame {
    fn effective_sort(&self) -> api::Sort {
        self.parameters.effective_sort(self.controls.sort)
    }
    fn input(&self, page: u32, entry: &Entry) -> api::Input<'_> {
        api::Input {
            query: &self.query,
            page,
            query_session: entry.query,
            search_session: entry.search,
            sort: self.effective_sort(),
            hide_deactivated: self.parameters.hide_deactivated(),
            primary_channel: &self.channel,
        }
    }
    fn request(&self, page: u32, entry: &Entry) -> Result<api::Request, Error> {
        api::request_for_account_type(
            self.parameters.client(),
            self.parameters.session(),
            self.input(page, entry),
            self.controls.account_types,
        )
    }
    fn current(&self, s: &ShellState) -> bool {
        self.controls.supported()
            && self.owner.account_current(s)
            && self
                .owner
                .matches_session(self.parameters.transport(), self.parameters.session())
            && self.parameters.current(s)
            && s.overlay == Some(Overlay::Search)
            && s.search_open_generation == self.dialog
            && s.search_pager.kind == SearchKind::People
            && s.search_pager.revision() == self.revision
            && s.search_pager.sort == self.sort
            && s.search_pager.query == self.query
            && s.search_query.trim() == self.query
            && s.main_view == MainView::Home
            && s.core.active_channel.as_deref() == Some(self.channel.as_str())
            && s.channel_generation == self.generation
            && s.channel_tab == self.tab
    }
}
fn scene(s: &ShellState) -> Option<&Scene> {
    s.search_people_module.as_ref()?.scene.as_ref().filter(|v| {
        Arc::ptr_eq(
            &s.search_people_module.as_ref().unwrap().controls,
            &v.frame.controls,
        ) && v.frame.current(s)
            && s.search_pager.page == (if v.book.active().is_some() { 1 } else { 0 })
            && s.search_pager.page_count == (if v.book.active().is_some() { 1 } else { 0 })
            && s.search_pager.total == 0
    })
}
fn owned<'a>(s: &'a ShellState, frame: &Arc<Frame>) -> Option<&'a Scene> {
    scene(s).filter(|v| Arc::ptr_eq(&v.frame, frame))
}
fn owned_mut<'a>(s: &'a mut ShellState, frame: &Arc<Frame>) -> Option<&'a mut Scene> {
    if owned(s, frame).is_none() {
        return None;
    }
    s.search_people_module.as_mut()?.scene.as_mut()
}
pub(crate) fn active(s: &ShellState) -> bool {
    scene(s).is_some()
}
pub(crate) fn has_scope(s: &ShellState) -> bool {
    s.search_pager.kind == SearchKind::People
        && s.search_people_module
            .as_ref()
            .is_some_and(|m| m.has_results_scope())
}
pub(crate) fn retired(s: &ShellState) -> bool {
    s.search_pager.kind == SearchKind::People
        && s.search_people_module
            .as_ref()
            .is_some_and(|m| m.scene.is_some())
        && scene(s).is_none()
}
pub(crate) fn rows(s: &ShellState) -> &[api::Hit] {
    scene(s)
        .and_then(|v| v.book.active())
        .map(|a| a.page.hits())
        .unwrap_or(&[])
}
pub(crate) fn page(s: &ShellState) -> u32 {
    scene(s)
        .and_then(|v| v.book.active())
        .map(|a| a.page.number())
        .unwrap_or(0)
}
pub(crate) fn page_count(s: &ShellState) -> u32 {
    scene(s).map(|v| v.book.page_count()).unwrap_or(0)
}
pub(crate) fn total(s: &ShellState) -> u64 {
    scene(s).map(|v| v.book.total()).unwrap_or(0)
}
pub(crate) fn loading(s: &ShellState) -> bool {
    scene(s).is_some_and(|v| v.book.pending().is_some())
}
pub(crate) fn row_user<'a>(s: &'a ShellState, index: usize) -> Option<&'a User> {
    let v = scene(s)?;
    let a = v.book.active()?;
    let id = a.page.hits().get(index)?.id();
    let w = s.core.workspaces.get(v.frame.owner.team())?;
    w.users.get(id).or_else(|| {
        v.metadata
            .get(&a.page.number())
            .filter(|m| Arc::ptr_eq(&m.page, &a.page))
            .and_then(|m| m.users.get(id))
            .map(Arc::as_ref)
    })
}
pub(crate) fn current_sort(s: &ShellState) -> Option<api::Sort> {
    Some(scene(s)?.frame.effective_sort())
}
pub(crate) fn local_sort_allowed(s: &ShellState) -> bool {
    scene(s).is_some_and(|v| v.frame.parameters.local_sort_allowed())
}
#[derive(Clone)]
pub(crate) struct SortSource {
    frame: Arc<Frame>,
    owner: Identity,
}
impl SortSource {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let v = scene(s)?;
        if !v.frame.parameters.local_sort_allowed()
            || v.book.active().is_none()
            || s.search_pager.loading
        {
            return None;
        }
        Some(Self {
            frame: v.frame.clone(),
            owner: Identity::capture(s)?,
        })
    }
}
fn plan_sort(s: &mut ShellState, source: SortSource, sort: api::Sort) -> Option<load::Job> {
    if !source.owner.current(s)
        || owned(s, &source.frame).is_none()
        || !source.frame.parameters.local_sort_allowed()
        || source.frame.controls.sort == sort
        || s.search_pager.loading
    {
        return None;
    }
    // Only the still-current rendered intent can replace the controls. Native
    // policy retires the old bag immediately; no earlier sort's cache is reused.
    let mut controls = source.frame.controls.as_ref().clone();
    controls.sort = sort;
    let request = s.search_pager.begin(
        s.core.account_epoch,
        source.frame.owner.team(),
        &source.frame.query,
        false,
    )?;
    s.cancel_search_open();
    s.clear_search_results();
    s.search_history_open = false;
    s.search_people_module.as_mut().unwrap().controls = Arc::new(controls);
    load::begin_in(
        s,
        &request,
        source.frame.parameters.transport(),
        source.frame.parameters.client(),
        source.frame.parameters.session(),
    )
}
pub(crate) fn change_sort(mut state: Signal<ShellState>, source: SortSource, sort: api::Sort) {
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_sort(&mut s, source, sort));
    if let Some(job) = job {
        dioxus::core::spawn_forever(load::run(state, job));
    }
}
#[derive(Clone)]
pub(crate) struct Selection {
    frame: Arc<Frame>,
    row_owner: Identity,
    page: u32,
}
impl Selection {
    pub(crate) fn capture(s: &ShellState, page: u32) -> Option<Self> {
        let v = scene(s)?;
        if page == 0 || page > v.book.page_count().max(1) {
            return None;
        }
        Some(Self {
            frame: v.frame.clone(),
            row_owner: Identity::capture(s)?,
            page,
        })
    }
}
#[derive(Clone)]
pub(crate) struct Retry {
    frame: Arc<Frame>,
    row_owner: Identity,
    ticket: Ticket,
}
impl Retry {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let v = scene(s)?;
        Some(Self {
            frame: v.frame.clone(),
            row_owner: Identity::capture(s)?,
            ticket: v.book.failure()?.clone(),
        })
    }
}
fn plan_selection(s: &mut ShellState, selection: Selection) -> Option<load::Job> {
    if !selection.row_owner.current(s) {
        return None;
    }
    let v = owned_mut(s, &selection.frame)?;
    match v.book.select(selection.page) {
        Plan::Fetch(ticket) => Some(load::Job {
            frame: selection.frame,
            ticket,
            initial: None,
            entry: load::EntryMode::Module,
        }),
        _ => None,
    }
}
pub(crate) fn select(mut state: Signal<ShellState>, selection: Selection) {
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_selection(&mut s, selection));
    if let Some(job) = job {
        dioxus::core::spawn_forever(load::run(state, job));
    }
}
fn plan_retry(s: &mut ShellState, retry: Retry) -> Option<load::Job> {
    if !retry.row_owner.current(s) {
        return None;
    }
    let first = owned(s, &retry.frame)
        .is_some_and(|v| v.book.active().is_none() && v.book.failure_current(&retry.ticket));
    if first {
        let epoch = s.core.account_epoch;
        let request =
            s.search_pager
                .retry_failed(epoch, retry.frame.owner.team(), &retry.frame.query)?;
        load::begin_in(
            s,
            &request,
            retry.frame.parameters.transport(),
            retry.frame.parameters.client(),
            retry.frame.parameters.session(),
        )
    } else {
        let v = owned_mut(s, &retry.frame)?;
        match v.book.retry(&retry.ticket) {
            Plan::Fetch(ticket) => Some(load::Job {
                frame: retry.frame,
                ticket,
                initial: None,
                entry: load::EntryMode::Module,
            }),
            _ => None,
        }
    }
}
pub(crate) fn retry(mut state: Signal<ShellState>, retry: Retry) {
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_retry(&mut s, retry));
    if let Some(job) = job {
        dioxus::core::spawn_forever(load::run(state, job));
    }
}
pub(crate) fn schedule_after_outage(
    state: Signal<ShellState>,
    visit: crate::bootstrap::reconnect::Visit,
) {
    let candidate = state.try_read().ok().and_then(|s| Retry::capture(&s));
    if let Some(candidate) = candidate {
        dioxus::core::spawn_forever(async move {
            if state.try_read().ok().is_some_and(|s| visit.current(&s)) {
                retry(state, candidate);
            }
        });
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
pub(super) async fn start(
    state: Signal<ShellState>,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> bool {
    let Some(job) = load::begin(state, request, transport, client, session) else {
        return false;
    };
    load::run(state, job).await;
    true
}
#[cfg(test)]
mod tests;
