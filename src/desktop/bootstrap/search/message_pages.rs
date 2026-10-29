//! One native Messages search book, with dialog-owned controls beside its cache.
//! Wire/session IDs and within-page projection retain the existing native policy.
use super::*;
use crate::state::Overlay;
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{search::SearchKind, slack::models::SearchPagination};
mod read;
use read::run;
pub(super) use read::{Lease, Read, initial};

pub(crate) struct Book {
    frame: Arc<read::Frame>,
    activation: Arc<()>,
    pages: BTreeMap<u32, Vec<SearchResultVm>>,
    bound: u32,
    total: u64,
    shown: u32,
    target: u32,
    revision: u64,
    pending: Option<SearchRequest>,
    failed: bool,
}
impl Book {
    fn current(&self, shell: &ShellState) -> bool {
        let pager = &shell.search_pager;
        self.frame.current(shell)
            && pager.revision() == self.revision
            && pager.page == self.shown
            && pager.page_count == self.bound
            && pager.total == self.total
            && pager.loading == self.pending.is_some()
            && pager.failed == self.failed
    }
    fn metadata(&self, page: u32) -> SearchPagination {
        SearchPagination {
            page: Some(page),
            page_count: Some(self.bound),
            total_count: Some(self.total),
            ..Default::default()
        }
    }
}

/// Presence owns this branch even when its credentials/control frame retired.
/// A legacy callback must not turn a retired book into an append search.
pub(crate) fn has_scope(shell: &ShellState) -> bool {
    shell.search_pager.kind == SearchKind::Messages && shell.search_message_pages.is_some()
}
pub(crate) fn active(shell: &ShellState) -> bool {
    has_scope(shell)
        && shell
            .search_message_pages
            .as_ref()
            .is_some_and(|book| book.current(shell))
}
pub(crate) fn rows(shell: &ShellState) -> &[SearchResultVm] {
    shell
        .search_message_pages
        .as_ref()
        .filter(|book| book.current(shell))
        .and_then(|book| book.pages.get(&book.shown))
        .map(Vec::as_slice)
        .unwrap_or_default()
}
pub(crate) fn status(shell: &ShellState) -> Option<(u32, u32, u32, u64, bool, bool)> {
    let book = shell
        .search_message_pages
        .as_ref()
        .filter(|book| book.current(shell))?;
    Some((
        book.shown,
        book.target,
        book.bound,
        book.total,
        book.pending.is_some(),
        book.failed,
    ))
}

#[derive(Clone)]
pub(crate) struct Control {
    frame: Arc<read::Frame>,
    activation: Arc<()>,
    cursor: recovery::Cursor,
    dialog: u64,
    shown: u32,
    target: u32,
    loading: bool,
    failed: bool,
}
impl Control {
    pub(crate) fn capture(shell: &ShellState) -> Option<Self> {
        let book = shell
            .search_message_pages
            .as_ref()
            .filter(|book| book.current(shell))?;
        if shell.overlay != Some(Overlay::Search)
            || shell.search_history_open
            || shell.search_filters.is_some()
        {
            return None;
        }
        Some(Self {
            frame: book.frame.clone(),
            activation: book.activation.clone(),
            cursor: recovery::Cursor::capture(shell),
            dialog: shell.search_open_generation,
            shown: book.shown,
            target: book.target,
            loading: book.pending.is_some(),
            failed: book.failed,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.overlay == Some(Overlay::Search)
            && !shell.search_history_open
            && shell.search_filters.is_none()
            && shell.search_open_generation == self.dialog
            && self.cursor.current(shell)
            && shell.search_message_pages.as_ref().is_some_and(|book| {
                book.current(shell)
                    && Arc::ptr_eq(&book.frame, &self.frame)
                    && Arc::ptr_eq(&book.activation, &self.activation)
                    && book.shown == self.shown
                    && book.target == self.target
                    && book.pending.is_some() == self.loading
                    && book.failed == self.failed
            })
    }
}

enum Plan {
    Cached,
    Fetch(read::Work),
}
fn plan(
    shell: &mut ShellState,
    source: &Control,
    page: u32,
    retry: bool,
    restored: Option<super::super::reconnect::Visit>,
) -> Option<Plan> {
    plan_read(shell, source, page, retry, restored, false)
}
fn plan_read(
    shell: &mut ShellState,
    source: &Control,
    page: u32,
    retry: bool,
    restored: Option<super::super::reconnect::Visit>,
    refresh: bool,
) -> Option<Plan> {
    if !source.current(shell) || source.loading {
        return None;
    }
    if restored.as_ref().is_some_and(|visit| !visit.current(shell)) {
        return None;
    }
    let book = shell.search_message_pages.as_ref()?;
    if page == 0
        || page > book.bound
        || (retry && (!book.failed || page != book.target))
        || (!refresh && !retry && page == book.shown && !book.failed)
        || (refresh && (retry || page != book.shown || book.failed))
    {
        return None;
    }
    let cached = (!retry && !refresh)
        .then(|| book.pages.get(&page).cloned())
        .flatten();
    let frame = book.frame.clone();
    let request =
        shell
            .search_pager
            .begin_message_page(frame.epoch, &frame.team, &frame.raw, page)?;
    shell.cancel_search_open();
    let revision = shell.search_pager.revision();
    let activation = Arc::new(());
    let book = shell.search_message_pages.as_mut()?;
    book.activation = activation.clone();
    book.target = page;
    book.revision = revision;
    book.pending = Some(request.clone());
    book.failed = false;
    let ticket = read::Ticket {
        frame,
        activation,
        request,
        cursor: recovery::Cursor::capture(shell),
        restored: restored.map(|visit| (visit, shell.search_open_generation)),
    };
    if let Some(rows) = cached {
        let metadata = shell.search_message_pages.as_ref()?.metadata(page);
        if !shell
            .search_pager
            .complete(&ticket.request, Some(&metadata))
        {
            Read::Page(ticket).retire(shell);
            return None;
        }
        Read::Page(ticket).publish(shell, rows, Some(&metadata));
        Some(Plan::Cached)
    } else {
        Some(Plan::Fetch(read::Work { ticket }))
    }
}

/// An unchanged filter Apply re-reads only the admitted current page, retaining
/// the Book and other cached pages. A retained but retired Book stays handled:
/// it never becomes an initial legacy search through this local intent.
pub(crate) async fn refresh_current(mut state: Signal<ShellState>) -> bool {
    if !has_scope(&state.peek()) {
        return false;
    }
    let planned = state.try_write().ok().and_then(|mut shell| {
        let source = Control::capture(&shell)?;
        if source.failed {
            plan(&mut shell, &source, source.target, true, None)
        } else {
            plan_read(&mut shell, &source, source.shown, false, None, true)
        }
    });
    if let Some(Plan::Fetch(work)) = planned {
        run(state, work).await;
    }
    true
}

pub(crate) fn choose(mut state: Signal<ShellState>, source: Control, page: u32) {
    let planned = state
        .try_write()
        .ok()
        .and_then(|mut shell| plan(&mut shell, &source, page, false, None));
    if let Some(Plan::Fetch(work)) = planned {
        spawn(async move {
            run(state, work).await;
        });
    }
}
pub(crate) fn retry(mut state: Signal<ShellState>, source: Control) {
    let planned = state
        .try_write()
        .ok()
        .and_then(|mut shell| plan(&mut shell, &source, source.target, true, None));
    if let Some(Plan::Fetch(work)) = planned {
        spawn(async move {
            run(state, work).await;
        });
    }
}
pub(super) async fn more(mut state: Signal<ShellState>) {
    let planned = state.try_write().ok().and_then(|mut shell| {
        let source = Control::capture(&shell)?;
        let page = if source.failed {
            source.target
        } else {
            source.shown.checked_add(1)?
        };
        plan(&mut shell, &source, page, source.failed, None)
    });
    if let Some(Plan::Fetch(work)) = planned {
        run(state, work).await;
    }
}

pub(in crate::bootstrap) fn schedule_after_outage(
    mut state: Signal<ShellState>,
    visit: super::super::reconnect::Visit,
) {
    let candidate = state
        .try_read()
        .ok()
        .and_then(|shell| Control::capture(&shell))
        .filter(|source| source.failed && !source.loading);
    let Some(source) = candidate else {
        return;
    };
    spawn(async move {
        let planned = state
            .try_write()
            .ok()
            .and_then(|mut shell| plan(&mut shell, &source, source.target, true, Some(visit)));
        if let Some(Plan::Fetch(work)) = planned {
            run(state, work).await;
        }
    });
}

/// A retained row occurrence, never an arbitrary ID borrowed from a new page.
#[derive(Clone)]
pub(crate) struct RowSource {
    control: Control,
    index: usize,
    channel_generation: u64,
    thread_generation: u64,
    view: crate::state::MainView,
}
impl RowSource {
    pub(crate) fn capture(shell: &ShellState, index: usize) -> Option<Self> {
        let control = Control::capture(shell)?;
        if control.loading {
            return None;
        }
        rows(shell).get(index)?;
        Some(Self {
            control,
            index,
            channel_generation: shell.channel_generation,
            thread_generation: shell.thread_generation,
            view: shell.main_view,
        })
    }
    pub(in crate::bootstrap) fn resolve(&self, shell: &ShellState) -> Option<SearchResultVm> {
        if !self.control.current(shell)
            || self.control.loading
            || shell.channel_generation != self.channel_generation
            || shell.thread_generation != self.thread_generation
            || shell.main_view != self.view
        {
            return None;
        }
        rows(shell).get(self.index).cloned()
    }
    pub(in crate::bootstrap) fn proof(&self) -> OpenProof {
        OpenProof {
            frame: self.control.frame.clone(),
            activation: self.control.activation.clone(),
            cursor: self.control.cursor.clone(),
            index: self.index,
            expected_dialog: self.control.dialog.wrapping_add(1),
        }
    }
}
#[derive(Clone)]
pub(crate) struct OpenProof {
    frame: Arc<read::Frame>,
    activation: Arc<()>,
    cursor: recovery::Cursor,
    index: usize,
    expected_dialog: u64,
}
impl OpenProof {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        matches!(shell.overlay, None | Some(Overlay::Search))
            && shell.search_open_generation == self.expected_dialog
            && self.cursor.current(shell)
            && shell.search_message_pages.as_ref().is_some_and(|book| {
                book.current(shell)
                    && Arc::ptr_eq(&book.frame, &self.frame)
                    && Arc::ptr_eq(&book.activation, &self.activation)
                    && book.pending.is_none()
                    && book
                        .pages
                        .get(&book.shown)
                        .is_some_and(|rows| rows.get(self.index).is_some())
            })
    }
}
pub(crate) fn open(state: Signal<ShellState>, source: RowSource) {
    super::super::search_navigation::open_message_page_hit(state, source);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fixture {
    First,
    Second,
    Failed,
}
pub(crate) fn fixture(shell: &mut ShellState, name: &str) {
    super::search_fixture(shell, "search");
    shell.search_message_pages_fixture = Some(if name.contains("failed") {
        Fixture::Failed
    } else if name.contains("page2") {
        Fixture::Second
    } else {
        Fixture::First
    });
    shell.search_pager.page = if name.contains("page2") { 2 } else { 1 };
    shell.search_pager.page_count = 9;
    shell.search_pager.total = 172;
    shell.search_pager.failed = name.contains("failed");
}

#[cfg(test)]
pub(crate) mod test_io;
#[cfg(test)]
pub(crate) mod tests;
