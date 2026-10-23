//! Explicit user retry of the retained plain Share. No composer or modal owner.
use super::*;
use cancel::Action;
use super_platinum_core::workflow_share::{
    self as protocol, Attempt as SendAttempt, Correlation, Outcome,
};

pub(super) struct Resend {
    sequence: usize,
    pub(super) generation: u64,
    request: Value,
    attempts: Vec<SendAttempt>,
    pub(super) raw: Option<Value>,
    pub(super) outcome: &'static str,
    pub(super) finished: bool,
    applied: bool,
}
impl Resend {
    pub(super) fn snapshot(&self) -> Value {
        json!({"sequence":self.sequence,"generation":self.generation,"request":self.request,
            "attempts":self.attempts,"raw":self.raw,"outcome":self.outcome,"finished":self.finished,"applied":self.applied})
    }
}

#[derive(Default)]
pub(super) struct Cooldown {
    until: Option<tokio::time::Instant>,
    overflow: bool,
}
impl Cooldown {
    pub(super) fn observe(&mut self, a: &SendAttempt) {
        self.observe_at(a, tokio::time::Instant::now());
    }
    fn observe_at(&mut self, a: &SendAttempt, now: tokio::time::Instant) {
        let millis = a
            .retry_after_millis
            .or_else(|| (a.outcome == Outcome::RateLimited).then_some(60_000));
        let Some(millis) = millis else { return };
        let Some(until) = now.checked_add(std::time::Duration::from_millis(millis)) else {
            self.overflow = true;
            return;
        };
        self.until = Some(self.until.map_or(until, |old| old.max(until)));
    }
    pub(super) fn blocked(&self) -> bool {
        self.blocked_at(tokio::time::Instant::now())
    }
    fn blocked_at(&self, now: tokio::time::Instant) -> bool {
        self.overflow || self.until.is_some_and(|until| now < until)
    }
}
fn operation(s: &ShellState, id: u64) -> Option<&run::Operation> {
    attempt(s, id)?
        .confirmation_share
        .as_ref()?
        .operation
        .as_ref()
}
fn operation_mut(s: &mut ShellState, id: u64) -> Option<&mut run::Operation> {
    attempt_mut(s, id)?
        .confirmation_share
        .as_mut()?
        .operation
        .as_mut()
}
pub(super) fn eligible(s: &ShellState, action: &Action) -> bool {
    action.current(s)
        && operation(s, action.attempt).is_some_and(|op| protocol::resend::eligible(op.failure()))
}
pub(super) fn blocked(s: &ShellState, action: &Action) -> bool {
    operation(s, action.attempt).is_none_or(|op| op.cooldown.blocked())
}

#[derive(Clone)]
struct Ticket {
    action: Action,
    generation: u64,
}
impl Ticket {
    fn current(&self, s: &ShellState) -> bool {
        let a = &self.action;
        // Navigation does not retire a dispatched message. Account/auth and the
        // exact unresolved original row do; no successor operation can adopt it.
        a.owner.account_current(s)
            && operation(s, a.attempt).is_some_and(|op| {
                op.owns(&a.owner, &a.message)
                    && op
                        .resends
                        .last()
                        .is_some_and(|r| r.generation == self.generation && !r.finished)
            })
            && s.core
                .workspaces
                .get(&a.owner.session.team_id)
                .and_then(|w| w.messages.get(a.message.channel()))
                .is_some_and(|bag| {
                    cancel::pending_original(bag, &a.message, &a.owner.session.user_id)
                })
    }
    fn receipt<'a>(&self, s: &'a mut ShellState) -> Option<&'a mut Resend> {
        operation_mut(s, self.action.attempt)?
            .resends
            .iter_mut()
            .find(|r| r.generation == self.generation)
    }
}

pub(super) fn send(mut state: Signal<ShellState>, action: Action) {
    let prepared = {
        let mut s = state.write();
        if !eligible(&s, &action) || blocked(&s, &action) {
            return;
        }
        let op = operation_mut(&mut s, action.attempt).unwrap();
        let Ok(request) = action.message.resend_request(
            &action.owner.client,
            &action.owner.session,
            op.failure(),
        ) else {
            return;
        };
        let generation = next();
        op.resends.push(Resend {
            sequence: op.resends.len() + 1,
            generation,
            request: request.inspect(),
            attempts: vec![],
            raw: None,
            outcome: "sending",
            finished: false,
            applied: false,
        });
        s.pending_delivery.remove(action.message.local_ts());
        s.refresh_from_core();
        (request, Ticket { action, generation })
    };
    dioxus::core::spawn_forever(async move {
        let (request, ticket) = prepared;
        // A second queued DOM click cannot dispatch after the first synchronously
        // creates its generation. Cancel is unavailable while this row is sending.
        let mut observer_state = state;
        let mut observer = |value: SendAttempt| {
            let mut s = observer_state.write();
            if let Some(op) = operation_mut(&mut s, ticket.action.attempt) {
                if op.generation() == ticket.generation {
                    op.cooldown.observe(&value);
                }
            }
            if let Some(r) = ticket.receipt(&mut s) {
                r.attempts.push(value);
            }
        };
        let result = if !ticket.current(&state.peek()) {
            Err(unavailable())
        } else if crate::fixture::is_fixture() {
            fixture::first_publish::confirmation::share::mutation(state, &request, &mut observer)
                .await
                .unwrap_or_else(|| Err(unavailable()))
        } else {
            ticket
                .action
                .owner
                .transport
                .execute_workflow_share_current(
                    &request,
                    || ticket.current(&state.peek()),
                    &mut observer,
                )
                .await
        };
        let mut s = state.write();
        let raw = result.as_ref().ok().cloned().or_else(|| {
            ticket
                .receipt(&mut s)
                .and_then(|r| r.attempts.last())
                .and_then(|a| a.raw.clone())
        });
        let correlation = raw
            .as_ref()
            .map(|raw| ticket.action.message.classify(raw.clone()).correlation())
            .unwrap_or_else(|| {
                match ticket
                    .receipt(&mut s)
                    .and_then(|r| r.attempts.last())
                    .map(|a| a.outcome)
                {
                    Some(Outcome::Rejected | Outcome::RateLimited) => Correlation::Rejected,
                    _ => Correlation::Unknown,
                }
            });
        let applied = ticket.current(&s);
        if let Some(r) = ticket.receipt(&mut s) {
            r.raw = raw.clone();
            r.outcome = match correlation {
                Correlation::Acknowledged => "acknowledged",
                Correlation::Rejected => "rejected",
                Correlation::Unknown => "unknown",
            };
            r.finished = true;
            r.applied = applied;
        }
        if applied {
            run::apply(
                &mut s,
                &ticket.action.owner.session.team_id,
                &ticket.action.message,
                correlation,
                raw.as_ref(),
            );
        }
        drop(s);
        if applied {
            posting::start(
                state,
                ticket.action.attempt,
                ticket.generation,
                ticket.action.owner.clone(),
                ticket.action.message.channel(),
                raw.as_ref(),
            );
        }
        wake_after_cooldown(state, ticket.action.attempt, ticket.generation);
        if applied && correlation == Correlation::Acknowledged && !crate::fixture::is_fixture() {
            crate::bootstrap::persist_workspace(&state, &ticket.action.owner.session.team_id);
        }
    });
}

pub(super) fn wake_after_cooldown(mut state: Signal<ShellState>, id: u64, generation: u64) {
    let wake = {
        let s = state.peek();
        operation(&s, id)
            .filter(|op| op.generation() == generation && !op.cooldown.overflow)
            .and_then(|op| {
                op.cancellable()
                    .map(|(owner, _, _)| (owner.clone(), op.cooldown.until))
            })
    };
    let Some((owner, Some(until))) = wake else {
        return;
    };
    if until <= tokio::time::Instant::now() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep_until(until).await;
        if owner.account_current(&state.peek())
            && operation(&state.peek(), id).is_some_and(|op| op.generation() == generation)
        {
            // One scoped wake only; expiry never sends or replaces any row.
            let _notify = state.write();
        }
    });
}

#[cfg(test)]
mod tests;
