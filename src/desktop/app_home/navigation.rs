//! A user-owned app lookup. Neither an app ID nor a late response may invent a DM.
use super::*;
mod workspace;
use super_platinum_core::{
    block_actions::home::navigation::{Tab, Target},
    slack::models::{Channel, User},
};

#[derive(Clone)]
pub(super) struct Pending {
    id: u64,
    target: Target,
    ctx: Arc<Context>,
    channel: Option<String>,
    generation: u64,
    thread: u64,
    main: MainView,
    tab: ChannelTab,
    rt: u64,
    home: Option<(u64, u64)>,
    home_input_revision: Option<u64>,
    calendar: Option<u64>,
    native_lease: Option<inputs::native::Release>,
}
impl Pending {
    fn current(&self, s: &ShellState) -> bool {
        crate::message_buttons::inputs::user::directory::context_current(s, &self.ctx)
            && s.core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&self.ctx.session.team_id))
                .is_some_and(|workspace| {
                    workspace.team_id == self.ctx.session.team_id
                        && workspace.url == self.ctx.session.url
                        && workspace.enterprise_id == self.ctx.session.enterprise_id
                })
            && self.calendar.map_or_else(
                || s.overlay.is_none(),
                |id| crate::message_buttons::shortcuts::calendar_home_navigation_current(s, id),
            )
            && s.core.active_channel == self.channel
            && s.channel_generation == self.generation
            && s.thread_generation == self.thread
            && s.main_view == self.main
            && s.channel_tab == self.tab
            && s.core
                .workspaces
                .get(&self.ctx.session.team_id)
                .is_some_and(|w| {
                    w.self_user_id == self.ctx.session.user_id && w.rt_generation == self.rt
                })
            && s.app_home
                .navigation
                .as_ref()
                .is_some_and(|n| n.id == self.id)
            && self.home.is_none_or(|(visit, revision)| {
                inputs::ready(s)
                    && inputs::revision(s) == self.home_input_revision
                    && s.app_home.surface.as_ref().is_some_and(|h| {
                        h.id == visit && h.revision == revision && h.owner.current(s)
                    })
            })
    }
    fn begin(s: &mut ShellState, target: Target) -> Option<Self> {
        Self::begin_from(s, target, None)
    }
    fn begin_from(s: &mut ShellState, target: Target, calendar: Option<u64>) -> Option<Self> {
        if !calendar.map_or_else(
            || s.overlay.is_none(),
            |id| crate::message_buttons::shortcuts::calendar_home_navigation_current(s, id),
        ) {
            return None;
        }
        let ctx = Arc::new(Context::capture(s)?);
        if !crate::message_buttons::inputs::user::directory::context_current(s, &ctx)
            || cooling(s, &ctx)
            || s.app_home
                .navigation
                .as_ref()
                .is_some_and(|n| n.target == target && n.current(s))
        {
            return None;
        }
        let w = s.core.workspaces.get(&ctx.session.team_id)?;
        if w.self_user_id != ctx.session.user_id {
            return None;
        }
        let home = if s.channel_tab == ChannelTab::AppHome {
            let h = s.app_home.surface.as_ref().filter(|h| h.owner.current(s))?;
            if h.loading
                || h.confirmation.is_some()
                || h.calendar_confirmation.is_some()
                || h.fixture
                || !inputs::ready(s)
            {
                return None;
            }
            Some((h.id, h.revision))
        } else {
            None
        };
        let pending = Self {
            id: next(),
            target,
            ctx,
            channel: s.core.active_channel.clone(),
            generation: s.channel_generation,
            thread: s.thread_generation,
            main: s.main_view,
            tab: s.channel_tab,
            rt: w.rt_generation,
            home,
            home_input_revision: inputs::revision(s),
            calendar,
            native_lease: None,
        };
        clear(s);
        s.app_home.navigation = Some(pending.clone());
        Some(pending)
    }
    fn release(&self, s: &mut ShellState) {
        if s.app_home
            .navigation
            .as_ref()
            .is_some_and(|n| n.id == self.id)
        {
            if let Some(lease) = s
                .app_home
                .navigation
                .as_mut()
                .and_then(|n| n.native_lease.take())
            {
                lease.release();
            }
            s.app_home.navigation = None;
        }
    }
    fn read_current(&self, s: &ShellState) -> bool {
        self.current(s) && !cooling(s, &self.ctx)
    }
    fn adopt(
        &self,
        s: &mut ShellState,
        home: &Home,
        tab: Tab,
        channel: Channel,
        user: User,
    ) -> bool {
        if !self.current(s)
            || home.app != self.target.app
            || home.installed_team != self.target.installed_team
            || channel.id != home.channel
            || !channel.is_im
            || channel.is_archived
            || channel.user.as_deref() != Some(&user.id)
            || !user.is_bot
            || user.deleted
        {
            return false;
        }
        let Some(w) = s.core.workspaces.get_mut(&self.ctx.session.team_id) else {
            return false;
        };
        w.users.insert(user.id.clone(), user);
        w.channels.insert(channel.id.clone(), channel);
        s.refresh_from_core();
        let Some(index) = s.channels.iter().position(|c| c.id == home.channel) else {
            return false;
        };
        self.release(s);
        if let Some(id) = self.calendar {
            crate::message_buttons::shortcuts::close_calendar_home_navigation(s, id);
        }
        s.select_channel(index, crate::state::ChannelOpen::Global);
        s.channel_tab = match tab {
            Tab::Home => ChannelTab::AppHome,
            Tab::About => ChannelTab::AppAbout,
            _ => ChannelTab::Messages,
        };
        true
    }
}

/// Caller must originate this explicit intent from its own current UI callback.
/// Canvas save/discard retains the typed target, never a guessed bot mapping.
pub(crate) fn open(mut state: Signal<ShellState>, target: Target) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::AppHome(target.clone()),
    ) {
        return;
    }
    let pending = Pending::begin(&mut state.write(), target);
    if let Some(pending) = pending {
        dioxus::core::spawn_forever(run(state, pending));
    }
}

/// Exact request cancellation. A late old callback cannot clear a newer request.
pub(crate) fn cancel(s: &mut ShellState, id: u64) {
    if s.app_home.navigation.as_ref().is_some_and(|n| n.id == id) {
        if let Some(lease) = s
            .app_home
            .navigation
            .as_mut()
            .and_then(|n| n.native_lease.take())
        {
            lease.release();
        }
        s.app_home.navigation = None;
    }
}
pub(super) fn clear(s: &mut ShellState) {
    if let Some(pending) = s.app_home.navigation.take() {
        if let Some(lease) = pending.native_lease {
            lease.release();
        }
    }
}
/// Calendar's parsed URI may name another already signed-in workspace.
/// Workspace handoff is synchronous; the existing once-only lookup owns every
/// later await and returned conversation. No outgoing token is reused.
pub(crate) fn open_calendar_link(
    mut state: Signal<ShellState>,
    link: home::navigation::Link,
    id: u64,
) -> bool {
    if link.workspace != link.target.installed_team {
        return false;
    }
    if state.peek().core.active_team.as_deref() == Some(link.workspace.as_str()) {
        open_from_calendar(state, link.target, id);
        return true;
    }
    let pending = workspace::begin(&mut state.write(), link, id);
    if let Some(pending) = pending {
        dioxus::core::spawn_forever(run(state, pending));
        true
    } else {
        false
    }
}

pub(crate) fn open_from_calendar(mut state: Signal<ShellState>, target: Target, id: u64) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::AppHome(target.clone()),
    ) {
        return;
    }
    let pending = Pending::begin_from(&mut state.write(), target, Some(id));
    if let Some(pending) = pending {
        dioxus::core::spawn_forever(run(state, pending));
    }
}
pub(crate) fn status(mut state: Signal<ShellState>) -> Element {
    let pending = pending_id(&state.read());
    rsx! {if let Some(id)=pending {div{role:"status",class:"app-home-notice","Opening app Home…" button{onclick:move |_|cancel(&mut state.write(),id),"Cancel"}}}}
}
pub(crate) fn pending_id(s: &ShellState) -> Option<u64> {
    s.app_home
        .navigation
        .as_ref()
        .filter(|n| n.current(s))
        .map(|n| n.id)
}

async fn run(mut state: Signal<ShellState>, pending: Pending) {
    let native_lease = if let Some((visit, revision)) = pending.home {
        let Some(lease) = inputs::native::lock_owned(state, actions::Key { visit, revision }).await
        else {
            pending.release(&mut state.write());
            return;
        };
        let mut s = state.write();
        if !pending.read_current(&s) {
            lease.release();
            pending.release(&mut s);
            return;
        }
        s.app_home.navigation.as_mut().unwrap().native_lease = Some(lease.clone());
        Some(lease)
    } else {
        None
    };
    let result = async {
        if !pending.read_current(&state.peek()) {
            return Ok(None);
        }
        let ctx = &pending.ctx;
        let request = pending.target.prepare(&ctx.client, &ctx.session)?;
        // Opening app Home can create/open a bot DM; never retry this dispatch.
        let raw = ctx.transport.execute_block_interaction(request).await?;
        if !pending.read_current(&state.peek()) {
            return Ok(None);
        }
        let (home, _) = pending.target.decode(&raw)?;
        let ingress =
            crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session)
                .for_channels([&home.channel]);
        let raw = ctx
            .transport
            .execute(home::fetch(&ctx.client, &ctx.session, &home.channel)?)
            .await?;
        if !pending.read_current(&state.peek()) {
            return Ok(None);
        }
        let (observed, _) = Home::decode(&home.channel, &raw)?;
        if observed.app != home.app || observed.installed_team != home.installed_team {
            return Err(Error::Api("app_home_navigation_identity_changed".into()));
        }
        let tab = home::navigation::choose_tab(
            pending.target.requested,
            observed.messages,
            observed.enabled,
        );
        let channel: Channel = serde_json::from_value(
            raw.get("channel")
                .cloned()
                .ok_or_else(|| Error::Api("app_home_navigation_channel_missing".into()))?,
        )
        .map_err(|_| Error::Api("app_home_navigation_channel_invalid".into()))?;
        let user_id = channel
            .user
            .as_ref()
            .filter(|_| channel.id == home.channel && channel.is_im)
            .ok_or_else(|| Error::Api("app_home_navigation_channel_invalid".into()))?;
        let users = api::fetch_users_info(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            vec![user_id.clone()],
        )
        .await?;
        if !pending.current(&state.peek()) {
            return Ok(None);
        }
        let mut users = users.into_iter().filter(|u| &u.id == user_id);
        let user = users
            .next()
            .ok_or_else(|| Error::Api("app_home_navigation_user_invalid".into()))?;
        if users.next().is_some() {
            return Err(Error::Api("app_home_navigation_user_invalid".into()));
        }
        Ok::<_, Error>(Some((observed, tab, channel, user, ingress)))
    }
    .await;
    if let Some(lease) = native_lease {
        lease.release();
    }
    let mut accepted_ingress = None;
    let opened = {
        let mut s = state.write();
        if !pending.current(&s) {
            // Retired UI cannot adopt or toast, but the same account still owes
            // Slack the returned cooldown. No follow-up request is scheduled.
            if let Err(Error::RateLimited { retry_after_secs }) = &result {
                retain_limit(&mut s, &pending.ctx, *retry_after_secs);
            }
            pending.release(&mut s);
            return;
        }
        match result {
            Ok(Some((home, tab, channel, user, ingress))) => {
                let observed = channel.clone();
                let opened = pending.adopt(&mut s, &home, tab, channel, user);
                if opened {
                    accepted_ingress = Some((ingress, observed));
                }
                pending.release(&mut s);
                opened.then_some(tab)
            }
            Ok(None) => {
                pending.release(&mut s);
                None
            }
            Err(e) => {
                failure(&mut s, &pending.ctx, &e);
                pending.release(&mut s);
                None
            }
        }
    };
    if let Some((ingress, channel)) = accepted_ingress {
        ingress.channels(&state, &[channel]);
    }
    match opened {
        Some(Tab::Home) => super::refresh(state).await,
        Some(Tab::About) => super::about::refresh(state).await,
        Some(Tab::Messages) => crate::bootstrap::refresh_selected_channel(state).await,
        _ => {}
    }
}

pub(super) fn retain_limit(s: &mut ShellState, ctx: &Context, seconds: Option<u64>) {
    if !crate::message_buttons::inputs::user::directory::owner_current(s, ctx) {
        return;
    }
    let deadline = Instant::now().checked_add(Duration::from_secs(seconds.unwrap_or(60)));
    s.app_home
        .cooldowns
        .retain(|(epoch, _), _| *epoch == ctx.epoch);
    s.app_home
        .cooldowns
        .entry((ctx.epoch, ctx.session.team_id.clone()))
        .and_modify(|old| {
            *old = match (*old, deadline) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => None,
            }
        })
        .or_insert(deadline);
}

#[cfg(test)]
mod tests;
