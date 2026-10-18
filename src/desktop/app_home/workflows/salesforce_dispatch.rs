//! Actual Salesforce transport custody. Controller tickets are local to one
//! retained connection instance, never sufficient dispatch authority on their own.
use super::*;
use salesforce::{Connection, Effect, Ticket};
use salesforce_observation::Observation;
use super_platinum_core::block_actions::home::workflow::salesforce::{Action, Target};
#[cfg(test)]
mod tests;

pub(crate) struct Session {
    pub(super) id: u64,
    pub(super) dialog: u64,
    pub(super) observation: Observation,
    pub(super) connection: Connection,
    origin: Origin,
    pending: Option<(Ticket, Action)>,
    queued: Option<(Ticket, Action)>,
    sent: bool,
    cancelled: bool,
    lease: Option<tokio::sync::oneshot::Sender<()>>,
    pub(super) browser: Option<Ticket>,
    // Exact unresolved local attempts survive dismissal. A later list response
    // does not silently erase whether these writes reached the service.
    unresolved: Vec<(Ticket, Action)>,
}
impl Session {
    fn owner_current(&self, s: &ShellState) -> bool {
        self.origin.current(s) && self.observation.current(s) && dialog_current(s, self.dialog)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        !self.cancelled
            && self.owner_current(s)
            && s.app_home.workflow.as_ref().is_some_and(|d| {
                d.salesforce_view
                    .as_ref()
                    .is_some_and(|(function, provider)| {
                        function == self.connection.target().function()
                            && provider == self.connection.target().provider()
                    })
            })
    }
    fn keep(&self) -> bool {
        self.pending.is_some() || !self.unresolved.is_empty() || !self.cancelled
    }
    fn release(&mut self) {
        if let Some(lease) = self.lease.take() {
            let _ = lease.send(());
        }
    }
}

pub(super) fn active(s: &ShellState, dialog: u64) -> bool {
    s.app_home
        .workflow_salesforce
        .iter()
        .any(|v| v.dialog == dialog && v.current(s) && (v.pending.is_some() || v.connection.busy()))
}
pub(super) fn unresolved(s: &ShellState, origin: &Origin) -> bool {
    s.app_home.workflow_salesforce.iter().any(|v| {
        v.origin.owner.ctx.epoch == origin.owner.ctx.epoch
            && v.origin.owner.ctx.session.team_id == origin.owner.ctx.session.team_id
            && v.origin.workflow.same_action(&origin.workflow)
            && (!v.unresolved.is_empty() || v.pending.is_some())
    })
}
pub(super) fn cancel(s: &mut ShellState, dialog: Option<u64>) {
    for v in &mut s.app_home.workflow_salesforce {
        if dialog.is_none_or(|id| v.dialog == id) {
            v.cancelled = true;
            v.browser = None;
            if let Some((ticket, _)) = v.queued.take() {
                v.connection.not_sent(ticket);
            }
            v.release();
            // Keep the controller's outstanding request until its actual result
            // settles. Cancelling the UI cannot turn a sent call into an unsent one.
            if v.pending.is_none() {
                v.connection.cancel();
            }
        }
    }
    s.app_home.workflow_salesforce.retain(Session::keep);
}

/// Called after actual assignment acquisition. Exact same field rejoins its
/// retained controller; remounting cannot silently repeat the initial auth call.
pub(super) fn attach(
    s: &mut ShellState,
    dialog: u64,
    observation: Observation,
    target: Target,
) -> Option<(u64, Option<Effect>)> {
    if !dialog_current(s, dialog) || !observation.current(s) {
        return None;
    }
    if let Some(index) = s.app_home.workflow_salesforce.iter().position(|v| {
        v.dialog == dialog
            && v.connection.target().function() == target.function()
            && v.connection.target().provider() == target.provider()
    }) {
        let existing = &s.app_home.workflow_salesforce[index];
        if !existing.origin.current(s)
            || !existing.observation.same(&observation)
            || !existing.connection.target().same_connection(&target)
        {
            return None;
        }
        if existing.cancelled {
            if existing.pending.is_some() || existing.unresolved.is_empty() {
                return None;
            }
            let existing = &mut s.app_home.workflow_salesforce[index];
            let effect = existing.connection.review_after_cancel()?;
            existing.cancelled = false;
            return Some((existing.id, Some(effect)));
        }
        return Some((existing.id, None));
    }
    let origin = s.app_home.workflow.as_ref()?.origin.clone();
    if s.app_home.workflow_salesforce.len() >= 64 || unresolved(s, &origin) {
        return None;
    }
    let mut connection = Connection::new(target);
    let effect = connection.begin();
    let id = next();
    s.app_home.workflow_salesforce.push(Session {
        id,
        dialog,
        observation,
        connection,
        origin,
        pending: None,
        queued: None,
        sent: false,
        cancelled: false,
        lease: None,
        browser: None,
        unresolved: Vec::new(),
    });
    Some((id, effect))
}

/// Every Effect is checked against an instance and its private controller ticket.
/// spawn_forever retains settlement when the modal component unmounts.
pub(super) fn dispatch(mut state: Signal<ShellState>, instance: u64, effect: Effect) {
    let Effect::Request { ticket, action } = effect else {
        return;
    };
    {
        let mut s = state.write();
        let Some(v) = s
            .app_home
            .workflow_salesforce
            .iter()
            .find(|v| v.id == instance)
        else {
            return;
        };
        if !v.current(&s)
            || v.pending.is_some()
            || !v.connection.expects(ticket, action)
            || (v.unresolved.len() >= 32 && !matches!(action, Action::List(_)))
        {
            return;
        }
        if cooling(&s, &v.origin.owner.ctx) {
            let v = s
                .app_home
                .workflow_salesforce
                .iter_mut()
                .find(|v| v.id == instance)
                .unwrap();
            v.queued = None;
            v.connection.not_sent(ticket);
            drop(s);
            run_next(state);
            return;
        }
        if s.app_home
            .workflow_salesforce
            .iter()
            .any(|v| v.pending.is_some())
        {
            let v = s
                .app_home
                .workflow_salesforce
                .iter_mut()
                .find(|v| v.id == instance)
                .unwrap();
            if v.queued.is_none() {
                v.queued = Some((ticket, action));
            }
            return;
        }
        let v = s
            .app_home
            .workflow_salesforce
            .iter_mut()
            .find(|v| v.id == instance)
            .unwrap();
        v.pending = Some((ticket, action));
        v.queued = None;
        v.sent = false;
    }
    dioxus::core::spawn_forever(async move {
        let owner = {
            let s = state.peek();
            s.app_home
                .workflow_salesforce
                .iter()
                .find(|v| v.id == instance)
                .map(|v| (v.dialog, v.origin.clone(), v.observation.clone()))
        };
        let Some((dialog, origin, old_observation)) = owner else {
            return;
        };
        // Reacquire assignments at each action boundary; an old observation is
        // not a certificate that the feature/environment has stayed unchanged.
        let fresh = salesforce_observation::read(state, dialog).await;
        if !fresh.as_ref().is_ok_and(|v| old_observation.same(v)) {
            before_send(&mut state.write(), instance, ticket);
            run_next(state);
            return;
        }
        let Some(lease) = inputs::native::lock(state, origin.key).await else {
            before_send(&mut state.write(), instance, ticket);
            run_next(state);
            return;
        };
        let request = (|| {
            let mut s = state.write();
            let Some(v) = s
                .app_home
                .workflow_salesforce
                .iter()
                .find(|v| v.id == instance)
            else {
                let _ = lease.send(());
                return None;
            };
            if !v.current(&s)
                || v.pending != Some((ticket, action))
                || v.sent
                || cooling(&s, &origin.owner.ctx)
            {
                let _ = lease.send(());
                before_send(&mut s, instance, ticket);
                return None;
            }
            let current = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
            let request = v.connection.target().request(
                action,
                &origin.owner.ctx.client,
                &origin.owner.ctx.session,
                current,
            );
            let Ok(request) = request else {
                let _ = lease.send(());
                before_send(&mut s, instance, ticket);
                return None;
            };
            let v = s
                .app_home
                .workflow_salesforce
                .iter_mut()
                .find(|v| v.id == instance)
                .unwrap();
            v.sent = true;
            let (release, complete) = native_lease::retain(lease);
            v.lease = Some(release);
            Some((request, complete))
        })();
        let Some((request, release_complete)) = request else {
            // Every refusal above sent the original Eval release directly.
            if lease.join::<Value>().await.is_ok() && origin.current(&state.peek()) {
                run_next(state);
            }
            return;
        };
        let result = io::once(state, &origin, request).await;
        // Keep pending/sent charged until the original JS lock is released.
        // Other requests remain queued while this task awaits completion.
        if let Some(v) = state
            .write()
            .app_home
            .workflow_salesforce
            .iter_mut()
            .find(|v| v.id == instance)
        {
            v.release();
        }
        let released_current = release_complete
            .current_after(|| {
                let s = state.peek();
                origin.current(&s)
                    && s.app_home
                        .workflow_salesforce
                        .iter()
                        .any(|v| v.id == instance && v.current(&s))
            })
            .await;
        // The real response still settles even if release failed or owner retired.
        let next_effect = settle(&mut state.write(), instance, ticket, action, result);
        if !released_current {
            let mut s = state.write();
            if let Some(v) = s
                .app_home
                .workflow_salesforce
                .iter_mut()
                .find(|v| v.id == instance)
            {
                match next_effect {
                    Some(Effect::Request { ticket, .. }) => {
                        v.connection.not_sent(ticket);
                    }
                    Some(Effect::OpenExternal { ticket, .. }) => {
                        v.connection.browser_failed(ticket);
                    }
                    _ => {}
                }
            }
            return;
        }
        match next_effect {
            Some(Effect::Request { ticket, action }) => {
                dispatch(state, instance, Effect::Request { ticket, action })
            }
            Some(Effect::OpenExternal { ticket, url }) => {
                let Some(lease) = inputs::native::lock(state, origin.key).await else {
                    let mut s = state.write();
                    if let Some(v) = s
                        .app_home
                        .workflow_salesforce
                        .iter_mut()
                        .find(|v| v.id == instance)
                    {
                        v.connection.browser_failed(ticket);
                    }
                    drop(s);
                    run_next(state);
                    return;
                };
                let open = {
                    let mut s = state.write();
                    let current = s
                        .app_home
                        .workflow_salesforce
                        .iter()
                        .find(|v| v.id == instance)
                        .is_some_and(|v| v.current(&s));
                    if current {
                        s.app_home
                            .workflow_salesforce
                            .iter_mut()
                            .find(|v| v.id == instance)
                            .unwrap()
                            .browser = Some(ticket);
                    }
                    current
                };
                if open {
                    crate::slack_links::open(state, &url);
                }
                let _ = lease.send(());
                if lease.join::<Value>().await.is_err() {
                    return;
                }
                if !origin.current(&state.peek()) {
                    return;
                }
            }
            _ => {}
        }
        // Also covers Effect::Request and the no-effect queue continuation.
        if origin.current(&state.peek()) {
            run_next(state);
        }
    });
}

fn run_next(state: Signal<ShellState>) {
    let next = {
        let s = state.peek();
        if s.app_home
            .workflow_salesforce
            .iter()
            .any(|v| v.pending.is_some())
        {
            return;
        }
        s.app_home
            .workflow_salesforce
            .iter()
            .filter(|v| v.current(&s))
            .find_map(|v| {
                v.queued
                    .map(|(ticket, action)| (v.id, Effect::Request { ticket, action }))
            })
    };
    if let Some((id, effect)) = next {
        dispatch(state, id, effect);
    }
}

fn before_send(s: &mut ShellState, instance: u64, ticket: Ticket) {
    let Some(v) = s
        .app_home
        .workflow_salesforce
        .iter_mut()
        .find(|v| v.id == instance)
    else {
        return;
    };
    if v.pending.is_some_and(|(t, _)| t == ticket) && !v.sent {
        v.pending = None;
        v.connection.not_sent(ticket);
        if v.cancelled {
            v.connection.cancel();
        }
        v.release();
    }
    let dialog = v.dialog;
    if let Some(d) = s.app_home.workflow.as_mut().filter(|d| d.id == dialog) {
        d.notice = Some(
            "The connection action was not sent. Review the current workflow before trying again."
                .into(),
        );
    }
    s.app_home.workflow_salesforce.retain(Session::keep);
}

fn settle(
    s: &mut ShellState,
    instance: u64,
    ticket: Ticket,
    action: Action,
    result: Result<Value, Error>,
) -> Option<Effect> {
    let v = s
        .app_home
        .workflow_salesforce
        .iter()
        .find(|v| v.id == instance)?;
    if v.pending != Some((ticket, action)) || !v.sent {
        return None;
    }
    let current = v.current(s);
    let origin = v.origin.clone();
    if let Err(error) = &result {
        failure(s, &origin.owner.ctx, error);
    }
    let definitely_unsent = matches!(&result, Err(Error::RateLimited { .. }))
        || matches!(&result, Err(Error::Api(code)) if code == "block_interaction_not_sent_offline");
    let v = s
        .app_home
        .workflow_salesforce
        .iter_mut()
        .find(|v| v.id == instance)?;
    v.release();
    v.pending = None;
    v.sent = false;
    let effect = if definitely_unsent {
        v.connection.not_sent(ticket);
        None
    } else {
        match result {
            Ok(raw) => v.connection.response(ticket, raw).ok(),
            Err(_) => v.connection.failed(ticket),
        }
    };
    if v.connection.uncertain() && !matches!(action, Action::List(_)) && !definitely_unsent {
        v.unresolved.push((ticket, action));
    }
    let dialog = v.dialog;
    let unknown = !v.unresolved.is_empty();
    if !current {
        v.cancelled = true;
        v.connection.cancel();
    }
    s.app_home.workflow_salesforce.retain(Session::keep);
    if current && unknown {
        if let Some(d) = s.app_home.workflow.as_mut().filter(|d| d.id == dialog) {
            d.notice = Some("The Salesforce action was not confirmed. Refresh and review its current connection status before starting the workflow.".into());
        }
    }
    current.then_some(effect).flatten()
}

/// The user explicitly reviews a fresh actual read. This retires only the
/// recorded local unknown attempts; it neither repeats nor certifies those calls.
pub(super) fn use_current(s: &mut ShellState, instance: u64) -> bool {
    let current = s
        .app_home
        .workflow_salesforce
        .iter()
        .find(|v| v.id == instance)
        .is_some_and(|v| v.current(s) && v.pending.is_none());
    if !current {
        return false;
    }
    let v = s
        .app_home
        .workflow_salesforce
        .iter_mut()
        .find(|v| v.id == instance)
        .unwrap();
    if !v.connection.use_current_connection() {
        return false;
    }
    v.unresolved.clear();
    true
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    SignIn,
    Disconnect,
    Check,
    BrowserReturn,
    UseCurrent,
}
pub(super) fn command(mut state: Signal<ShellState>, instance: u64, command: Command) {
    let effect = {
        let mut s = state.write();
        let Some(v) = s
            .app_home
            .workflow_salesforce
            .iter()
            .find(|v| v.id == instance)
        else {
            return;
        };
        if !v.current(&s)
            || v.pending.is_some()
            || v.queued.is_some()
            || s.app_home
                .workflow
                .as_ref()
                .is_none_or(|d| d.id != v.dialog || d.busy)
            || cooling(&s, &v.origin.owner.ctx)
        {
            return;
        }
        if matches!(command, Command::UseCurrent) {
            use_current(&mut s, instance);
            return;
        }
        let v = s
            .app_home
            .workflow_salesforce
            .iter_mut()
            .find(|v| v.id == instance)
            .unwrap();
        match command {
            Command::SignIn => v.connection.sign_in(),
            Command::Disconnect => v.connection.disconnect(),
            Command::Check => v.connection.check(),
            Command::BrowserReturn => v
                .browser
                .take()
                .and_then(|ticket| v.connection.browser_returned(ticket)),
            Command::UseCurrent => unreachable!(),
        }
    };
    if let Some(effect) = effect {
        dispatch(state, instance, effect);
    }
}

/// Caller has already matched the actual account realtime generation. Each
/// target additionally binds the live Home visit, workflow, app/provider and org.
pub(super) fn realtime(s: &mut ShellState, team: &str, event: &Value) {
    let Some(view) = s.app_home.surface.as_ref().and_then(|h| h.view.clone()) else {
        return;
    };
    let Some(d) = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| d.origin.current(s) && d.origin.owner.ctx.session.team_id == team)
    else {
        return;
    };
    let Some(observation) = d.salesforce_observation.as_ref().filter(|o| o.current(s)) else {
        return;
    };
    let workspace = d.origin.owner.ctx.session.clone();
    let targets: Vec<_> = d
        .review
        .connections()
        .iter()
        .filter_map(|a| {
            Target::capture(
                &d.review,
                &view,
                &workspace,
                &a.function,
                &a.provider,
                observation.native().context(),
            )
            .ok()
            .map(|target| (d.id, target, workspace.clone()))
        })
        .collect();
    for (dialog, target, workspace) in targets {
        if let Some(d) = s.app_home.workflow.as_mut().filter(|d| d.id == dialog) {
            let _ = d.review.salesforce_event(&target, event, &view, &workspace);
        }
    }
}
