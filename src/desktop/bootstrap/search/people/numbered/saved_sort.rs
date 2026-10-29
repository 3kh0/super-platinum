//! Saved-sort native policy: optimistic local producer, independent read, one
//! bounded explicit write. Socket equality is never an operation acknowledgement.
use super::*;
use std::future::Future;
use super_platinum_core::slack::{
    api::people_sort_preference as wire, transport::PeopleSortPreferenceFailure as Failure,
};

#[derive(Clone, Copy)]
pub(crate) enum Fixture {
    Saving,
    Unconfirmed,
}

#[derive(Default)]
pub(super) struct State {
    pending: Option<Arc<Attempt>>,
    failure: Option<SaveFailure>,
}
struct SaveFailure {
    attempt: Arc<Attempt>,
    unconfirmed: bool,
}
struct Attempt {
    owner: Identity,
    parameters: Parameters,
    sort: api::Sort,
    rate: Arc<Rate>,
}
impl Attempt {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.account_current(s)
            && self.parameters.current(s)
            && self.parameters.sort_producer() == people_parameters::PreferenceProducer::LocalChoice
    }
}
fn reserved(s: &ShellState, a: &Arc<Attempt>) -> bool {
    a.current(s)
        && s.search_people_module.as_ref().is_some_and(|m| {
            m.account.current(s) && m.saved.pending.as_ref().is_some_and(|p| Arc::ptr_eq(p, a))
        })
}
#[derive(Clone)]
pub(crate) struct SavedSortSource {
    frame: Arc<Frame>,
    owner: Identity,
}
impl SavedSortSource {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let v = scene(s)?;
        if v.frame.parameters.local_sort_allowed()
            || v.book.active().is_none()
            || v.book.pending().is_some()
            || s.search_pager.loading
            || s.search_people_module.as_ref()?.saved.pending.is_some()
        {
            return None;
        }
        Some(Self {
            frame: v.frame.clone(),
            owner: Identity::capture(s)?,
        })
    }
}
pub(super) struct Work {
    attempt: Arc<Attempt>,
}
pub(super) struct Choice {
    pub(super) read: Option<sort_refresh::RefreshWork>,
    pub(super) write: Work,
}
pub(super) fn plan_choice(
    s: &mut ShellState,
    source: SavedSortSource,
    sort: api::Sort,
) -> Option<Choice> {
    if !source.owner.current(s)
        || owned(s, &source.frame).is_none()
        || SavedSortSource::capture(s).is_none()
    {
        return None;
    }
    let read_source = RefreshSource::capture(s)?;
    let change = source.frame.parameters.choose_sort(s, sort)?;
    let parameters = source.frame.parameters.after_sort_change(s, &change)?;
    let attempt = Arc::new(Attempt {
        owner: source.owner,
        parameters,
        sort,
        rate: source.frame.rate.clone(),
    });
    let m = s.search_people_module.as_mut()?;
    m.saved.failure = None;
    m.saved.pending = Some(attempt.clone());
    let read = after_sort_change(s, read_source, &change);
    Some(Choice {
        read,
        write: Work { attempt },
    })
}
pub(crate) fn choose(mut state: Signal<ShellState>, source: SavedSortSource, sort: api::Sort) {
    let choice = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_choice(&mut s, source, sort));
    if let Some(choice) = choice {
        if let Some(read) = choice.read {
            run_refresh(state, read);
        }
        dioxus::core::spawn_forever(run(state, choice.write));
    }
}
pub(crate) fn status(s: &ShellState) -> Option<&'static str> {
    let m = s.search_people_module.as_ref()?;
    if !m.account.current(s) {
        return None;
    }
    if let Some(a) = m.saved.pending.as_ref().filter(|a| a.current(s)) {
        let _ = a;
        return Some("Saving sort preference\u{2026}");
    }
    let f = m.saved.failure.as_ref().filter(|f| f.attempt.current(s))?;
    Some(if f.unconfirmed {
        "Couldn\u{2019}t confirm save. Your choice is retained."
    } else {
        "Sort preference was not sent. Your choice is retained."
    })
}
#[derive(Clone)]
pub(crate) struct RetrySource {
    attempt: Arc<Attempt>,
    owner: Identity,
}
impl RetrySource {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let m = s.search_people_module.as_ref()?;
        let f = m.saved.failure.as_ref()?;
        if m.saved.pending.is_some() || !f.attempt.current(s) || scene(s).is_none() {
            return None;
        }
        Some(Self {
            attempt: f.attempt.clone(),
            owner: Identity::capture(s)?,
        })
    }
}
pub(super) fn plan_retry(s: &mut ShellState, source: RetrySource) -> Option<Work> {
    if !source.owner.current(s) || !source.attempt.current(s) || scene(s).is_none() {
        return None;
    }
    let m = s.search_people_module.as_mut()?;
    if m.saved.pending.is_some()
        || !m
            .saved
            .failure
            .as_ref()
            .is_some_and(|f| Arc::ptr_eq(&f.attempt, &source.attempt))
    {
        return None;
    }
    let attempt = Arc::new(Attempt {
        owner: source.attempt.owner.clone(),
        parameters: source.attempt.parameters.clone(),
        sort: source.attempt.sort,
        rate: source.attempt.rate.clone(),
    });
    m.saved.failure = None;
    m.saved.pending = Some(attempt.clone());
    Some(Work { attempt })
}
pub(crate) fn retry(mut state: Signal<ShellState>, source: RetrySource) {
    let work = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_retry(&mut s, source));
    if let Some(work) = work {
        dioxus::core::spawn_forever(run(state, work));
    }
}
// Release only this reservation, even when its publication/parameters retired.
// Query/read replacement must never permit overlapping preference writes.
fn release(s: &mut ShellState, a: &Arc<Attempt>) -> bool {
    let Some(m) = s.search_people_module.as_mut() else {
        return false;
    };
    if m.saved.pending.as_ref().is_some_and(|p| Arc::ptr_eq(p, a)) {
        m.saved.pending = None;
        true
    } else {
        false
    }
}
struct Lease {
    state: Signal<ShellState>,
    attempt: Arc<Attempt>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.try_write() {
            let publish = reserved(&s, &self.attempt);
            if release(&mut s, &self.attempt) && publish {
                s.search_people_module.as_mut().unwrap().saved.failure = Some(SaveFailure {
                    attempt: self.attempt.clone(),
                    unconfirmed: true,
                });
            }
        }
    }
}
type Current = Rc<dyn Fn() -> bool>;
type Observe = Rc<dyn Fn(&Result<serde_json::Value, Error>)>;
async fn run(state: Signal<ShellState>, work: Work) {
    let transport = work.attempt.parameters.transport().clone();
    if let Some(read) = run_with(state, work, move |request, current, observe| async move {
        transport
            .execute_people_sort_preference_current(
                &request,
                move || current(),
                move |r| observe(r),
            )
            .await
    })
    .await
    {
        run_refresh(state, read);
    }
}
pub(super) async fn run_with<F, Fut>(
    mut state: Signal<ShellState>,
    work: Work,
    execute: F,
) -> Option<sort_refresh::RefreshWork>
where
    F: FnOnce(wire::Request, Current, Observe) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Failure>>,
{
    let attempt = work.attempt;
    let _lease = Lease {
        state,
        attempt: attempt.clone(),
    };
    if !state
        .try_read()
        .ok()
        .is_some_and(|s| reserved(&s, &attempt))
    {
        return None;
    }
    let scoped = attempt.clone();
    let current: Current =
        Rc::new(move || state.try_read().ok().is_some_and(|s| reserved(&s, &scoped)));
    let rate = attempt.rate.clone();
    let observe: Observe = Rc::new(move |r| {
        if let Ok(s) = state.try_read() {
            rate.observe(&s, r);
        }
    });
    let result = if attempt.rate.cooling() {
        Err(Failure::NotRequested(Error::RateLimited {
            retry_after_secs: None,
        }))
    } else {
        match wire::request(
            attempt.parameters.client(),
            attempt.parameters.session(),
            attempt.sort,
        ) {
            Err(e) => Err(Failure::NotRequested(e)),
            Ok(request) => execute(request, current, observe).await.and_then(|raw| {
                wire::validate_response(&raw).map_err(Failure::Unconfirmed)?;
                Ok(raw)
            }),
        }
    };
    let mut s = state.try_write().ok()?;
    let publish = reserved(&s, &attempt);
    if !release(&mut s, &attempt) || !publish {
        return None;
    }
    match result {
        Ok(_) => {
            let read = RefreshSource::capture(&s);
            let change = attempt.parameters.submitted_echo(&mut s, attempt.sort)?;
            s.search_people_module.as_mut()?.saved.failure = None;
            read.and_then(|read| after_sort_change(&mut s, read, &change))
        }
        Err(error) => {
            s.search_people_module.as_mut()?.saved.failure = Some(SaveFailure {
                attempt,
                unconfirmed: matches!(error, Failure::Unconfirmed(_)),
            });
            None
        }
    }
}
