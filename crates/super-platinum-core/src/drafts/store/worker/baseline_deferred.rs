//! Explicit same-ticket claim after cooldown. Never a new read reservation.
use super::*;
use crate::drafts::store::worker::owned_queue::operations::BaselineState;
use p::commands::{Action, Command};

impl Completion {
    pub(in crate::drafts::store::worker) fn baseline_state(&self) -> Result<BaselineState, Reason> {
        let s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if !s.baseline {
            return Err(Reason::Stale);
        }
        if s.raw.is_some() {
            return Ok(BaselineState::ResponseRetained);
        }
        if s.handed {
            return Ok(BaselineState::Requested);
        }
        if s.deferred.disconnected {
            return Ok(BaselineState::Disconnected);
        }
        if let Some(Err(error)) = s.deferred.result {
            return Ok(BaselineState::Failed(error));
        }
        match s.binding.as_deref() {
            Some(Binding::Baseline(_)) => Ok(BaselineState::Ready),
            Some(Binding::DeferredBaseline(_, until)) => Ok(BaselineState::Deferred(*until)),
            None => Ok(BaselineState::ClaimPending {
                now_ms: s.deferred.now_ms.ok_or(Reason::Incomplete)?,
                submitted: s.deferred.submitted.is_some(),
            }),
            _ => Err(Reason::Stale),
        }
    }
}

#[derive(Default)]
pub(super) struct Deferred {
    request: Option<Request>,
    receiver: Option<mpsc::Receiver<Result<p::BaselineClaim, p::Error>>>,
    submitted: Option<u64>,
    now_ms: Option<u64>,
    result: Option<Result<Option<u64>, p::Error>>,
    disconnected: bool,
    pub(super) history: Vec<(u64, Result<Option<u64>, p::Error>)>,
}
impl Deferred {
    pub(super) fn settled(&self) -> bool {
        self.request.is_none()
            && self.receiver.is_none()
            && !self.disconnected
            && (self.submitted.is_none() || self.result.as_ref().is_some_and(Result::is_ok))
            && self.history.iter().all(|(_, result)| result.is_ok())
    }
}
impl Worker {
    pub(in crate::drafts::store::worker) fn resume_baseline_deferred(
        &self,
        c: &Completion,
        now_ms: u64,
    ) -> Result<(), Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let entry = self.exact_completion(&pending, c)?;
        let a = pending.admission.as_ref().unwrap();
        a.check(a.identity.owner.account(), false)?;
        if entry.generation != a.generation {
            return Err(Reason::Stale);
        }
        let mut s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
        if !s.baseline || s.handed || s.raw.is_some() || s.deferred.disconnected {
            return Err(Reason::Stale);
        }
        if s.deferred.submitted.is_some() && s.deferred.result.is_none() {
            return if s.deferred.now_ms == Some(now_ms) {
                Ok(())
            } else {
                Err(Reason::Stale)
            };
        }
        if s.deferred.request.is_none() {
            let until = match s.binding.as_deref() {
                Some(Binding::DeferredBaseline(_, until)) => *until,
                _ => return Err(Reason::Stale),
            };
            if now_ms < until {
                return Err(Reason::Incomplete);
            }
            let binding = s.binding.take().unwrap();
            let binding = match Arc::try_unwrap(binding) {
                Ok(v) => v,
                Err(v) => {
                    s.binding = Some(v);
                    return Err(Reason::Incomplete);
                }
            };
            let Binding::DeferredBaseline(ticket, _) = binding else {
                unreachable!()
            };
            let (reply, receiver) = mpsc::channel();
            s.deferred.request = Some(Request::Protected(Box::new(Command {
                account: c.key.route.owner.account().into(),
                action: Action::ClaimBaseline {
                    ticket,
                    now_ms,
                    reply,
                },
            })));
            s.deferred.receiver = Some(receiver);
            s.deferred.submitted = None;
            s.deferred.result = None;
            s.deferred.now_ms = Some(now_ms);
        } else if s.deferred.now_ms != Some(now_ms) {
            return Err(Reason::Stale);
        }
        let request = s.deferred.request.take().ok_or(Reason::Incomplete)?;
        match Self::try_submit_locked(&mut pending, &c.key, request) {
            Ok(id) => {
                s.deferred.submitted = Some(id);
                drop(s);
                drop(pending);
                self.ready.notify_one();
                Ok(())
            }
            Err(e) => {
                s.deferred.request = Some(e.input);
                Err(e.reason)
            }
        }
    }
    pub(in crate::drafts::store::worker) fn wait_baseline_deferred(
        &self,
        c: &Completion,
    ) -> Result<Result<Option<u64>, p::Error>, Reason> {
        {
            let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
            self.exact_completion(&pending, c)?;
        }
        let mut s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
        let (id, result) = loop {
            let id = s.deferred.submitted.ok_or(Reason::Incomplete)?;
            if let Some(result) = &s.deferred.result {
                break (id, *result);
            }
            if s.deferred.disconnected {
                return Err(Reason::Incomplete);
            }
            if let Some(receiver) = s.deferred.receiver.take() {
                drop(s);
                let reply = receiver.recv();
                s = c.held.status.lock().map_err(|_| Reason::Incomplete)?;
                match reply {
                    Ok(claim) => {
                        let result = match claim {
                            Ok(p::BaselineClaim::Ready {
                                request,
                                completion,
                            }) => {
                                s.binding = Some(Arc::new(Binding::Baseline(completion)));
                                s.dispatch = Some(Dispatch::Baseline(request));
                                Ok(None)
                            }
                            Ok(p::BaselineClaim::Deferred { ticket, until }) => {
                                s.binding =
                                    Some(Arc::new(Binding::DeferredBaseline(ticket, until)));
                                Ok(Some(until))
                            }
                            Err(e) => Err(e),
                        };
                        s.deferred.history.push((id, result));
                        s.deferred.result = Some(result);
                        c.held.changed.notify_all();
                        // Return this exact attempt even if another caller later
                        // explicitly resumes a newly deferred ticket.
                        break (id, result);
                    }
                    Err(_) => {
                        s.deferred.disconnected = true;
                        c.held.changed.notify_all();
                    }
                }
            } else {
                s = c.held.changed.wait(s).map_err(|_| Reason::Incomplete)?;
            }
        };
        drop(s);
        self.wait_accounted(&c.key, id)?;
        Ok(result)
    }
}
