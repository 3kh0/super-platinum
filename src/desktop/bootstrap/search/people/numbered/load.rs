use super::*;
use std::future::Future;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryMode {
    Module,
    FirstAll,
}
pub(super) struct Job {
    pub(super) frame: Arc<Frame>,
    pub(super) ticket: Ticket,
    pub(super) initial: Option<SearchRequest>,
    pub(super) entry: EntryMode,
}
pub(super) fn begin(
    mut state: Signal<ShellState>,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> Option<Job> {
    let mut s = state.try_write().ok()?;
    begin_in(&mut s, request, transport, client, session)
}
pub(super) fn begin_in(
    s: &mut ShellState,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> Option<Job> {
    // Empty promoted filter history is an actual owned model here. Rich query
    // operators remain in the bounded adapter until their state is represented.
    if request.query.contains([':', '"']) {
        return None;
    }
    if s.main_view != MainView::Home || !super::super::super::current(&s, request, transport) {
        return None;
    }
    let owner = Identity::capture(&s)?;
    let parameters = people_parameters::capture(&s, &request.team)?;
    // Pair every credential and destination producer in the same serial read.
    // A request containing A's token must never be sent with B's cookie/client.
    if !owner.matches_session(transport, session)
        || !owner.matches_session(parameters.transport(), parameters.session())
        || !client.same_configuration(parameters.client())
        || !client.same_configuration(&s.core.client)
    {
        return None;
    }
    let channel = s.core.active_channel.clone()?;
    let account = Account::capture(&s)?;
    let rate = s
        .search_people_module
        .as_ref()
        .filter(|m| m.account == account)
        .map(|m| m.rate.clone())
        .unwrap_or_else(|| {
            Arc::new(Rate {
                owner: account.clone(),
                limit: Mutex::new(Limit::None),
            })
        });
    let controls = s
        .search_people_module
        .as_ref()
        .filter(|m| m.account == account)
        .map(|m| m.controls.clone())
        .unwrap_or_else(|| Arc::new(Controls::fresh()));
    let frame = Arc::new(Frame {
        owner,
        controls: controls.clone(),
        parameters,
        rate: rate.clone(),
        query: request.query.clone(),
        dialog: s.search_open_generation,
        revision: s.search_pager.revision(),
        sort: s.search_pager.sort,
        channel,
        generation: s.channel_generation,
        tab: s.channel_tab,
    });
    if !frame.current(&s) {
        return None;
    }
    let mut book = Book::default();
    let Plan::Fetch(ticket) = book.select(1) else {
        return None;
    };
    if s.search_people_module
        .as_ref()
        .is_none_or(|m| m.account != account)
    {
        s.search_people_module = Some(Module {
            account,
            controls,
            sessions: Sessions::default(),
            rate,
            scene: None,
            saved: saved_sort::State::default(),
        });
    }
    s.search_people_module.as_mut()?.scene = Some(Scene {
        frame: frame.clone(),
        book,
        metadata: BTreeMap::new(),
    });
    Some(Job {
        frame,
        ticket,
        initial: Some(request.clone()),
        entry: EntryMode::Module,
    })
}
fn current(s: &ShellState, job: &Job) -> bool {
    job.initial
        .as_ref()
        .is_none_or(|r| s.search_pager.accepts(r))
        && owned(s, &job.frame).is_some_and(|v| v.book.accepts(&job.ticket))
}
struct Lease {
    state: Signal<ShellState>,
    frame: Arc<Frame>,
    ticket: Ticket,
    initial: Option<SearchRequest>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.try_write() {
            if let Some(v) = owned_mut(&mut s, &self.frame) {
                if v.book.fail(&self.ticket) {
                    if let Some(request) = &self.initial {
                        s.search_pager.fail(request);
                    }
                }
            }
        }
    }
}
type Current = Rc<dyn Fn() -> bool>;
type Observe = Rc<dyn Fn(&Result<serde_json::Value, Error>)>;
pub(super) async fn run(state: Signal<ShellState>, job: Job) {
    let transport = job.frame.parameters.transport().clone();
    let frame = job.frame.clone();
    let changed = run_with(state, job, move |request, current, observe| async move {
        transport
            .execute_people_page_current(&request, move || current(), move |r| observe(r))
            .await
    })
    .await;
    if changed {
        profiles::hydrate(state, frame).await;
    }
}
pub(super) async fn run_with<F, Fut>(mut state: Signal<ShellState>, job: Job, execute: F) -> bool
where
    F: FnOnce(api::Request, Current, Observe) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let _lease = Lease {
        state,
        frame: job.frame.clone(),
        ticket: job.ticket.clone(),
        initial: job.initial.clone(),
    };
    let entry = {
        let Ok(mut s) = state.try_write() else {
            return false;
        };
        if !current(&s, &job) {
            return false;
        }
        let sessions = &mut s.search_people_module.as_mut().unwrap().sessions;
        match job.entry {
            EntryMode::Module => sessions.enter(now_ms(), uuid::Uuid::new_v4),
            EntryMode::FirstAll => sessions.enter_first_all(now_ms(), uuid::Uuid::new_v4),
        }
    };
    let scope_frame = job.frame.clone();
    let scope_ticket = job.ticket.clone();
    let admitted: Current = Rc::new(move || {
        state
            .try_read()
            .ok()
            .is_some_and(|s| owned(&s, &scope_frame).is_some_and(|v| v.book.accepts(&scope_ticket)))
    });
    let rate = job.frame.rate.clone();
    let observe: Observe = Rc::new(move |result| {
        if let Ok(s) = state.try_read() {
            rate.observe(&s, result);
        }
    });
    let result = if job.frame.rate.cooling() {
        Err(Error::RateLimited {
            retry_after_secs: None,
        })
    } else {
        match job.frame.request(job.ticket.page, &entry) {
            Ok(request) => execute(request, admitted, observe)
                .await
                .and_then(|raw| Page::decode(raw, job.ticket.page)),
            Err(error) => Err(error),
        }
    };
    let Ok(mut s) = state.try_write() else {
        return false;
    };
    if !current(&s, &job) {
        return false;
    }
    match result {
        Ok(page) => {
            if !owned_mut(&mut s, &job.frame)
                .unwrap()
                .book
                .commit(&job.ticket, page)
            {
                return false;
            }
            if let Some(request) = &job.initial {
                if !s.search_pager.complete_lookup(request) {
                    return false;
                }
            }
            true
        }
        Err(error) => {
            let retained = owned(&s, &job.frame).unwrap().book.active().is_some();
            owned_mut(&mut s, &job.frame)
                .unwrap()
                .book
                .fail(&job.ticket);
            if let Some(request) = &job.initial {
                s.search_pager.fail(request);
            }
            s.report_failure(&error, "People search needs a connection", || {
                if retained {
                    "Could not load this page of people. Your previous page is retained.".into()
                } else {
                    "Could not find people. Please try again.".into()
                }
            });
            false
        }
    }
}
