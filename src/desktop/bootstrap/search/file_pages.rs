//! Explicit private Files find journey; no search hit becomes a file capability.
//! Cold request/Primary mapping and transport metadata are native policy.
use super::{
    recovery::{Cursor, Identity},
    *,
};
use crate::{
    app_home::channel_fetches::file_search_parameters::{self, Parameters},
    state::{ChannelTab, MainView, Overlay},
};
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use super_platinum_core::{
    search::{
        SearchKind, SearchSort,
        file_pages::{Book, Plan, Ticket},
        people_pages::Sessions,
    },
    slack::{Error, api::file_pages as api},
};
mod load;
mod parameters;
pub(crate) mod saved;
use parameters::ReadParameters;
#[cfg(test)]
pub(crate) mod test_io;

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
struct Rate {
    owner: Account,
    until: Mutex<Option<Instant>>,
    forever: Mutex<bool>,
}
impl Rate {
    fn cooling(&self) -> bool {
        self.forever.lock().map_or(true, |v| *v)
            || self
                .until
                .lock()
                .map_or(true, |v| v.is_some_and(|t| Instant::now() < t))
    }
    fn observe(&self, s: &ShellState, result: &Result<serde_json::Value, Error>) {
        if !self.owner.current(s) {
            return;
        }
        let seconds = match result {
            Err(Error::RateLimited { retry_after_secs }) => Some(retry_after_secs.unwrap_or(30)),
            Err(Error::HttpStatus {
                retry_after_secs: Some(n),
                ..
            }) => Some(*n),
            _ => None,
        };
        if let Some(seconds) = seconds {
            if let Some(next) = Instant::now().checked_add(Duration::from_secs(seconds)) {
                if let Ok(mut old) = self.until.lock() {
                    if old.is_none_or(|t| t < next) {
                        *old = Some(next);
                    }
                }
            } else if let Ok(mut forever) = self.forever.lock() {
                *forever = true;
            }
        }
    }
}
/// A real native cold constructor followed by promotion. Rich history and team
/// filters are deliberately outside this represented plain-term request model.
struct Promoted {
    query: String,
    sort: api::Sort,
    selected_team: String,
    history_in: Vec<String>,
    history_from: Vec<String>,
}
impl Promoted {
    fn cold_and_promote(query: &str, sort: api::Sort) -> Option<Self> {
        if query.trim().is_empty() || query.contains([':', '"']) {
            return None;
        }
        Some(Self {
            query: query.into(),
            sort,
            selected_team: String::new(),
            history_in: Vec::new(),
            history_from: Vec::new(),
        })
    }
    fn supported(&self) -> bool {
        self.selected_team.is_empty()
            && self.history_in.is_empty()
            && self.history_from.is_empty()
            && !self.query.contains([':', '"'])
    }
}
pub(crate) struct Module {
    rate: Arc<Rate>,
    sessions: Sessions,
    scene: Option<Scene>,
    saved: saved::State,
}
impl Module {
    pub(crate) fn clear_results(&mut self) {
        self.scene = None;
    }
}
struct Scene {
    frame: Arc<Frame>,
    book: Book,
    control: Arc<()>,
}
struct Frame {
    owner: Identity,
    parameters: ReadParameters,
    promoted: Arc<Promoted>,
    revision: u64,
    legacy_sort: SearchSort,
    channel: String,
    generation: u64,
    thread: u64,
    tab: ChannelTab,
    file_revision: u64,
}
impl Frame {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && !s.search_history_open
            && s.search_filters.is_none()
            && self.parameters.current(s)
            && self.promoted.supported()
            && self
                .owner
                .matches_session(self.parameters.transport(), self.parameters.session())
            && self.parameters.client().same_configuration(&s.core.client)
            && s.search_pager.kind == SearchKind::Files
            && s.search_pager.revision() == self.revision
            && s.search_pager.sort == self.legacy_sort
            && s.search_pager.query == self.promoted.query
            && s.search_query.trim() == self.promoted.query
            && s.main_view == MainView::Home
            && s.core.active_channel.as_deref() == Some(&self.channel)
            && s.channel_generation == self.generation
            && s.thread_generation == self.thread
            && s.channel_tab == self.tab
    }
}
fn scene(s: &ShellState) -> Option<&Scene> {
    s.search_file_module
        .as_ref()?
        .scene
        .as_ref()
        .filter(|v| v.frame.current(s))
}
fn owned<'a>(s: &'a ShellState, f: &Arc<Frame>) -> Option<&'a Scene> {
    scene(s).filter(|v| Arc::ptr_eq(&v.frame, f))
}
fn owned_mut<'a>(s: &'a mut ShellState, f: &Arc<Frame>) -> Option<&'a mut Scene> {
    owned(s, f)?;
    s.search_file_module.as_mut()?.scene.as_mut()
}
pub(crate) fn has_scope(s: &ShellState) -> bool {
    s.search_pager.kind == SearchKind::Files
        && s.search_file_module
            .as_ref()
            .is_some_and(|m| m.scene.is_some())
}
pub(crate) fn active(s: &ShellState) -> bool {
    scene(s).is_some()
}
pub(crate) fn visible(s: &ShellState, hit: &api::Hit) -> bool {
    !hit.tombstoned()
        && !s
            .core
            .active_team
            .as_ref()
            .and_then(|team| s.core.workspaces.get(team))
            .is_some_and(|workspace| workspace.deleted_files.contains(hit.id()))
        && !scene(s).is_some_and(|v| {
            crate::file_events::changed_since(
                s,
                v.frame.owner.team(),
                hit.id(),
                v.frame.file_revision,
            )
        })
}
pub(crate) fn rows(s: &ShellState) -> &[api::Hit] {
    scene(s)
        .filter(|v| v.book.pending().is_none())
        .and_then(|v| v.book.active())
        .map(|a| a.page.hits())
        .unwrap_or_default()
}
pub(crate) fn status(s: &ShellState) -> Option<(u32, u32, u32, u64, bool, bool, api::Sort)> {
    let v = scene(s)?;
    let shown = v.book.active().map(|a| a.page.number()).unwrap_or(0);
    let target = v
        .book
        .pending()
        .map(|t| t.page)
        .or(v.book.failed_page())
        .unwrap_or(shown);
    Some((
        shown,
        target,
        v.book.page_count(),
        v.book.total(),
        v.book.pending().is_some(),
        v.book.failure().is_some(),
        v.frame.promoted.sort,
    ))
}
#[derive(Clone)]
pub(crate) struct EntrySource {
    owner: Identity,
    parameters: Parameters,
    cursor: Cursor,
    observed: Option<(Arc<Frame>, Arc<()>)>,
    channel: String,
    generation: u64,
    thread: u64,
    tab: ChannelTab,
    loading: bool,
    failed: bool,
}
impl EntrySource {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        if s.overlay != Some(Overlay::Search)
            || s.search_pager.kind != SearchKind::Files
            || s.main_view != MainView::Home
            || s.search_filters.is_some()
            || s.search_history_open
            || s.search_pager.loading
            || s.search_pager.query != s.search_query.trim()
            || s.search_pager.page == 0 && !has_scope(s)
            || Promoted::cold_and_promote(s.search_query.trim(), api::Sort::Relevant).is_none()
            || scene(s).is_some_and(|v| v.book.pending().is_some())
        {
            return None;
        }
        let owner = Identity::capture(s)?;
        let parameters = file_search_parameters::capture(s, owner.team())?;
        if !owner.matches_session(parameters.transport(), parameters.session())
            || !parameters.client().same_configuration(&s.core.client)
        {
            return None;
        }
        Some(Self {
            owner,
            parameters,
            cursor: Cursor::capture(s),
            observed: s
                .search_file_module
                .as_ref()
                .and_then(|m| m.scene.as_ref())
                .map(|v| (v.frame.clone(), v.control.clone())),
            channel: s.core.active_channel.clone()?,
            generation: s.channel_generation,
            thread: s.thread_generation,
            tab: s.channel_tab,
            loading: s.search_pager.loading,
            failed: s.search_pager.failed,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && self.parameters.current(s)
            && self.cursor.current(s)
            && s.search_filters.is_none()
            && !s.search_history_open
            && s.main_view == MainView::Home
            && s.core.active_channel.as_ref() == Some(&self.channel)
            && s.channel_generation == self.generation
            && s.thread_generation == self.thread
            && s.channel_tab == self.tab
            && s.search_pager.loading == self.loading
            && s.search_pager.failed == self.failed
            && match (
                &self.observed,
                s.search_file_module.as_ref().and_then(|m| m.scene.as_ref()),
            ) {
                (None, None) => true,
                (Some((f, c)), Some(v)) => {
                    Arc::ptr_eq(f, &v.frame)
                        && Arc::ptr_eq(c, &v.control)
                        && (v.book.pending().is_none() || !v.frame.current(s))
                }
                _ => false,
            }
    }
}
fn plan_entry(s: &mut ShellState, source: EntrySource, sort: api::Sort) -> Option<load::Job> {
    if !source.current(s) {
        return None;
    }
    let promoted = Arc::new(Promoted::cold_and_promote(s.search_query.trim(), sort)?);
    let team = source.owner.team().to_owned();
    let request = s
        .search_pager
        .begin(s.core.account_epoch, &team, &promoted.query, false)?;
    s.clear_search_results();
    s.cancel_search_open();
    let owner = Identity::capture(s)?;
    let frame = Arc::new(Frame {
        owner: owner.clone(),
        parameters: ReadParameters::Local(source.parameters),
        promoted,
        revision: s.search_pager.revision(),
        legacy_sort: s.search_pager.sort,
        channel: source.channel,
        generation: source.generation,
        thread: source.thread,
        tab: source.tab,
        file_revision: crate::file_events::snapshot_revision(s),
    });
    if !frame.current(s) {
        s.search_pager.fail(&request);
        return None;
    }
    let mut book = Book::default();
    let Plan::Fetch(ticket) = book.select(1) else {
        return None;
    };
    if s.search_file_module
        .as_ref()
        .is_none_or(|m| !m.rate.owner.current(s))
    {
        s.search_file_module = Some(Module {
            rate: Arc::new(Rate {
                owner: Account::capture(s)?,
                until: Mutex::new(None),
                forever: Mutex::new(false),
            }),
            sessions: Sessions::default(),
            scene: None,
            saved: saved::State::default(),
        });
    }
    s.search_file_module.as_mut()?.scene = Some(Scene {
        frame: frame.clone(),
        book,
        control: Arc::new(()),
    });
    Some(load::Job {
        frame,
        ticket,
        initial: Some(request),
        restored: None,
    })
}
pub(crate) fn change_sort(mut state: Signal<ShellState>, source: EntrySource, sort: api::Sort) {
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_entry(&mut s, source, sort));
    if let Some(job) = job {
        dioxus::core::spawn_forever(load::run(state, job));
    }
}
#[derive(Clone)]
pub(crate) struct Control {
    frame: Arc<Frame>,
    control: Arc<()>,
    cursor: Cursor,
}
impl Control {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let v = scene(s)?;
        if s.search_filters.is_some() || s.search_history_open || v.book.pending().is_some() {
            return None;
        }
        Some(Self {
            frame: v.frame.clone(),
            control: v.control.clone(),
            cursor: Cursor::capture(s),
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        self.cursor.current(s)
            && s.search_filters.is_none()
            && !s.search_history_open
            && owned(s, &self.frame).is_some_and(|v| {
                Arc::ptr_eq(&v.control, &self.control) && v.book.pending().is_none()
            })
    }
}
fn plan(s: &mut ShellState, source: Control, page: u32, retry: bool) -> Option<load::Job> {
    if !source.current(s) {
        return None;
    }
    if retry
        && owned(s, &source.frame)
            .is_some_and(|v| v.book.active().is_none() && v.book.failed_page() == Some(page))
    {
        if matches!(source.frame.parameters, ReadParameters::Saved(_)) {
            return saved::restart(s);
        }
        let entry = EntrySource::capture(s)?;
        return plan_entry(s, entry, source.frame.promoted.sort);
    }
    let v = owned_mut(s, &source.frame)?;
    if retry && v.book.failed_page() != Some(page) {
        return None;
    }
    let planned = if retry {
        v.book.retry(&v.book.failure()?.clone())
    } else {
        v.book.select(page)
    };
    match planned {
        Plan::Refused => None,
        Plan::Cached => {
            v.control = Arc::new(());
            None
        }
        Plan::Fetch(ticket) => {
            v.control = Arc::new(());
            Some(load::Job {
                frame: source.frame,
                ticket,
                initial: None,
                restored: None,
            })
        }
    }
}
pub(crate) fn choose(mut state: Signal<ShellState>, source: Control, page: u32) {
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| plan(&mut s, source, page, false));
    if let Some(job) = job {
        dioxus::core::spawn_forever(load::run(state, job));
    }
}
pub(crate) fn retry(mut state: Signal<ShellState>, source: Control) {
    let job = state.try_write().ok().and_then(|mut s| {
        let page = owned(&s, &source.frame)?.book.failed_page()?;
        plan(&mut s, source, page, true)
    });
    if let Some(job) = job {
        dioxus::core::spawn_forever(load::run(state, job));
    }
}
fn plan_recovery(
    s: &mut ShellState,
    source: Control,
    visit: crate::bootstrap::reconnect::Visit,
) -> Option<load::Job> {
    if !visit.current(s) {
        return None;
    }
    let page = owned(s, &source.frame)?.book.failed_page()?;
    let mut job = plan(s, source, page, true)?;
    job.restored = Some(visit);
    Some(job)
}
pub(in crate::bootstrap) fn schedule_after_outage(
    mut state: Signal<ShellState>,
    visit: crate::bootstrap::reconnect::Visit,
) {
    let source = state.try_read().ok().and_then(|s| Control::capture(&s));
    if let Some(source) = source {
        dioxus::core::spawn_forever(async move {
            let job = state
                .try_write()
                .ok()
                .and_then(|mut s| plan_recovery(&mut s, source, visit));
            if let Some(job) = job {
                load::run(state, job).await;
            }
        });
    }
}
/// Enter on an existing private scope never falls back into the legacy backend,
/// including expired parameters. A fresh explicit intent may re-admit new facts.
pub(super) async fn refresh(mut state: Signal<ShellState>) -> bool {
    if !has_scope(&state.peek()) {
        return false;
    }
    let job = state.try_write().ok().and_then(|mut s| {
        if s.search_file_module
            .as_ref()?
            .scene
            .as_ref()
            .is_some_and(|v| matches!(v.frame.parameters, ReadParameters::Saved(_)))
        {
            return saved::restart(&mut s);
        }
        let source = EntrySource::capture(&s)?;
        let sort = s
            .search_file_module
            .as_ref()?
            .scene
            .as_ref()?
            .frame
            .promoted
            .sort;
        plan_entry(&mut s, source, sort)
    });
    if let Some(job) = job {
        load::run(state, job).await;
    }
    true
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
#[cfg(test)]
pub(crate) mod tests;
