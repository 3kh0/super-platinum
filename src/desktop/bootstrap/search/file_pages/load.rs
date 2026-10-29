use super::*;
use std::future::Future;
pub(super) struct Job {
    pub(super) frame: Arc<Frame>,
    pub(super) ticket: Ticket,
    pub(super) initial: Option<SearchRequest>,
    pub(super) restored: Option<crate::bootstrap::reconnect::Visit>,
}
fn current(s: &ShellState, job: &Job) -> bool {
    job.restored.as_ref().is_none_or(|v| v.admitted_current(s))
        && job
            .initial
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
                    v.control = Arc::new(());
                }
            }
            // Release only this exact coarse request, even if its parameters
            // retired. It cannot touch a successor's request or private book.
            if let Some(r) = &self.initial {
                s.search_pager.fail(r);
            }
        }
    }
}
type Current = Rc<dyn Fn() -> bool>;
type Observe = Rc<dyn Fn(&Result<serde_json::Value, Error>)>;
pub(super) async fn run(state: Signal<ShellState>, job: Job) {
    #[cfg(test)]
    let io = dioxus::prelude::try_consume_context::<Rc<test_io::Io>>()
        .expect("private Files test must provide scoped IO");
    #[cfg(not(test))]
    let transport = job.frame.parameters.transport().clone();
    run_with(state, job, move |request, current, observe| async move {
        #[cfg(test)]
        {
            if !current() {
                return Err(Error::Api("file_page_retired".into()));
            }
            let result = io.execute(request).await;
            observe(&result);
            result
        }
        #[cfg(not(test))]
        {
            transport
                .execute_file_page_current(&request, move || current(), move |r| observe(r))
                .await
        }
    })
    .await;
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
    let (entry, rate) = {
        let Ok(mut s) = state.try_write() else {
            return false;
        };
        if !current(&s, &job) {
            return false;
        }
        let m = s.search_file_module.as_mut().unwrap();
        (
            m.sessions.enter(now_ms(), uuid::Uuid::new_v4),
            m.rate.clone(),
        )
    };
    let f = job.frame.clone();
    let t = job.ticket.clone();
    let initial = job.initial.clone();
    let restored = job.restored.clone();
    let admitted: Current = Rc::new(move || {
        state.try_read().ok().is_some_and(|s| {
            restored.as_ref().is_none_or(|v| v.admitted_current(&s))
                && initial.as_ref().is_none_or(|r| s.search_pager.accepts(r))
                && owned(&s, &f).is_some_and(|v| v.book.accepts(&t))
        })
    });
    let response_rate = rate.clone();
    let observe: Observe = Rc::new(move |result| {
        if let Ok(s) = state.try_read() {
            response_rate.observe(&s, result);
        }
    });
    let f = &job.frame;
    let result = if rate.cooling() {
        Err(Error::RateLimited {
            retry_after_secs: None,
        })
    } else {
        match api::request(
            f.parameters.client(),
            f.parameters.session(),
            api::Input {
                query: &f.promoted.query,
                page: job.ticket.page,
                query_session: entry.query,
                search_session: entry.search,
                sort: f.promoted.sort,
                exclude_bots: f.parameters.exclude_bots(),
                only_my_channels: f.parameters.only_my_channels(),
                primary_channel: &f.channel,
            },
        ) {
            Ok(request) => execute(request, admitted, observe)
                .await
                .and_then(|v| api::Page::decode(v, job.ticket.page)),
            Err(e) => Err(e),
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
            owned_mut(&mut s, &job.frame).unwrap().control = Arc::new(());
            if let Some(r) = &job.initial {
                if !s.search_pager.complete(r, None) {
                    return false;
                }
            }
            true
        }
        Err(error) => {
            let v = owned_mut(&mut s, &job.frame).unwrap();
            v.book.fail(&job.ticket);
            v.control = Arc::new(());
            if let Some(r) = &job.initial {
                s.search_pager.fail(r);
            }
            s.report_failure(&error, "File search needs a connection", || {
                "Could not load this page of files. Your previous page is retained.".into()
            });
            false
        }
    }
}
