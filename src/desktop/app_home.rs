//! Bot conversation Home, with visit-owned reads and once-only app entry actions.
use crate::{
    scheduled::Context,
    state::{ChannelTab, MainView, Overlay, ShellState},
};
use dioxus::prelude::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use super_platinum_core::{
    block_actions::home::{self, Button, Home, View},
    slack::{Error, api, events::RtEvent},
};
mod actions;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;
pub(crate) mod view;

#[derive(Default)]
pub(crate) struct Root {
    correlations: Vec<(String, Owner)>,
    pub(crate) fixture_forms: Vec<serde_json::Value>,
    surface: Option<Surface>,
    attempts: Vec<actions::Attempt>,
    cooldowns: std::collections::BTreeMap<(u64, String), Option<Instant>>,
}
struct Surface {
    owner: Owner,
    id: u64,
    revision: u64,
    loading: bool,
    home: Option<Home>,
    view: Option<View>,
    notice: Option<String>,
    opened: bool,
    confirmation: Option<Button>,
    fixture: bool,
}
#[derive(Clone)]
struct Owner {
    ctx: Arc<Context>,
    channel: String,
    visit: u64,
    home_visit: u64,
    main: MainView,
    rt: u64,
}
impl Owner {
    fn capture(s: &ShellState) -> Option<Self> {
        if !eligible(s) || s.channel_tab != ChannelTab::AppHome {
            return None;
        }
        let ctx = Arc::new(Context::capture(s)?);
        let w = s.core.workspaces.get(&ctx.session.team_id)?;
        (w.self_user_id == ctx.session.user_id).then(|| Self {
            ctx,
            channel: s.core.active_channel.clone().unwrap(),
            visit: s.channel_generation,
            home_visit: next(),
            main: s.main_view,
            rt: w.rt_generation,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        crate::message_buttons::inputs::user::directory::context_current(s, &self.ctx)
            && s.core
                .workspaces
                .get(&self.ctx.session.team_id)
                .is_some_and(|w| {
                    w.self_user_id == self.ctx.session.user_id && w.rt_generation == self.rt
                })
            && s.core.active_channel.as_ref() == Some(&self.channel)
            && s.channel_generation == self.visit
            && s.main_view == self.main
            && s.channel_tab == ChannelTab::AppHome
            && s.detail_open()
            && s.app_home
                .surface
                .as_ref()
                .is_some_and(|h| h.id == self.home_visit)
    }
}
fn next() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}
pub(crate) fn eligible(s: &ShellState) -> bool {
    let Some(w) = s
        .core
        .active_team
        .as_ref()
        .and_then(|t| s.core.workspaces.get(t))
    else {
        return false;
    };
    s.detail_open()
        && s.core
            .active_channel
            .as_ref()
            .and_then(|c| w.channels.get(c))
            .is_some_and(|c| {
                c.is_im
                    && c.user
                        .as_ref()
                        .and_then(|u| w.users.get(u))
                        .is_some_and(|u| u.is_bot && !u.deleted)
            })
}
fn current(s: &ShellState, id: u64) -> bool {
    s.app_home
        .surface
        .as_ref()
        .is_some_and(|h| h.id == id && h.owner.current(s))
}
fn cooling(s: &ShellState, ctx: &Context) -> bool {
    s.app_home
        .cooldowns
        .get(&(ctx.epoch, ctx.session.team_id.clone()))
        .is_some_and(|d| d.is_none_or(|d| d > Instant::now()))
}
fn failure(s: &mut ShellState, ctx: &Context, e: &Error) {
    if !crate::message_buttons::inputs::user::directory::owner_current(s, ctx) {
        return;
    }
    if let Error::RateLimited { retry_after_secs } = e {
        s.app_home
            .cooldowns
            .retain(|(epoch, _), _| *epoch == ctx.epoch);
        let deadline =
            Instant::now().checked_add(Duration::from_secs(retry_after_secs.unwrap_or(60)));
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
    s.report_failure(e,"Reconnect to load this app.",||"The app request could not be confirmed. Try a read-only refresh when the connection is available.".into());
}
pub(crate) async fn refresh(mut state: Signal<ShellState>) {
    let (owner, id, revision, initial) = {
        let mut s = state.write();
        if s.overlay.is_some() {
            return;
        }
        let Some(owner) = Owner::capture(&s) else {
            return;
        };
        if cooling(&s, &owner.ctx) {
            return;
        }
        let replace = s
            .app_home
            .surface
            .as_ref()
            .is_none_or(|h| !h.owner.current(&s));
        if replace {
            s.app_home.surface = Some(Surface {
                owner: owner.clone(),
                id: owner.home_visit,
                revision: 0,
                loading: false,
                home: None,
                view: None,
                notice: None,
                opened: false,
                confirmation: None,
                fixture: false,
            })
        }
        let h = s.app_home.surface.as_mut().unwrap();
        if h.loading || h.fixture {
            return;
        }
        h.loading = true;
        h.revision = next();
        h.confirmation = None;
        h.notice = None;
        (h.owner.clone(), h.id, h.revision, !h.opened)
    };
    let result = async {
        let req = home::fetch(&owner.ctx.client, &owner.ctx.session, &owner.channel)?;
        let raw = owner.ctx.transport.execute(req).await?;
        let (home, mut view) = Home::decode(&owner.channel, &raw)?;
        if view.is_none() {
            if let Some(id) = home.view_id.as_deref() {
                let raw = owner
                    .ctx
                    .transport
                    .execute_block_interaction(api::block_view_get(
                        &owner.ctx.client,
                        &owner.ctx.session,
                        id,
                    )?)
                    .await?;
                match api::decode_block_view_get(&raw)? {
                    api::BlockViewResponse::Accepted(v) => view = Some(View::decode(&home, v)?),
                    api::BlockViewResponse::Rejected(_) => {
                        return Err(Error::Api("app_home_view_unavailable".into()));
                    }
                }
            }
        }
        Ok::<_, Error>((home, view))
    }
    .await;
    let opened = {
        let mut s = state.write();
        if !current(&s, id) || s.app_home.surface.as_ref().unwrap().revision != revision {
            return;
        }
        if let Err(e) = &result {
            failure(&mut s, &owner.ctx, e)
        }
        let h = s.app_home.surface.as_mut().unwrap();
        h.loading = false;
        match result {
            Ok((home, view)) => {
                h.home = Some(home.clone());
                h.view = view;
                h.notice = if !home.enabled {
                    Some("This app does not have a Home tab. Open Messages to continue.".into())
                } else if h.view.is_none() {
                    Some(
                        "The app has not published Home content yet. Refresh to check again."
                            .into(),
                    )
                } else {
                    None
                };
                if initial && home.enabled {
                    h.opened = true;
                    Some(home)
                } else {
                    None
                }
            }
            Err(_) => {
                h.notice =
                    Some("Home content could not be loaded. Refresh to try the read again.".into());
                None
            }
        }
    };
    if let Some(home) = opened {
        if !current(&state.peek(), id) {
            return;
        }
        let result = match home::opened(&owner.ctx.client, &owner.ctx.session, &home) {
            Ok(req) => owner.ctx.transport.execute_block_interaction(req).await,
            Err(e) => Err(e),
        };
        let mut s = state.write();
        if !current(&s, id) {
            return;
        }
        if let Err(e) = &result {
            failure(&mut s, &owner.ctx, e)
        }
        if !result
            .as_ref()
            .is_ok_and(|r| r.get("ok").and_then(serde_json::Value::as_bool) == Some(true))
        {
            s.app_home.surface.as_mut().unwrap().notice=Some("The app's Home-open notification was not confirmed. It will not be repeated on refresh.".into());
        }
    }
}
/// Socket caller supplies the accepted account's realtime generation. A home
/// event cannot reuse modal token authority or navigate to another app/channel.
pub(crate) fn realtime(s: &mut ShellState, team: &str, generation: u64, event: &RtEvent) -> bool {
    let RtEvent::Unknown(raw) = event else {
        return false;
    };
    if !matches!(raw.kind.as_str(), "view_opened" | "view_updated") {
        return false;
    }
    let Some(h) = s.app_home.surface.as_ref().filter(|h| {
        h.owner.current(s) && h.owner.ctx.session.team_id == team && h.owner.rt == generation
    }) else {
        return false;
    };
    let Some(home) = h.home.as_ref() else {
        return false;
    };
    if raw.rest.get("app_id").and_then(serde_json::Value::as_str) != Some(&home.app) {
        return false;
    }
    let Some(id) = raw.rest.get("view_id").and_then(serde_json::Value::as_str) else {
        return false;
    };
    let mut next_home = home.clone();
    if raw.kind == "view_opened" {
        if raw
            .rest
            .get("view_type")
            .and_then(serde_json::Value::as_str)
            != Some("home")
            || raw
                .rest
                .get("channel_id")
                .and_then(serde_json::Value::as_str)
                != Some(&home.channel)
        {
            return false;
        }
        next_home.view_id = Some(id.into());
        next_home.enabled = true;
    } else if home.view_id.as_deref() != Some(id) {
        return false;
    }
    let Some(payload) = raw.rest.get("view") else {
        let h = s.app_home.surface.as_mut().unwrap();
        h.home = Some(next_home);
        h.view = None;
        h.revision = next();
        h.loading = false;
        h.confirmation = None;
        h.notice = Some("Home changed. Refresh to load its current content.".into());
        return true;
    };
    let Ok(view) = View::decode(&next_home, payload.clone()) else {
        return false;
    };
    let h = s.app_home.surface.as_mut().unwrap();
    h.home = Some(next_home);
    h.view = Some(view);
    h.revision = next();
    h.loading = false;
    h.confirmation = None;
    h.notice = None;
    false
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
        .correlations
        .iter()
        .find(|(t, _)| t == token)
        .is_none_or(|(_, o)| o.current(s))
}

pub(crate) fn leave(s: &mut ShellState) {
    s.app_home.surface = None;
}
