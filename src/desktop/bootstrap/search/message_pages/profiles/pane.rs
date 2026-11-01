//! Sealed Search-only handoff; post-open ownership does not reuse RowSource.
use super::*;
use std::sync::Mutex;
pub(crate) struct Open {
    prepared: Arc<Prepared>,
    actor: Arc<Actor>,
    dialog: u64,
}
pub(crate) struct Pane {
    origin: Arc<Prepared>,
    actor: Arc<Actor>,
    data: Mutex<Arc<Prepared>>,
    denied: AtomicBool,
    dialog: u64,
    channel: u64,
    thread: u64,
    view: crate::state::MainView,
    profile: u64,
}
#[derive(Clone)]
pub(crate) struct Back {
    pane: Arc<Pane>,
    serial: u64,
}
pub(crate) struct Refreshed {
    prepared: Arc<Prepared>,
    actor: Arc<Actor>,
}
impl Pane {
    pub(crate) fn base_current(&self, s: &ShellState) -> bool {
        s.overlay.is_none()
            && s.profile_user.as_deref() == Some(self.actor.id.as_str())
            && s.profile_generation == self.profile
            && s.search_open_generation == self.dialog
            && s.channel_generation == self.channel
            && s.thread_generation == self.thread
            && s.main_view == self.view
            && actor_current(s, &self.origin, &self.actor)
    }
    pub(crate) fn data_current(&self, s: &ShellState) -> bool {
        self.base_current(s)
            && !self.denied.load(Ordering::Acquire)
            && self.data.lock().is_ok_and(|p| {
                !p.pending
                    && !p.failed
                    && p.context.base(s)
                    && p.context.policy_current(s)
                    && actor_current(s, &p, &self.actor)
                    && !p.hidden.contains(&self.actor.id)
                    && p.members.contains_key(&self.actor.id)
            })
    }
    pub(crate) fn withdraw(&self) {
        self.denied.store(true, Ordering::Release);
    }
    pub(crate) fn refresh_started(&self, s: &mut ShellState) {
        // Retire only author activation, retaining valid passive pane details.
        let old = s
            .search_message_pages
            .as_ref()
            .and_then(|b| b.profiles.prepared.clone());
        if let Some(p) = old {
            if Arc::ptr_eq(&p.roster, &self.origin.roster) {
                fail_pending(s, &p);
            }
        }
    }
    pub(crate) fn back(self: &Arc<Self>, serial: u64) -> Back {
        Back {
            pane: self.clone(),
            serial,
        }
    }
}
impl Back {
    pub(crate) fn current(&self, s: &ShellState, pane: &Arc<Pane>, serial: u64) -> bool {
        self.serial == serial && Arc::ptr_eq(&self.pane, pane) && pane.base_current(s)
    }
}
impl Open {
    pub(crate) fn team(&self) -> &str {
        &self.prepared.context.frame.team
    }
    /// Only the consumed click produces this value. No Projection constructor.
    pub(crate) fn details(
        &self,
        s: &ShellState,
    ) -> Option<super_platinum_core::profile_reading::Projection> {
        if s.overlay.is_some()
            || s.search_open_generation != self.dialog
            // The exact socket claim already linearized before departure. A
            // later queued ingress may retire presentation after native apply,
            // but cannot strand the user between Search and this sealed pane.
            || self.prepared.pending
            || self.prepared.failed
            || !self.prepared.context.serial(s)
            || !self.prepared.context.policy_current(s)
            || !actor_serial(s, &self.prepared, &self.actor)
        {
            return None;
        }
        self.prepared.members.get(&self.actor.id).cloned()
    }
    pub(crate) fn pane(self, s: &ShellState) -> Option<Arc<Pane>> {
        if s.overlay.is_some()
            || s.profile_user.as_deref() != Some(&self.actor.id)
            || s.search_open_generation != self.dialog
        {
            return None;
        }
        Some(Arc::new(Pane {
            origin: self.prepared.clone(),
            actor: self.actor,
            data: Mutex::new(self.prepared),
            denied: AtomicBool::new(false),
            dialog: self.dialog,
            channel: s.channel_generation,
            thread: s.thread_generation,
            view: s.main_view,
            profile: s.profile_generation,
        }))
    }
}
pub(crate) fn open(mut state: Signal<ShellState>, control: Control) {
    let Ok(mut s) = state.try_write() else { return };
    if !control.current(&s) {
        return;
    }
    let Some(dialog) = s.search_open_generation.checked_add(1) else {
        return;
    };
    if s.profile_generation == u64::MAX || s.thread_generation == u64::MAX {
        return;
    }
    let Some(watch) = control.prepared.context.watch(&s, &control.actor.key.0) else {
        return;
    };
    if !control.current(&s) || !watch.current() {
        return;
    }
    // This is the synchronous single-use claim only. No owner/socket calls here.
    if watch.with_current(|| {
        control
            .used
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }) != Some(true)
    {
        return;
    }
    if !ready(&s, &control.prepared) || !actor_current(&s, &control.prepared, &control.actor) {
        return;
    }
    let open = Open {
        prepared: control.prepared,
        actor: control.actor,
        dialog,
    };
    s.dismiss_overlay();
    if s.overlay.is_some() || s.search_open_generation != dialog {
        return;
    }
    crate::bootstrap::profile_search::show_search(&mut s, open);
}
pub(crate) fn back(mut state: Signal<ShellState>, token: Back) {
    if let Ok(mut s) = state.try_write() {
        crate::bootstrap::profile_search::back_search(&mut s, token);
    }
}
impl Refreshed {
    pub(crate) fn user(
        &self,
        s: &ShellState,
        pane: &Arc<Pane>,
    ) -> Option<super_platinum_core::slack::models::User> {
        if !pane.base_current(s)
            || !Arc::ptr_eq(&self.actor, &pane.actor)
            || !ready(s, &self.prepared)
            || !actor_current(s, &self.prepared, &self.actor)
            || self.prepared.hidden.contains(&self.actor.id)
        {
            return None;
        }
        Some(self.prepared.members.get(&self.actor.id)?.user().clone())
    }
    pub(crate) fn bind(self, pane: &Arc<Pane>) {
        if let Ok(mut data) = pane.data.lock() {
            *data = self.prepared;
            pane.denied.store(false, Ordering::Release);
        }
    }
}
pub(crate) async fn refresh(
    mut state: Signal<ShellState>,
    ticket: crate::bootstrap::profile_search::SearchTicket,
) {
    #[cfg(test)]
    let io = super::model_io();
    #[cfg(test)]
    if io.is_none() {
        ticket.finish(&mut state.write(), None, true);
        return;
    }
    let context = {
        let s = state.peek();
        if !ticket.current(&s) {
            drop(s);
            ticket.finish(&mut state.write(), None, true);
            return;
        }
        ticket.pane().origin.context.refreshed(&s)
    };
    let Some(context) = context else {
        ticket.finish(&mut state.write(), None, true);
        return;
    };
    if context.cooling(&state.peek()) {
        ticket.finish(&mut state.write(), None, true);
        return;
    }
    let p = ticket.pane().clone();
    let Some(watch) = context.watch(&state.peek(), &p.actor.key.0) else {
        ticket.finish(&mut state.write(), None, true);
        return;
    };
    let result = async {
        let current =
            || ticket.current(&state.peek()) && context.current(&state.peek()) && watch.current();
        if !current() {
            return Err(super_platinum_core::slack::Error::Api(
                "search_author_not_requested".into(),
            ));
        }
        let f = &context.frame;
        let request =
            super_platinum_core::block_actions::name_policy::request::user(&f.client, &f.session);
        #[cfg(test)]
        let raw = io.as_ref().unwrap().execute(request).await?;
        #[cfg(not(test))]
        let raw = f
            .transport
            .execute_admitted(
                request,
                current,
                || true,
                |result| run::observe_attempt(state, &context, result),
            )
            .await?
            .ok_or_else(|| {
                super_platinum_core::slack::Error::Api("search_author_not_requested".into())
            })?;
        if !current() {
            return Err(super_platinum_core::slack::Error::Api(
                "search_author_not_requested".into(),
            ));
        }
        let hidden = run::parse_policy(&raw).ok_or_else(|| {
            super_platinum_core::slack::Error::Api("search_author_unqualified".into())
        })?;
        if hidden.contains(&p.actor.id) {
            return Ok(None);
        }
        let ids = vec![p.actor.id.clone()];
        let request =
            super_platinum_core::slack::edge::profile_users_info(&f.client, &f.session, &ids)
                .map_err(|_| {
                    super_platinum_core::slack::Error::Api("search_author_not_requested".into())
                })?;
        #[cfg(test)]
        let raw = io.as_ref().unwrap().execute(request).await?;
        #[cfg(not(test))]
        let raw = f
            .transport
            .execute_admitted(
                request,
                current,
                || true,
                |result| run::observe_attempt(state, &context, result),
            )
            .await?
            .ok_or_else(|| {
                super_platinum_core::slack::Error::Api("search_author_not_requested".into())
            })?;
        if !current() {
            return Err(super_platinum_core::slack::Error::Api(
                "search_author_not_requested".into(),
            ));
        }
        let members = run::humans(&raw, &ids, &f.team).ok_or_else(|| {
            super_platinum_core::slack::Error::Api("search_author_unqualified".into())
        })?;
        Ok(Some(Refreshed {
            prepared: Arc::new(Prepared {
                context: context.clone(),
                roster: p.origin.roster.clone(),
                members,
                hidden,
                pending: false,
                failed: false,
                retry_at: None,
                blocked: false,
            }),
            actor: p.actor.clone(),
        }))
    }
    .await;
    let Ok(mut s) = state.try_write() else { return };
    if let Err(error) = &result {
        context.note_failure(&mut s, error);
    }
    if !ticket.current(&s) || !context.current(&s) || !watch.current() {
        ticket.finish(&mut s, None, true);
        return;
    }
    match result {
        Ok(value) => {
            let value = value.and_then(|v| v.user(&s, &p).map(|u| (v, u)));
            let published = watch.with_current(|| ticket.finish_unchecked(&mut s, value));
            if published.is_none() {
                ticket.finish(&mut s, None, true);
            }
        }
        Err(error) => ticket.finish(&mut s, None, run::retain_failure(&error)),
    }
}
