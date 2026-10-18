//! App About is a separate, visit-owned read surface. It never adopts Home input.
use super::*;
use super_platinum_core::block_actions::home::about::{Connection, Flags, Info, Profile};
mod actions;
mod description;
pub(crate) mod fixture;
pub(crate) mod management;
mod view;
pub(crate) use view::panel;

#[derive(Default)]
pub(super) struct State {
    management: management::State,
    surface: Option<Surface>,
    attempts: Vec<actions::Attempt>,
    correlations: Vec<(String, Owner)>,
}
struct Surface {
    owner: Owner,
    id: u64,
    revision: u64,
    home: Option<Home>,
    info: Option<Info>,
    profile: Option<Profile>,
    flags: Option<Flags>,
    connection: Option<Connection>,
    connection_enabled: Option<bool>,
    loading: bool,
    notice: Option<String>,
    fixture: bool,
}
#[derive(Clone)]
struct Owner {
    ctx: Arc<Context>,
    channel: String,
    bot: String,
    visit: u64,
    main: MainView,
    rt: u64,
}
impl Owner {
    fn capture(s: &ShellState) -> Option<Self> {
        if !eligible(s) || s.channel_tab != ChannelTab::AppAbout {
            return None;
        }
        let ctx = Arc::new(Context::capture(s)?);
        let w = s.core.workspaces.get(&ctx.session.team_id)?;
        let channel = s.core.active_channel.clone()?;
        let bot = w.channels.get(&channel)?.user.clone()?;
        Some(Self {
            ctx,
            channel,
            bot,
            visit: s.channel_generation,
            main: s.main_view,
            rt: w.rt_generation,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        crate::message_buttons::inputs::user::directory::context_current(s, &self.ctx)
            && s.core
                .session
                .as_ref()
                .and_then(|v| v.workspaces.get(&self.ctx.session.team_id))
                .is_some_and(|w| {
                    w.url == self.ctx.session.url
                        && w.enterprise_id == self.ctx.session.enterprise_id
                })
            && s.core.active_channel.as_ref() == Some(&self.channel)
            && s.channel_generation == self.visit
            && s.main_view == self.main
            && s.channel_tab == ChannelTab::AppAbout
            && s.detail_open()
            && s.core
                .workspaces
                .get(&self.ctx.session.team_id)
                .is_some_and(|w| {
                    w.self_user_id == self.ctx.session.user_id
                        && w.rt_generation == self.rt
                        && w.channels.get(&self.channel).is_some_and(|c| {
                            c.is_im && !c.is_archived && c.user.as_ref() == Some(&self.bot)
                        })
                        && w.users
                            .get(&self.bot)
                            .is_some_and(|u| u.is_bot && !u.deleted)
                })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    id: u64,
    revision: u64,
}
fn current(s: &ShellState, key: Key) -> bool {
    s.app_home
        .about
        .surface
        .as_ref()
        .is_some_and(|h| h.id == key.id && h.revision == key.revision && h.owner.current(s))
}
fn read_current(s: &ShellState, key: Key) -> bool {
    current(s, key)
        && s.overlay.is_none()
        && s.app_home
            .about
            .surface
            .as_ref()
            .is_some_and(|h| !cooling(s, &h.owner.ctx))
}
/// About owns Activity independently of App Home. Name preferences arriving
/// on this owner's socket must retire its observed policy as well.
pub(super) fn name_policy_changed(
    s: &ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> bool {
    // Workflow documents additionally bind the selected account ID. About's
    // shared context alone checks epoch/transport/credentials, not that ID.
    if !management::showing(s) {
        return false;
    }
    let Some(h) = s.app_home.about.surface.as_ref().filter(|h| {
        h.owner.current(s) && h.owner.ctx.session.team_id == team && h.owner.rt == generation
    }) else {
        return false;
    };
    match event {
        RtEvent::PreferencesChanged { .. } => true,
        RtEvent::UserChanged(user) => user.id == h.owner.ctx.session.user_id,
        RtEvent::Unknown(raw) => matches!(
            raw.kind.as_str(),
            "team_pref_change" | "enterprise_prefs_updated" | "team_join" | "team_leave"
        ),
        _ => false,
    }
}
pub(super) fn clear(s: &mut ShellState) {
    s.app_home.about.surface = None;
}

pub(crate) async fn refresh(mut state: Signal<ShellState>) {
    let Some((owner, key)) = begin_read(&mut state.write()) else {
        return;
    };
    let result = async {
        if !read_current(&state.peek(), key) {
            return Ok(None);
        }
        let ctx = &owner.ctx;
        let ingress =
            crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session)
                .for_channels([&owner.channel]);
        let raw = request(
            state,
            key,
            &owner,
            home::fetch(&ctx.client, &ctx.session, &owner.channel)?,
        )
        .await?;
        if !read_current(&state.peek(), key) {
            return Ok(None);
        }
        let (home, _) = Home::decode(&owner.channel, &raw)?;
        if raw
            .pointer("/channel/user")
            .and_then(serde_json::Value::as_str)
            != Some(&owner.bot)
        {
            return Err(Error::Api("app_about_bot_changed".into()));
        }
        if let Ok(channel) = serde_json::from_value::<super_platinum_core::slack::models::Channel>(
            raw["channel"].clone(),
        ) {
            ingress.channels(&state, &[channel]);
        }
        let raw = request(
            state,
            key,
            &owner,
            Info::request(&ctx.client, &ctx.session, &home.app)?,
        )
        .await?;
        if !read_current(&state.peek(), key) {
            return Ok(None);
        }
        let info = Info::decode(&home.app, raw)?;
        // Keep the verified About response visible if optional connection/policy
        // acquisition subsequently fails. Loading still blocks all actions.
        {
            let mut s = state.write();
            if !adopt_info(&mut s, key, home.clone(), info.clone()) {
                return Ok(None);
            }
        }
        let raw = request(
            state,
            key,
            &owner,
            super_platinum_core::block_actions::app_eligibility::profile_request(
                &ctx.client,
                &ctx.session,
                &home.app,
            )?,
        )
        .await?;
        if !read_current(&state.peek(), key) {
            return Ok(None);
        }
        let profile = Profile::decode(&home.app, raw)?;
        // Reuse the existing authenticated experiment request; absence of a
        // completed assignment response never turns the feature on.
        let raw = request(
            state,
            key,
            &owner,
            api::list_experiments(&ctx.client, &ctx.session),
        )
        .await?;
        if !read_current(&state.peek(), key) {
            return Ok(None);
        }
        let flags = Flags::decode(&raw)?;
        let enabled = flags.connection;
        let connection = if enabled {
            let raw = request(
                state,
                key,
                &owner,
                Connection::request(&ctx.client, &ctx.session, &home.app)?,
            )
            .await?;
            if !read_current(&state.peek(), key) {
                return Ok(None);
            }
            Some(Connection::decode(&home.app, raw)?)
        } else {
            None
        };
        Ok::<_, Error>(Some((home, info, profile, flags, connection)))
    }
    .await;
    let mut s = state.write();
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        navigation::retain_limit(&mut s, &owner.ctx, *retry_after_secs);
    }
    if !current(&s, key) {
        return;
    }
    if let Err(e) = &result {
        failure(&mut s, &owner.ctx, e)
    }
    let h = s.app_home.about.surface.as_mut().unwrap();
    h.loading = false;
    match result {
        Ok(Some((home, info, profile, flags, connection))) => {
            h.home = Some(home);
            h.info = Some(info);
            h.profile = Some(profile);
            h.flags = Some(flags);
            h.connection_enabled = Some(flags.connection);
            h.connection = connection;
            h.notice = None;
        }
        Ok(None) => {
            h.notice = Some("This read was interrupted. Refresh to check again.".into());
        }
        Err(_) => {
            h.notice =
                Some("About could not be refreshed. Check the connection and refresh again.".into())
        }
    }
}
async fn request(
    state: Signal<ShellState>,
    key: Key,
    owner: &Owner,
    request: super_platinum_core::slack::PreparedRequest,
) -> Result<serde_json::Value, Error> {
    if !read_current(&state.peek(), key) || !owner.current(&state.peek()) {
        return Err(Error::Api("app_about_read_retired".into()));
    }
    if crate::fixture::is_fixture() {
        return fixture::read_response(&state.peek(), &request);
    }
    owner.ctx.transport.execute(request).await
}
fn adopt_info(s: &mut ShellState, key: Key, home: Home, info: Info) -> bool {
    if !read_current(s, key) || home.app != info.app() {
        return false;
    }
    let h = s.app_home.about.surface.as_mut().unwrap();
    h.home = Some(home);
    h.info = Some(info);
    true
}
fn begin_read(s: &mut ShellState) -> Option<(Owner, Key)> {
    if s.overlay.is_some() {
        return None;
    }
    let owner = Owner::capture(s)?;
    if cooling(s, &owner.ctx) {
        return None;
    }
    if s.app_home
        .about
        .surface
        .as_ref()
        .is_some_and(|h| h.owner.current(s) && (h.loading || h.fixture))
    {
        return None;
    }
    let id = next();
    let revision = next();
    s.app_home.about.surface = Some(Surface {
        owner: owner.clone(),
        id,
        revision,
        home: None,
        info: None,
        profile: None,
        flags: None,
        connection: None,
        connection_enabled: None,
        loading: true,
        notice: None,
        fixture: fixture::read_enabled(s),
    });
    Some((owner, Key { id, revision }))
}
pub(crate) fn accepts_modal(
    s: &ShellState,
    raw: &super_platinum_core::slack::events::RawEvent,
) -> bool {
    let Some(token) = raw
        .rest
        .get("client_token")
        .and_then(serde_json::Value::as_str)
    else {
        return true;
    };
    s.app_home
        .about
        .correlations
        .iter()
        .find(|(t, _)| t == token)
        .is_none_or(|(_, o)| o.current(s))
        && !s
            .app_home
            .about
            .attempts
            .iter()
            .any(|a| a.token.token == token && a.cancelled)
}
/// Called only after the existing modal reducer has accepted the correlated event.
pub(crate) fn modal_adopted(
    s: &mut ShellState,
    raw: &super_platinum_core::slack::events::RawEvent,
) {
    if raw.kind != "view_opened" {
        return;
    }
    let Some(token) = raw
        .rest
        .get("client_token")
        .and_then(serde_json::Value::as_str)
    else {
        return;
    };
    s.app_home.about.attempts.retain(|a| a.token.token != token);
}

#[cfg(test)]
mod tests;
