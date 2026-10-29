//! One physical bounded driver, independent of the page read and UI publication.
use super::*;
use std::future::Future;
use super_platinum_core::slack::{
    api::file_sort_preference as wire, transport::FileSortPreferenceFailure as Failure,
};
pub(super) struct Attempt {
    frame: Arc<Frame>,
    parameters: observation::Parameters,
    rate: Arc<Rate>,
    sort: api::Sort,
}
impl Attempt {
    fn current(&self, s: &ShellState) -> bool {
        self.parameters.current(s)
            && self.parameters.local_choice()
            && owned(s, &self.frame).is_some()
    }
}
pub(super) enum Outcome {
    Submitted(Arc<Attempt>),
    Failed {
        attempt: Arc<Attempt>,
        unconfirmed: bool,
    },
}
pub(super) struct Work {
    attempt: Arc<Attempt>,
    reservation: registry::Reservation,
}
fn reserved(s: &ShellState, a: &Arc<Attempt>) -> bool {
    a.current(s)
        && s.search_file_module
            .as_ref()
            .is_some_and(|m| m.saved.pending.as_ref().is_some_and(|p| Arc::ptr_eq(p, a)))
}
pub(super) fn plan_choice(
    s: &mut ShellState,
    source: Source,
    sort: api::Sort,
) -> Option<(load::Job, Work)> {
    if !source.entry.current(s) || owned(s, &source.frame).is_none() || Source::capture(s).is_none()
    {
        return None;
    }
    let reservation = s.search_file_save_registry.claim(s)?;
    let parameters = source.entry.parameters.choose(s, sort)?;
    let read = start(s, source.entry, parameters.clone(), None)?;
    let rate = s
        .search_file_module
        .as_ref()?
        .saved
        .rate
        .clone()
        .filter(|r| r.owner.current(s))
        .unwrap_or_else(|| {
            Arc::new(Rate {
                owner: Account::capture(s).expect("admitted Files account"),
                until: Mutex::new(None),
                forever: Mutex::new(false),
            })
        });
    let attempt = Arc::new(Attempt {
        frame: read.frame.clone(),
        parameters,
        rate: rate.clone(),
        sort,
    });
    let saved = &mut s.search_file_module.as_mut()?.saved;
    saved.rate = Some(rate);
    saved.outcome = None;
    saved.pending = Some(attempt.clone());
    Some((
        read,
        Work {
            attempt,
            reservation,
        },
    ))
}
pub(super) fn status(s: &ShellState) -> Option<&'static str> {
    let m = s.search_file_module.as_ref()?;
    if m.saved.pending.as_ref().is_some_and(|a| a.current(s)) && s.search_file_save_registry.busy(s)
    {
        return Some("Saving file sort preference\u{2026}");
    }
    if s.search_file_save_registry.busy(s) {
        return Some("Previous file sort save is finishing\u{2026}");
    }
    match m.saved.outcome.as_ref()? {
        Outcome::Submitted(a) if a.current(s) => Some("File sort preference submitted."),
        Outcome::Failed {
            attempt,
            unconfirmed,
        } if attempt.current(s) => Some(if *unconfirmed {
            "Couldn\u{2019}t confirm save. Your choice is retained."
        } else {
            "Sort preference was not sent. Your choice is retained."
        }),
        _ => None,
    }
}
#[derive(Clone)]
pub(crate) struct RetrySource {
    attempt: Arc<Attempt>,
    owner: Identity,
    cursor: Cursor,
    control: Arc<()>,
}
impl RetrySource {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        if s.search_file_save_registry.busy(s) {
            return None;
        }
        let m = s.search_file_module.as_ref()?;
        let Outcome::Failed { attempt, .. } = m.saved.outcome.as_ref()? else {
            return None;
        };
        if !attempt.current(s) {
            return None;
        }
        let v = scene(s)?;
        Some(Self {
            attempt: attempt.clone(),
            owner: Identity::capture(s)?,
            cursor: Cursor::capture(s),
            control: v.control.clone(),
        })
    }
}
pub(super) fn plan_retry(s: &mut ShellState, source: RetrySource) -> Option<Work> {
    if !source.owner.current(s)
        || !source.cursor.current(s)
        || !source.attempt.current(s)
        || !scene(s).is_some_and(|v| Arc::ptr_eq(&v.control, &source.control))
    {
        return None;
    }
    let m = s.search_file_module.as_ref()?;
    if !matches!(m.saved.outcome.as_ref(),Some(Outcome::Failed{attempt,..})if Arc::ptr_eq(attempt,&source.attempt))
    {
        return None;
    }
    let reservation = s.search_file_save_registry.claim(s)?;
    let a = source.attempt;
    let attempt = Arc::new(Attempt {
        frame: a.frame.clone(),
        parameters: a.parameters.clone(),
        rate: a.rate.clone(),
        sort: a.sort,
    });
    let saved = &mut s.search_file_module.as_mut()?.saved;
    saved.outcome = None;
    saved.pending = Some(attempt.clone());
    Some(Work {
        attempt,
        reservation,
    })
}
pub(crate) fn retry(mut state: Signal<ShellState>, source: RetrySource) {
    if let Some(work) = state
        .try_write()
        .ok()
        .and_then(|mut s| plan_retry(&mut s, source))
    {
        dioxus::core::spawn_forever(run(state, work));
    }
}
type Current = Rc<dyn Fn() -> bool>;
type Observe = Rc<dyn Fn(&Result<serde_json::Value, Error>)>;
pub(super) async fn run(state: Signal<ShellState>, work: Work) {
    #[cfg(test)]
    let io = dioxus::prelude::try_consume_context::<Rc<test_io::Io>>()
        .expect("saved Files test must provide scoped preference IO");
    #[cfg(not(test))]
    let transport = work.attempt.parameters.transport().clone();
    run_with(state, work, move |request, current, observe| async move {
        #[cfg(test)]
        {
            if !current() {
                return Err(Failure::NotRequested(Error::Api(
                    "file_sort_intent_retired".into(),
                )));
            }
            let result = io.execute(request).await;
            observe(
                &result
                    .as_ref()
                    .map(Clone::clone)
                    .map_err(|e| e.error().clone()),
            );
            result
        }
        #[cfg(not(test))]
        {
            transport
                .execute_file_sort_preference_current(
                    &request,
                    move || current(),
                    move |r| observe(r),
                )
                .await
        }
    })
    .await;
}
pub(super) async fn run_with<F, Fut>(mut state: Signal<ShellState>, work: Work, execute: F)
where
    F: FnOnce(wire::Request, Current, Observe) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Failure>>,
{
    // Work owns this token even before first poll. No retirement select/cancel
    // can interrupt a dispatched driver. Release never borrows ShellState.
    let Work {
        attempt,
        reservation,
    } = work;
    let _reservation = reservation;
    let admitted = state
        .try_read()
        .ok()
        .is_some_and(|s| reserved(&s, &attempt));
    let a = attempt.clone();
    let current: Current = Rc::new(move || state.try_read().ok().is_some_and(|s| reserved(&s, &a)));
    let rate = attempt.rate.clone();
    let observe: Observe = Rc::new(move |r| {
        if let Ok(s) = state.try_read() {
            rate.observe(&s, r);
        }
    });
    let result = if !admitted || !_reservation.held() {
        Err(Failure::NotRequested(Error::Api(
            "file_sort_intent_retired".into(),
        )))
    } else if attempt.rate.cooling() {
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
                wire::validate_response(&raw)
                    .map(|()| raw)
                    .map_err(Failure::Unconfirmed)
            }),
        }
    };
    if let Ok(mut s) = state.try_write() {
        let publish = reserved(&s, &attempt);
        if let Some(m) = s.search_file_module.as_mut() {
            if m.saved
                .pending
                .as_ref()
                .is_some_and(|a| Arc::ptr_eq(a, &attempt))
            {
                m.saved.pending = None;
                if publish {
                    m.saved.outcome = Some(match result {
                        Ok(_) => Outcome::Submitted(attempt),
                        Err(e) => Outcome::Failed {
                            attempt,
                            unconfirmed: matches!(e, Failure::Unconfirmed(_)),
                        },
                    });
                }
            }
        }
    }
}
