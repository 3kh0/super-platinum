//! Explicit joined-channel metadata browsing. No join/history/canonical grant.
use crate::state::{MainView, Overlay, ShellState};
use dioxus::prelude::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        Error, SlackClient, Transport,
        api::channel_directory::{self as api, Kind, Page, Query, Scope, Sort},
    },
};
mod input;
mod open;
mod read;
mod retention;
mod view;
pub(crate) use retention::remember_on_close;
pub(crate) use view::dialog;

#[derive(Default)]
pub(crate) struct State {
    origin: Arc<()>,
    revision: u64,
    families: Vec<Arc<Family>>,
    entry: Option<Entry>,
    draft: String,
    // Local input is passive; draft remains the committed request coordinate.
    edited: Option<String>,
    kind: Kind,
    browse_sort: Option<Sort>,
    search_sort: Option<Sort>,
    pending: Option<Arc<read::Ticket>>,
    loaded: Option<Arc<Loaded>>,
    selected: Option<String>,
    selected_origin: Arc<()>,
    failed: Option<Query>,
    error: Option<&'static str>,
    presentation: Option<Page>,
    presentation_selected: Option<String>,
    cursors: std::collections::BTreeMap<u32, Option<String>>,
}
impl State {
    fn sort(&self) -> Sort {
        if self.draft.trim().is_empty() {
            self.browse_sort.unwrap_or(Sort::Name)
        } else {
            self.search_sort.unwrap_or(Sort::Relevant)
        }
    }
    fn busy(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.active.load(Ordering::Acquire) && p.revision == self.revision)
    }
    fn query(&self) -> Query {
        Query {
            text: self.draft.clone(),
            scope: Scope::Joined,
            kind: self.kind,
            sort: self.sort(),
            page: 1,
            cursor: None,
        }
    }
}

// The family lives across local reentry, so an original retired429 still holds
// queued peers for that same account/session, never a replacement owner.
struct Family {
    owner: crate::bootstrap::ReadOwner,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    deadline: Mutex<Option<Instant>>,
    overflowed: AtomicBool,
    retained: Mutex<retention::Retained>,
}
impl Family {
    fn capture(s: &ShellState) -> Option<Arc<Self>> {
        let team = s.core.active_team.as_ref()?;
        let transport = s.core.transport.clone()?;
        let session = s.core.session.as_ref()?.workspaces.get(team)?.clone();
        let owner = crate::bootstrap::ReadOwner::capture(&s.core, &transport, &session)?;
        Some(Arc::new(Self {
            owner,
            transport,
            client: s.core.client.clone(),
            session,
            deadline: Mutex::new(None),
            overflowed: AtomicBool::new(false),
            retained: Mutex::new(retention::Retained::default()),
        }))
    }
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(&s.core) && s.core.client.same_configuration(&self.client)
    }
    fn ready(&self) -> bool {
        !self.overflowed.load(Ordering::Acquire)
            && self
                .deadline
                .lock()
                .is_ok_and(|d| d.is_none_or(|at| Instant::now() >= at))
    }
    fn observe(&self, result: &Result<serde_json::Value, Error>) {
        let seconds = match result {
            Err(Error::RateLimited { retry_after_secs }) => *retry_after_secs,
            Err(Error::HttpStatus {
                status: 429,
                retry_after_secs,
            }) => *retry_after_secs,
            _ => return,
        }
        .unwrap_or(30)
        .max(1);
        let Some(at) = Instant::now().checked_add(Duration::from_secs(seconds)) else {
            self.overflowed.store(true, Ordering::Release);
            return;
        };
        if let Ok(mut deadline) = self.deadline.lock() {
            if deadline.is_none_or(|old| old < at) {
                *deadline = Some(at);
            }
        } else {
            self.overflowed.store(true, Ordering::Release);
        }
    }
}

#[derive(Clone)]
struct Frame {
    channel: u64,
    thread: u64,
    shortcut: u64,
    view: MainView,
}
impl Frame {
    fn capture(s: &ShellState) -> Self {
        Self {
            channel: s.channel_generation,
            thread: s.thread_generation,
            shortcut: s.shortcut_generation,
            view: s.main_view,
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        self.channel == s.channel_generation
            && self.thread == s.thread_generation
            && self.shortcut == s.shortcut_generation
            && self.view == s.main_view
    }
}

#[derive(Clone)]
pub(crate) struct Launch {
    family: Arc<Family>,
    frame: Frame,
}
impl Launch {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        if !s.signed_in
            || s.loading
            || s.main_view != MainView::Home
            || s.overlay.is_some()
            || std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
        {
            return None;
        }
        Some(Self {
            family: Family::capture(s)?,
            frame: Frame::capture(s),
        })
    }
    fn enter(&self, mut state: Signal<ShellState>) -> Option<Entry> {
        let mut s = state.try_write().ok()?;
        if !s.signed_in
            || s.loading
            || s.main_view != MainView::Home
            || s.overlay.is_some()
            || std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some()
            || !self.frame.current(&s)
            || !self.family.current(&s)
            || s.core.active_team.as_ref() != Some(&self.family.session.team_id)
        {
            return None;
        }
        let family = s
            .channel_directory
            .families
            .iter()
            .find(|f| f.session.team_id == self.family.session.team_id && f.current(&s))
            .cloned()
            .unwrap_or_else(|| self.family.clone());
        let mut families = std::mem::take(&mut s.channel_directory.families);
        families.retain(|f| f.current(&s));
        if !families.iter().any(|f| Arc::ptr_eq(f, &family)) {
            // At most one current owner per configured workspace survives.
            families.push(family.clone());
        }
        // Passive intent only: the new visit still needs its own page/read and
        // canonical admission. Consuming the memory cancels the closed window.
        let remembered = family
            .retained
            .try_lock()
            .ok()
            .and_then(|mut value| value.take_at(Instant::now()));
        s.shortcut_generation = s.shortcut_generation.wrapping_add(1);
        s.close_profile();
        s.profile_hover = None;
        let origin = Arc::new(());
        let entry = Entry {
            origin: origin.clone(),
            family,
            frame: Frame::capture(&s),
        };
        s.channel_directory = State {
            origin,
            families,
            entry: Some(entry.clone()),
            ..Default::default()
        };
        if let Some(remembered) = remembered {
            remembered.apply(&mut s.channel_directory);
        }
        s.overlay = Some(Overlay::ChannelDirectory);
        Some(entry)
    }
    pub(crate) fn start(&self, state: Signal<ShellState>) {
        if let Some(entry) = self.enter(state) {
            entry.start(state, read::Mode::Search);
        }
    }
}

#[derive(Clone)]
struct Entry {
    origin: Arc<()>,
    family: Arc<Family>,
    frame: Frame,
}
impl Entry {
    fn current(&self, s: &ShellState) -> bool {
        s.signed_in
            && !s.loading
            && s.overlay == Some(Overlay::ChannelDirectory)
            && std::env::var_os("SUPER_PLATINUM_FIXTURE").is_none()
            && s.core.active_team.as_ref() == Some(&self.family.session.team_id)
            && Arc::ptr_eq(&self.origin, &s.channel_directory.origin)
            && self.family.current(s)
            && self.frame.current(s)
    }
    fn change(&self, mut state: Signal<ShellState>, text: Option<String>, sort: Option<Sort>) {
        let Ok(mut s) = state.try_write() else { return };
        if !self.current(&s) {
            return;
        }
        let d = &mut s.channel_directory;
        d.revision = d.revision.wrapping_add(1);
        d.selected = None;
        d.failed = None;
        d.error = None;
        if let Some(text) = text {
            d.draft = text;
            d.edited = None;
        }
        if let Some(sort) = sort {
            if d.draft.trim().is_empty() {
                d.browse_sort = Some(sort)
            } else {
                d.search_sort = Some(sort)
            }
        }
    }
    fn change_kind(&self, mut state: Signal<ShellState>, kind: Kind) {
        let Ok(mut s) = state.try_write() else { return };
        if !self.current(&s) {
            return;
        }
        let d = &mut s.channel_directory;
        d.revision = d.revision.wrapping_add(1);
        d.kind = kind;
        d.selected = None;
        d.failed = None;
        d.error = None;
    }
    fn reset(&self, mut state: Signal<ShellState>, revision: u64) {
        let Ok(mut s) = state.try_write() else { return };
        if !self.current(&s)
            || s.channel_directory.revision != revision
            || s.channel_directory.busy()
        {
            return;
        }
        // cJeF resets page/query/filters, not the independent sort buckets.
        // A new page1 request retires old page/selection authority as usual.
        let d = &mut s.channel_directory;
        d.revision = d.revision.wrapping_add(1);
        d.draft.clear();
        d.edited = None;
        d.kind = Kind::default();
        d.selected = None;
        d.failed = None;
        d.error = None;
        drop(s);
        self.start(state, read::Mode::Search);
    }
    fn close(&self, mut state: Signal<ShellState>) {
        let Ok(mut s) = state.try_write() else { return };
        // Exact local visit may close even if its credentials have retired.
        if s.overlay == Some(Overlay::ChannelDirectory)
            && Arc::ptr_eq(&self.origin, &s.channel_directory.origin)
        {
            s.channel_directory.revision = s.channel_directory.revision.wrapping_add(1);
            s.channel_directory.selected = None;
            s.dismiss_overlay();
        }
    }
}

struct Loaded {
    entry: Entry,
    query: Query,
    page: Page,
    realtime: u64,
    channels: u64,
}
impl Loaded {
    fn current(&self, s: &ShellState) -> bool {
        let d = &s.channel_directory;
        self.entry.current(s)
            && d.loaded
                .as_ref()
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
            && d.draft == self.query.text
            && d.kind == self.query.kind
            && d.sort() == self.query.sort
            && s.core
                .workspaces
                .get(&self.entry.family.session.team_id)
                .is_some_and(|w| {
                    w.rt_generation == self.realtime
                        && w.channel_lifecycle.revision() == self.channels
                })
    }
    fn select(&self, mut state: Signal<ShellState>, revision: u64, id: &str) {
        let Ok(mut s) = state.try_write() else { return };
        if self.current(&s)
            && !s.channel_directory.busy()
            && s.channel_directory.revision == revision
            && self.page.items.iter().any(|i| i.id == id)
        {
            s.channel_directory.selected = Some(id.into());
            s.channel_directory.selected_origin = Arc::new(());
        }
    }
}

pub(crate) fn fixture(s: &mut ShellState) {
    s.channel_directory = State::default();
    s.main_view = MainView::Home;
    s.overlay = Some(Overlay::ChannelDirectory);
    // Presentation only; no original owner/Entry/Loaded publication is minted.
    s.core.active_account = None;
    s.core.session = None;
    s.core.transport = None;
    let raw = serde_json::json!({"ok":true,"items":[
        {"id":"C1","name":"release-notes","purpose":"Plan releases and share what changed.","member_count":42,"is_member":true},
        {"id":"C2","name":"design-review","purpose":"Discuss interface and accessibility reviews.","member_count":18,"is_member":true}
    ],"pagination":{"page":1,"page_count":2,"total_count":65,"next_cursor":"fixture-next"}});
    s.channel_directory.presentation =
        Some(api::decode(raw, &s.channel_directory.query()).unwrap());
    s.channel_directory.presentation_selected = Some("C1".into());
}

pub(crate) fn schedule_after_outage(
    mut state: Signal<ShellState>,
    visit: crate::bootstrap::reconnect::Visit,
) {
    let entry = {
        let Ok(mut s) = state.try_write() else { return };
        if !visit.current(&s) {
            return;
        }
        let entry = s.channel_directory.entry.clone().filter(|e| e.current(&s));
        if let Some(entry) = &entry {
            entry.recover_stopped_read(&mut s, true);
        }
        if s.channel_directory.failed.is_none() {
            return;
        }
        entry
    };
    if let Some(entry) = entry {
        entry.start_with_visit(state, read::Mode::Retry, Some(visit));
    }
}

#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
