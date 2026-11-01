use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::watch;
#[derive(Clone)]
pub(super) struct Pending {
    scene: super::super::owner::NativeReadCheck,
    ticket: Arc<()>,
    reconnect: Option<crate::bootstrap::reconnect::Visit>,
    admitted: Arc<AtomicBool>,
    done: watch::Receiver<bool>,
}
impl Pending {
    fn reconnect_current(&self, s: &ShellState) -> bool {
        self.reconnect.as_ref().is_none_or(|v| {
            if self.admitted.load(Ordering::Acquire) {
                v.admitted_current(s)
            } else {
                v.current(s)
            }
        })
    }
}
pub(super) fn coalesced(s: &ShellState) -> Option<watch::Receiver<bool>> {
    let p = s.channel_tabs.threads.pending_scope.as_ref()?;
    if *p.done.borrow()
        || p.done.has_changed().is_err()
        || !(p.scene)(s)
        || !p.reconnect_current(s)
        || !s
            .channel_tabs
            .threads
            .pending
            .as_ref()
            .is_some_and(|t| Arc::ptr_eq(t, &p.ticket))
    {
        return None;
    }
    Some(p.done.clone())
}

struct Lease {
    state: Signal<ShellState>,
    origin: Arc<()>,
    ticket: Arc<()>,
    rate: limits::Ticket,
    scope: Pending,
    done: watch::Sender<bool>,
}
impl Lease {
    fn current(&self, source: &Source, s: &ShellState) -> bool {
        source.visit_current(s) && self.owns(s) && self.scope.reconnect_current(s)
    }
    fn admit(&self, source: &Source, s: &ShellState) -> bool {
        if !self.current(source, s) || !limits::allows(s, &self.rate) {
            return false;
        }
        self.scope.admitted.store(true, Ordering::Release);
        true
    }
    fn owns(&self, s: &ShellState) -> bool {
        Arc::ptr_eq(&self.origin, &s.channel_tabs.threads.origin)
            && s.channel_tabs
                .threads
                .pending
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.ticket))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.done.send_replace(true);
        let mut state = self.state;
        if let Ok(mut s) = state.try_write() {
            if self.owns(&s) {
                s.channel_tabs.threads.pending = None;
                s.channel_tabs.threads.pending_scope = None;
            }
            limits::release(&mut s, &self.rate);
        };
    }
}
pub(super) async fn run(state: Signal<ShellState>, source: Source) {
    run_owned(state, source, None).await;
}
pub(super) async fn run_owned(
    mut state: Signal<ShellState>,
    source: Source,
    reconnect: Option<crate::bootstrap::reconnect::Visit>,
) {
    let owner = limits::Owner::from_gate(&source.route.gate);
    let (read, ticket, rate, scope, done) = {
        let mut s = state.write();
        if !source.current(&s) || reconnect.as_ref().is_some_and(|v| !v.current(&s)) {
            return;
        }
        if limits::status(&s, &owner).cooling() {
            limits::denied(&mut s, &owner);
            return;
        }
        let Some(next) = s.channel_tabs.threads.sequence.checked_add(1) else {
            return;
        };
        let read = match Read::capture(&s.core, &source.route.gate.team, &source.route.gate.channel)
        {
            Ok(r) => r,
            Err(_) => {
                s.channel_tabs.threads.notice =
                    "The conversation changed. Refresh to load its threads.".into();
                return;
            }
        };
        let Some(rate) = limits::reserve(&mut s, owner.clone()) else {
            limits::denied(&mut s, &owner);
            return;
        };
        let Some(scene) = source.read_check() else {
            limits::release(&mut s, &rate);
            return;
        };
        let ticket = Arc::new(());
        let (done, receiver) = watch::channel(false);
        let scope = Pending {
            scene,
            ticket: ticket.clone(),
            reconnect,
            admitted: Arc::new(AtomicBool::new(false)),
            done: receiver,
        };
        let view = &mut s.channel_tabs.threads;
        view.sequence = next;
        view.pending = Some(ticket.clone());
        view.pending_scope = Some(scope.clone());
        view.notice.clear();
        (read, ticket, rate, scope, done)
    };
    let lease = Lease {
        state,
        origin: source.origin.clone(),
        ticket,
        rate,
        scope,
        done,
    };
    let active = || state.try_peek().is_ok_and(|s| lease.current(&source, &s));
    if !active() {
        return;
    }
    let request = match channel_threads::request(
        &source.route.gate.client,
        &source.route.gate.session,
        &source.route.gate.channel,
    ) {
        Ok(r) => r,
        Err(_) => return,
    };
    #[cfg(test)]
    let response = io::execute(
        &request,
        active,
        || state.try_peek().is_ok_and(|s| lease.admit(&source, &s)),
        |r| limits::observe(state, &lease.rate, r),
    )
    .await;
    #[cfg(not(test))]
    let response = execute_native(state, &source, &lease, request).await;
    if !active() {
        return;
    }
    let result = match response {
        Ok(Some(raw)) => Page::decode(&raw, &source.route.gate.channel),
        Ok(None) => {
            limits::denied(&mut state.write(), &owner);
            return;
        }
        Err(e) => Err(e),
    };
    let mut published = false;
    {
        let mut s = state.write();
        if !lease.current(&source, &s) {
            return;
        }
        match result {
            Ok(page) => match read.publish(&mut s.core, page) {
                Ok(accepted) => {
                    let accepted = Arc::new(accepted);
                    let local = previous::Context::seed(&s, &source.route, accepted.clone());
                    let view = &mut s.channel_tabs.threads;
                    view.local = Some(local);
                    view.accepted = Some(accepted);
                    view.notice.clear();
                    published = true;
                }
                Err(_) => {
                    s.channel_tabs.threads.notice =
                        "The conversation changed while loading. Refresh to load its threads."
                            .into()
                }
            },
            Err(error) => {
                let view = &mut s.channel_tabs.threads;
                view.notice = if view.loaded() {
                    "Could not refresh threads. Your loaded messages are still available."
                } else {
                    "Could not load threads. Try again when you reconnect."
                }
                .into();
                s.report_failure(&error, "Threads need a connection", || {
                    "Could not load this channel's threads.".into()
                });
            }
        }
        // Presentation mirrors never clear or replace authoritative peer facts.
        let rate = limits::status(&s, &owner);
        s.channel_tabs.threads.retry_at = rate.until;
        s.channel_tabs.threads.rate_blocked = rate.blocked;
    }
    drop(lease);
    if published {
        crate::bootstrap::persist_workspace(&state, &source.route.gate.team);
    }
}
// Compiled in test builds too. The mandatory scoped executor alone executes there.
async fn execute_native(
    state: Signal<ShellState>,
    source: &Source,
    lease: &Lease,
    request: super_platinum_core::slack::PreparedRequest,
) -> Result<Option<Value>, Error> {
    source
        .route
        .gate
        .transport
        .execute_admitted(
            request,
            || state.try_peek().is_ok_and(|s| lease.current(&source, &s)),
            || state.try_peek().is_ok_and(|s| lease.admit(&source, &s)),
            |result| limits::observe(state, &lease.rate, result),
        )
        .await
}
