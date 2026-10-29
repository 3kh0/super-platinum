//! Genuine saved Files order. Reads never save; explicit choices own one save.
use super::*;
use crate::app_home::channel_fetches::file_saved_parameters as observation;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport},
};
pub(crate) mod registry;
#[cfg(test)]
pub(crate) mod test_io;
#[cfg(test)]
pub(crate) mod tests;
mod write;

#[derive(Default)]
pub(super) struct State {
    pending: Option<Arc<write::Attempt>>,
    outcome: Option<write::Outcome>,
    rate: Option<Arc<Rate>>,
}
#[derive(Clone)]
struct Entry {
    owner: Identity,
    parameters: observation::Parameters,
    cursor: Cursor,
    observed: Option<(Arc<Frame>, Arc<()>)>,
    channel: String,
    generation: u64,
    thread: u64,
    tab: ChannelTab,
    loading: bool,
    failed: bool,
}
impl Entry {
    fn capture(s: &ShellState, initial: bool) -> Option<Self> {
        if s.overlay != Some(Overlay::Search)
            || s.search_pager.kind != SearchKind::Files
            || s.main_view != MainView::Home
            || s.search_history_open
            || s.search_filters.is_some()
            || s.search_pager.loading != initial
            || s.search_pager.query != s.search_query.trim()
            || (!initial && scene(s).is_some_and(|v| v.book.pending().is_some()))
            || Promoted::cold_and_promote(s.search_query.trim(), api::Sort::Relevant).is_none()
        {
            return None;
        }
        let owner = Identity::capture(s)?;
        let parameters = observation::capture(s, owner.team())?;
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
    fn context_current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
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
    fn current(&self, s: &ShellState) -> bool {
        self.context_current(s) && self.parameters.current(s)
    }
}
/// The private caller validates the old entry before an exact serial LocalChoice
/// edge. Fresh parameters, never value equality, authorize the successor frame.
fn start(
    s: &mut ShellState,
    entry: Entry,
    parameters: observation::Parameters,
    initial: Option<SearchRequest>,
) -> Option<load::Job> {
    if !entry.context_current(s) || !parameters.current(s) {
        return None;
    }
    let sort = parameters.sort();
    let promoted = Arc::new(Promoted::cold_and_promote(s.search_query.trim(), sort)?);
    let request = match initial {
        Some(r) => {
            if !s.search_pager.accepts(&r) {
                return None;
            }
            r
        }
        None => s.search_pager.begin(
            s.core.account_epoch,
            entry.owner.team(),
            &promoted.query,
            false,
        )?,
    };
    s.clear_search_results();
    s.cancel_search_open();
    let frame = Arc::new(Frame {
        owner: Identity::capture(s)?,
        parameters: ReadParameters::Saved(parameters),
        promoted,
        revision: s.search_pager.revision(),
        legacy_sort: s.search_pager.sort,
        channel: entry.channel,
        generation: entry.generation,
        thread: entry.thread,
        tab: entry.tab,
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
            saved: State::default(),
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
fn begin(
    s: &mut ShellState,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> Option<load::Job> {
    if request.kind != SearchKind::Files
        || request.page != 1
        || !super::super::current(s, request, transport)
    {
        return None;
    }
    let entry = Entry::capture(s, true)?;
    if !entry.current(s)
        || !entry.owner.matches_session(transport, session)
        || !client.same_configuration(entry.parameters.client())
    {
        return None;
    }
    let parameters = entry.parameters.clone();
    start(s, entry, parameters, Some(request.clone()))
}
/// Actual native serial Files entry; qualified main-window tab mapping, not a
/// claim to reproduce Source's first lazy-tab scheduling or browser metadata.
pub(in crate::bootstrap::search) async fn try_load(
    mut state: Signal<ShellState>,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> bool {
    if has_scope(&state.peek()) {
        return true;
    }
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| begin(&mut s, request, transport, client, session));
    if let Some(job) = job {
        load::run(state, job).await;
        true
    } else {
        false
    }
}
pub(super) fn restart(s: &mut ShellState) -> Option<load::Job> {
    let entry = Entry::capture(s, false)?;
    if !entry.current(s) {
        return None;
    }
    let p = entry.parameters.clone();
    start(s, entry, p, None)
}
#[derive(Clone)]
pub(crate) struct Source {
    entry: Entry,
    frame: Arc<Frame>,
}
impl Source {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let v = scene(s)?;
        if !matches!(v.frame.parameters, ReadParameters::Saved(_))
            || v.book.active().is_none()
            || v.book.pending().is_some()
            || s.search_file_save_registry.busy(s)
        {
            return None;
        }
        Some(Self {
            entry: Entry::capture(s, false)?,
            frame: v.frame.clone(),
        })
    }
}
pub(crate) fn choose(mut state: Signal<ShellState>, source: Source, sort: api::Sort) {
    let choice = state
        .try_write()
        .ok()
        .and_then(|mut s| write::plan_choice(&mut s, source, sort));
    if let Some((read, work)) = choice {
        dioxus::core::spawn_forever(load::run(state, read));
        // The unique reservation already exists. Never expose a cancel handle
        // or stop this driver when the originating view/receipt retires.
        dioxus::core::spawn_forever(write::run(state, work));
    }
}
pub(crate) use write::{RetrySource, retry};
#[derive(Clone, Copy)]
pub(crate) enum Fixture {
    Saving,
    Unconfirmed,
}
pub(crate) fn owns(s: &ShellState) -> bool {
    s.search_file_sort_fixture.is_some()
        || s.search_file_module
            .as_ref()
            .and_then(|m| m.scene.as_ref())
            .is_some_and(|v| matches!(v.frame.parameters, ReadParameters::Saved(_)))
}
pub(crate) fn status(s: &ShellState) -> Option<&'static str> {
    s.search_file_sort_fixture
        .map(|v| match v {
            Fixture::Saving => "Saving file sort preference\u{2026}",
            Fixture::Unconfirmed => "Couldn\u{2019}t confirm save. Your choice is retained.",
        })
        .or_else(|| write::status(s))
}
