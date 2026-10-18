//! Installed-app shortcut chooser; existing modal correlation owns outcomes.
use super::source::Source;
use crate::{
    scheduled::Context,
    state::{ChannelTab, ComposerTarget, MainView, Overlay, ShellState},
};
use dioxus::prelude::*;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    block_actions::{modal::ViewedConversation, shortcuts as domain},
    slack::Error,
};
mod calendar;
pub(crate) mod fixture;
pub(crate) mod view;
#[derive(Default)]
pub(crate) struct State {
    dialog: Option<Dialog>,
    calendar_attempts: Vec<calendar::Attempt>,
    cooldowns: BTreeMap<(u64, String), Option<Instant>>,
}
struct Owner {
    ctx: Arc<Context>,
    channel: String,
    generation: u64,
    thread: Option<(String, u64)>,
    main: MainView,
    tab: ChannelTab,
    rt: u64,
    source: Option<Source>,
    related: Vec<String>,
    channel_scope: domain::grid::ChannelScope,
}
impl Owner {
    fn current(&self, s: &ShellState) -> bool {
        super::inputs::user::directory::context_current(s, &self.ctx)
            && s.core
                .workspaces
                .get(&self.ctx.session.team_id)
                .is_some_and(|w| {
                    w.self_user_id == self.ctx.session.user_id && w.rt_generation == self.rt
                })
            && s.core.active_channel.as_deref() == Some(self.channel.as_str())
            && s.channel_generation == self.generation
            && s.main_view == self.main
            && s.channel_tab == self.tab
            && self.thread.as_ref().is_none_or(|(root, g)| {
                s.thread_root.as_ref() == Some(root) && s.thread_generation == *g
            })
            && self.source.as_ref().is_none_or(|v| v.current(s))
            && channel_scope(s, &self.ctx, &self.channel).as_ref() == Some(&self.channel_scope)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Loading,
    Choosing,
    Sending,
    Acknowledged,
    Uncertain,
    Failed,
}
struct Dialog {
    fixture: bool,
    calendar: Option<calendar::Calendar>,
    id: u64,
    owner: Owner,
    kind: domain::Kind,
    viewed: Option<ViewedConversation>,
    catalog: Option<domain::Catalog>,
    query: String,
    page: usize,
    selected: Option<usize>,
    team: Option<String>,
    phase: Phase,
    notice: Option<String>,
    token: Option<crate::app_modal::ActionToken>,
}
fn channel_scope(s: &ShellState, c: &Context, id: &str) -> Option<domain::grid::ChannelScope> {
    let ch = s
        .core
        .workspaces
        .get(&c.session.team_id)?
        .channels
        .get(id)?;
    domain::grid::ChannelScope::capture(ch)
}
fn related(s: &ShellState, c: &Context, id: &str) -> Option<Vec<String>> {
    let scope = channel_scope(s, c, id)?;
    scope.relevant(None).or_else(|| {
        // Retained ordinary workspace fallback. It does not supply Grid members.
        (!scope.needs_membership() && c.session.enterprise_id.is_none())
            .then(|| vec![c.session.team_id.clone()])
    })
}
// This is a read result, never a new credential or workspace authorization.
fn adopt_membership(s: &mut ShellState, id: u64, membership: domain::grid::Membership) -> bool {
    if !is_current(s, id) {
        return false;
    }
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    if d.phase != Phase::Loading {
        return false;
    }
    let Some(related) = d.owner.channel_scope.relevant(Some(&membership)) else {
        return false;
    };
    d.owner.related = related;
    true
}

pub(crate) fn open_global(state: Signal<ShellState>, target: ComposerTarget, expected: &str) {
    if crate::view::composer::field_scope(&state.peek(), target) != expected {
        return;
    }
    open(state, domain::Kind::Global, None, Some(target));
}
pub(crate) fn open_message(state: Signal<ShellState>, source: Source) {
    open(state, domain::Kind::Message, Some(source), None)
}
fn open(
    mut state: Signal<ShellState>,
    kind: domain::Kind,
    source: Option<Source>,
    target: Option<ComposerTarget>,
) {
    if crate::fixture::is_fixture() {
        return;
    }
    let start = {
        let mut s = state.write();
        if s.overlay.is_some() || s.channel_tab != ChannelTab::Messages || !s.detail_open() {
            return;
        }
        cancel(&mut s);
        let Some(ctx) = Context::capture(&s).map(Arc::new) else {
            return;
        };
        let Some(channel) = s.core.active_channel.clone() else {
            return;
        };
        let Some(channel_scope) = channel_scope(&s, &ctx, &channel) else {
            s.show_toast("Workspace information is unavailable for shortcuts.");
            return;
        };
        let needs_membership = channel_scope.needs_membership();
        let related = if needs_membership {
            Vec::new()
        } else {
            let Some(related) = related(&s, &ctx, &channel) else {
                s.show_toast("Workspace information is unavailable for shortcuts.");
                return;
            };
            related
        };
        if source.as_ref().is_some_and(|v| !v.current(&s)) {
            return;
        }
        let thread = if target == Some(ComposerTarget::Thread) {
            let Some(root) = s.thread_root.clone() else {
                return;
            };
            Some((root, s.thread_generation))
        } else {
            None
        };
        let Some(rt) = s
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .map(|w| w.rt_generation)
        else {
            return;
        };
        let id = super::next_id();
        let viewed = crate::app_modal::capture_shortcut_viewed(&s);
        let owner = Owner {
            ctx: ctx.clone(),
            channel,
            generation: s.channel_generation,
            thread,
            main: s.main_view,
            tab: s.channel_tab,
            rt,
            source,
            related,
            channel_scope,
        };
        let cooling = cooling(&s, &ctx);
        s.message_buttons.shortcuts.dialog = Some(Dialog {
            fixture: false,
            calendar: None,
            id,
            owner,
            kind,
            viewed,
            catalog: None,
            query: String::new(),
            page: 0,
            selected: None,
            team: None,
            phase: if cooling {
                Phase::Failed
            } else {
                Phase::Loading
            },
            notice: cooling.then(|| {
                "Slack asked us to wait. Close this dialog and try again after the rate limit."
                    .into()
            }),
            token: None,
        });
        s.overlay = Some(Overlay::AppShortcuts);
        (!cooling).then_some((id, ctx, needs_membership))
    };
    if let Some((id, ctx, needs_membership)) = start {
        dioxus::core::spawn_forever(async move {
            // Owner is checked immediately before dispatch, not only after loading.
            if !is_current(&state.peek(), id) {
                return;
            }
            if needs_membership {
                let result = ctx
                    .transport
                    .execute(domain::grid::discovery(&ctx.client, &ctx.session))
                    .await
                    .and_then(|raw| domain::grid::Membership::decode(raw, &ctx.session));
                let mut s = state.write();
                retain_limit(&mut s, &ctx, result.as_ref().err());
                if !is_current(&s, id) {
                    return;
                }
                let admitted =
                    result.is_ok_and(|membership| adopt_membership(&mut s, id, membership));
                if !admitted {
                    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
                    d.phase = Phase::Failed;
                    d.notice = Some(
                        "Workspace membership could not be loaded. Your message is unchanged."
                            .into(),
                    );
                    return;
                }
            }
            // Membership resolution never bypasses the existing owner fence.
            if !is_current(&state.peek(), id) {
                return;
            }
            let result = ctx
                .transport
                .execute(domain::discovery(&ctx.client, &ctx.session))
                .await
                .and_then(domain::Catalog::decode);
            let mut s = state.write();
            retain_limit(&mut s, &ctx, result.as_ref().err());
            if !is_current(&s, id) {
                return;
            }
            let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
            match result {
                Ok(c) => {
                    d.catalog = Some(c);
                    d.phase = Phase::Choosing
                }
                Err(_) => {
                    d.phase = Phase::Failed;
                    d.notice =
                        Some("Shortcuts could not be loaded. Your message is unchanged.".into())
                }
            }
        });
    }
}
fn is_current(s: &ShellState, id: u64) -> bool {
    s.overlay == Some(Overlay::AppShortcuts)
        && s.message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .is_some_and(|d| d.id == id && d.owner.current(s))
}
fn retain_limit(s: &mut ShellState, ctx: &Context, error: Option<&Error>) {
    if !super::inputs::user::directory::owner_current(s, ctx) {
        return;
    }
    if let Some(Error::RateLimited { retry_after_secs }) = error {
        let map = &mut s.message_buttons.shortcuts.cooldowns;
        map.retain(|(epoch, _), deadline| {
            *epoch == ctx.epoch && deadline.is_none_or(|d| d > Instant::now())
        });
        // An unrepresentable server deadline remains blocked; never shorten it.
        map.insert(
            (ctx.epoch, ctx.session.team_id.clone()),
            Instant::now().checked_add(Duration::from_secs(retry_after_secs.unwrap_or(60))),
        );
    }
}
fn cooling(s: &ShellState, ctx: &Context) -> bool {
    s.message_buttons
        .shortcuts
        .cooldowns
        .get(&(ctx.epoch, ctx.session.team_id.clone()))
        .is_some_and(|t| t.is_none_or(|d| d > Instant::now()))
}
pub(crate) fn cancel(s: &mut ShellState) {
    if let Some(d) = s.message_buttons.shortcuts.dialog.take() {
        if let Some(token) = d.token {
            crate::app_modal::reject_action(s, &token)
        }
    }
    if s.overlay == Some(Overlay::AppShortcuts) {
        s.overlay = None;
        s.shortcut_generation = s.shortcut_generation.wrapping_add(1)
    }
}
fn cancel_owned(s: &mut ShellState, id: u64) {
    if s.message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .is_some_and(|d| d.id == id)
        && s.overlay == Some(Overlay::AppShortcuts)
    {
        cancel(s);
    }
}
fn page(s: &mut ShellState, id: u64, next: bool) {
    if !is_current(s, id) {
        return;
    }
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    if d.phase != Phase::Choosing {
        return;
    }
    let count = matching(d).count();
    d.page = if next {
        (d.page + 1).min(count.saturating_sub(1) / 64)
    } else {
        d.page.saturating_sub(1)
    };
}
fn matching(d: &Dialog) -> impl Iterator<Item = (usize, &domain::Action)> {
    let query = d.query.to_lowercase();
    d.catalog
        .as_ref()
        .into_iter()
        .flat_map(|c| c.actions.iter().enumerate())
        .filter(move |(_, a)| {
            a.kind == d.kind
                && a.available_in_chooser(&d.owner.related)
                && query.split_whitespace().all(|q| {
                    a.name.to_lowercase().contains(q)
                        || a.app_name.to_lowercase().contains(q)
                        || a.description.to_lowercase().contains(q)
                })
        })
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let retire = s
        .message_buttons
        .shortcuts
        .dialog
        .as_ref()
        .is_some_and(|d| {
            !d.owner.current(s)
                || (s.overlay != Some(Overlay::AppShortcuts)
                    && s.overlay != Some(Overlay::AppModal))
        });
    if retire {
        cancel(s);
    }
    retire
}
fn query(s: &mut ShellState, id: u64, value: String) {
    if !is_current(s, id) || value.len() > 4096 {
        return;
    }
    if let Some(d) = s
        .message_buttons
        .shortcuts
        .dialog
        .as_mut()
        .filter(|d| d.phase == Phase::Choosing)
    {
        d.query = value;
        d.page = 0;
    }
}
fn choose(s: &mut ShellState, id: u64, index: usize) {
    if !is_current(s, id) {
        return;
    }
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    if d.phase != Phase::Choosing {
        return;
    }
    let Some(a) = d
        .catalog
        .as_ref()
        .and_then(|c| c.actions.get(index))
        .filter(|a| a.kind == d.kind && a.available_in_chooser(&d.owner.related))
    else {
        return;
    };
    let teams = a.applicable_teams(&d.owner.related);
    d.team = (d.kind == domain::Kind::Global && teams.len() == 1).then(|| teams[0].clone());
    d.selected = Some(index);
}
fn choose_team(s: &mut ShellState, id: u64, team: String) {
    if !is_current(s, id) {
        return;
    }
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    if d.phase != Phase::Choosing || d.kind != domain::Kind::Global {
        return;
    }
    if d.selected
        .and_then(|i| d.catalog.as_ref()?.actions.get(i))
        .is_some_and(|a| a.applicable_teams(&d.owner.related).contains(&team))
    {
        d.team = Some(team)
    }
}
fn prepare_launch(
    s: &mut ShellState,
    id: u64,
) -> Option<(Arc<Context>, super_platinum_core::slack::PreparedRequest)> {
    if !is_current(&s, id) {
        return None;
    }
    let d = s.message_buttons.shortcuts.dialog.as_ref().unwrap();
    if d.phase != Phase::Choosing || d.fixture {
        return None;
    }
    let Some(action) = d
        .selected
        .and_then(|i| d.catalog.as_ref()?.actions.get(i))
        .cloned()
    else {
        return None;
    };
    let ctx = d.owner.ctx.clone();
    if cooling(&s, &ctx) {
        return None;
    }
    let context = domain::Context {
        channel: Some(d.owner.channel.clone()),
        message_ts: d
            .owner
            .source
            .as_ref()
            .map(|v| v.owner().message_ts.clone()),
    };
    let viewed = d.viewed.clone();
    let Some(token) = crate::app_modal::register_action_with_context(
        s,
        &action.app,
        context.channel.clone(),
        viewed,
    ) else {
        return None;
    };
    let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
    let request = domain::prepare(
        &ctx.client,
        &ctx.session,
        d.catalog.as_ref().unwrap(),
        &action,
        &context,
        &d.owner.related,
        d.team.as_deref(),
        &token.token,
    );
    match request {
        Ok(r) => {
            d.phase = Phase::Sending;
            d.token = Some(token);
            Some((ctx, r))
        }
        Err(_) => {
            crate::app_modal::reject_action(s, &token);
            None
        }
    }
}
fn launch(mut state: Signal<ShellState>, id: u64) {
    if calendar::begin(state, id) {
        return;
    }
    let prepared = prepare_launch(&mut state.write(), id);
    let Some((ctx, request)) = prepared else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !is_current(&state.peek(), id) {
            let mut s = state.write();
            poll(&mut s);
            return;
        }
        let result = ctx.transport.execute_block_interaction(request).await;
        let mut s = state.write();
        retain_limit(&mut s, &ctx, result.as_ref().err());
        poll(&mut s);
        // A correlated modal may have arrived before the HTTP reply. Preserve it.
        if !is_current(&s, id) {
            return;
        }
        let outcome = result
            .as_ref()
            .map(domain::decode)
            .unwrap_or(domain::Outcome::Unknown);
        let known_unsent = matches!(&result,Err(Error::Api(v)) if v=="block_interaction_not_sent_offline")
            || matches!(&result, Err(Error::RateLimited { .. }));
        let d = s.message_buttons.shortcuts.dialog.as_mut().unwrap();
        match outcome {
            domain::Outcome::Acknowledged => {
                d.phase = Phase::Acknowledged;
                d.notice=Some("Shortcut sent to the app. A form may open; this does not confirm the app's work is complete.".into())
            }
            domain::Outcome::Restricted | domain::Outcome::Rejected => {
                d.phase = Phase::Failed;
                d.notice=Some("The app declined this shortcut or access is restricted. Your message is unchanged.".into())
            }
            domain::Outcome::Unknown => {
                d.phase = if known_unsent {
                    Phase::Failed
                } else {
                    Phase::Uncertain
                };
                d.notice=Some(if known_unsent{"The shortcut was not sent. Wait for connectivity or the rate limit before a new attempt."}else{"The app's reply could not be confirmed. This shortcut will not be repeated automatically."}.into())
            }
        }
        if d.phase == Phase::Failed {
            let token = d.token.take();
            if let Some(t) = token {
                crate::app_modal::reject_action(&mut s, &t)
            }
        }
    });
}

#[cfg(test)]
mod tests;
