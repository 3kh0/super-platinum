//! Owner-bound transient ordinary history, shared by live and disconnected sources.
use crate::state::ShellState;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use super_platinum_core::{
    history_pagination::Pages,
    slack::{Error, api::HistoryArgs},
};
pub(crate) mod check;
mod context;
pub(crate) mod fixture;
mod notice;
pub(super) use context::Context;
pub(crate) use notice::render;

const PAGE_BUDGET: usize = 8;
const TIME_BUDGET: Duration = Duration::from_secs(20);
type Key = (String, String);
#[derive(Default)]
pub(crate) struct State {
    next: u64,
    entries: BTreeMap<Key, Entry>,
}
struct Entry {
    owner: Arc<context::Owner>,
    account: Option<String>,
    epoch: u64,
    scope: String,
    ticket: u64,
    pages: Pages,
    loading: bool,
    error: bool,
    paused: bool,
    restart_required: bool,
    initial: bool,
}

pub(super) fn ticket_current(s: &ShellState, team: &str, channel: &str, ticket: u64) -> bool {
    s.ordinary_history
        .entries
        .get(&(team.into(), channel.into()))
        .is_some_and(|e| e.ticket == ticket && e.owner.current(&s.core))
}
fn entry<'a>(s: &'a ShellState, team: &str, channel: &str) -> Option<&'a Entry> {
    s.ordinary_history
        .entries
        .get(&(team.into(), channel.into()))
        .filter(|e| e.owner.current(&s.core))
}
fn unread_owned(s: &ShellState) -> bool {
    s.read_presentation
        .unread_loading
        .as_ref()
        .is_some_and(|(scope, _)| scope == &s.timeline_scope())
        || s.core.pending_scroll_to.as_ref().is_some_and(|(c, t)| {
            s.core.active_channel.as_ref() == Some(c)
                && matches!(
                    t,
                    super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(_)
                )
        })
}
pub(super) fn unread_start(s: &mut ShellState, team: &str, channel: &str) {
    s.ordinary_history
        .entries
        .remove(&(team.into(), channel.into()));
    if s.core.active_team.as_deref() == Some(team)
        && s.core.active_channel.as_deref() == Some(channel)
    {
        s.loading_older = false;
    }
}
pub(super) fn automatic(s: &ShellState, team: &str, channel: &str) -> bool {
    if unread_owned(s) {
        return false;
    }
    if let Some(e) = entry(s, team, channel) {
        return e.pages.has_more() && !e.loading && !e.error && !e.paused;
    }
    s.core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(channel))
        .is_some_and(|b| b.has_more_older)
}
pub(super) async fn refresh(
    mut state: Signal<ShellState>,
    context: Context,
    team: &str,
    channel: &str,
    latest: Option<String>,
    resume: bool,
    reconnect: Option<&super::reconnect::Visit>,
) -> Option<u64> {
    let owner = context.owner();
    let (ticket, generation) = {
        let mut s = state.write();
        if !owner.current(&s.core) || !super::reconnect::history_gate::current(&s, reconnect) {
            return None;
        }
        let old = entry(&s, team, channel);
        if resume && old.is_some_and(|e| e.loading || !e.pages.has_more()) {
            return None;
        }
        let pages = if resume {
            old.map(|e| e.pages.clone())
        } else {
            None
        }
        .unwrap_or_else(|| {
            Pages::new(HistoryArgs {
                channel: channel.into(),
                inclusive: latest.is_some() && !resume,
                latest,
                limit: Some(100),
                ..Default::default()
            })
        });
        let initial = if resume {
            old.is_some_and(|e| e.initial)
        } else {
            true
        };
        s.ordinary_history.next = s.ordinary_history.next.wrapping_add(1);
        let ticket = s.ordinary_history.next;
        let account = s.core.active_account.clone();
        let epoch = s.core.account_epoch;
        let scope = s.timeline_scope();
        if s.core.active_team.as_deref() == Some(team)
            && s.core.active_channel.as_deref() == Some(channel)
        {
            s.loading_older = resume;
        }
        s.ordinary_history.entries.insert(
            (team.into(), channel.into()),
            Entry {
                owner: owner.clone(),
                account,
                epoch,
                scope,
                ticket,
                pages,
                loading: true,
                error: false,
                paused: false,
                restart_required: false,
                initial,
            },
        );
        if let Some(w) = s.core.workspaces.get_mut(team) {
            w.messages
                .entry(channel.into())
                .or_default()
                .history_refreshing = true;
        }
        (ticket, s.channel_generation)
    };
    let current = |s: &ShellState| {
        ticket_current(s, team, channel, ticket)
            && super::reconnect::history_gate::current(s, reconnect)
    };
    let deadline = tokio::time::Instant::now() + TIME_BUDGET;
    for index in 0..PAGE_BUDGET {
        if !current(&state.peek()) {
            retire(state, team, channel, ticket);
            return Some(ticket);
        }
        let args = state.peek().ordinary_history.entries[&(team.into(), channel.into())]
            .pages
            .request()
            .clone();
        let file_read = state.write().core.begin_file_display_read(team);
        let pending = fixture::request(&state.peek(), ticket, args.clone());
        let request_id = pending.as_ref().map(|p| p.id);
        let result = tokio::time::timeout_at(deadline, context.fetch(args, pending))
            .await
            .unwrap_or_else(|_| Err(Error::Api("history_page_timeout".into())));
        if !current(&state.peek()) {
            fixture::response(request_id, &state.peek(), false, "retired");
            retire(state, team, channel, ticket);
            return Some(ticket);
        }
        let result = result.and_then(|page| {
            state
                .peek()
                .core
                .finish_file_display_read(team, file_read)
                .map(|files| (page, files))
                .map_err(|e| Error::Api(e.into()))
        });
        let (page, files) = match result {
            Ok(value) => value,
            Err(error) => {
                let expired = matches!(&error,Error::Api(code) if code=="invalid_cursor");
                let reset = expired.then(|| {
                    state.peek().ordinary_history.entries[&(team.into(), channel.into())]
                        .pages
                        .expired()
                });
                if let Some(Ok(pages)) = reset {
                    state
                        .write()
                        .ordinary_history
                        .entries
                        .get_mut(&(team.into(), channel.into()))
                        .unwrap()
                        .pages = pages;
                    fixture::response(request_id, &state.peek(), true, "expired-restart");
                    if index + 1 < PAGE_BUDGET {
                        continue;
                    }
                    finish(state, team, channel, ticket, false, true);
                    return Some(ticket);
                }
                if expired {
                    state
                        .write()
                        .ordinary_history
                        .entries
                        .get_mut(&(team.into(), channel.into()))
                        .unwrap()
                        .restart_required = true;
                }
                finish(state, team, channel, ticket, true, false);
                fixture::response(
                    request_id,
                    &state.peek(),
                    true,
                    if expired { "expired-refused" } else { "error" },
                );
                state
                    .write()
                    .report_failure(&error, "", || "Could not load messages. Try again.".into());
                return Some(ticket);
            }
        };
        let advanced = {
            state.peek().ordinary_history.entries[&(team.into(), channel.into())]
                .pages
                .advance(&page)
        };
        let pages = match advanced {
            Ok(pages) => pages,
            Err(_) => {
                finish(state, team, channel, ticket, true, false);
                fixture::response(request_id, &state.peek(), true, "invalid-page");
                return Some(ticket);
            }
        };
        let more = pages.has_more();
        let grew = {
            let mut s = state.write();
            let uploads =
                crate::messaging::upload_recovery::incoming(&s, team, channel, &page.messages);
            let mut grew = false;
            let mut has_rows = false;
            if let Some(w) = s.core.workspaces.get_mut(team) {
                let bag = w.messages.entry(channel.into()).or_default();
                let count = bag.messages.len();
                for message in page.messages {
                    let message = files.message(message);
                    if super_platinum_core::state::is_channel_timeline_visible(&message) {
                        bag.upsert(message);
                    }
                }
                grew = bag.messages.len() > count;
                has_rows = !bag.messages.is_empty();
                bag.loaded = true;
                bag.history_failed = false;
                bag.has_more_older = more;
            }
            let e = s
                .ordinary_history
                .entries
                .get_mut(&(team.into(), channel.into()))
                .unwrap();
            e.pages = pages;
            if has_rows {
                e.initial = false;
            }
            crate::messaging::upload_recovery::reconcile(&mut s, team, channel, Some(&uploads));
            s.refresh_from_core();
            grew
        };
        fixture::response(request_id, &state.peek(), true, "merged");
        if !more || grew {
            finish(state, team, channel, ticket, false, false);
            break;
        }
        if index + 1 == PAGE_BUDGET || tokio::time::Instant::now() >= deadline {
            finish(state, team, channel, ticket, false, true);
            break;
        }
    }
    if current(&state.peek()) {
        dioxus::prelude::spawn(crate::performance::mark_history_painted(
            state,
            team.into(),
            channel.into(),
            generation,
        ));
    }
    Some(ticket)
}
fn finish(
    mut state: Signal<ShellState>,
    team: &str,
    channel: &str,
    ticket: u64,
    error: bool,
    paused: bool,
) {
    let mut s = state.write();
    if !ticket_current(&s, team, channel, ticket) {
        return;
    }
    let e = s
        .ordinary_history
        .entries
        .get_mut(&(team.into(), channel.into()))
        .unwrap();
    e.loading = false;
    e.error = error;
    e.paused = paused;
    if let Some(b) = s
        .core
        .workspaces
        .get_mut(team)
        .and_then(|w| w.messages.get_mut(channel))
    {
        b.history_failed = error;
        b.history_refreshing = false;
    }
}

pub(crate) async fn resume(mut state: Signal<ShellState>) {
    let (team, channel, scope, epoch, context, latest) = {
        let s = state.peek();
        if unread_owned(&s) {
            return;
        }
        let (Some(team), Some(channel)) =
            (s.core.active_team.clone(), s.core.active_channel.clone())
        else {
            return;
        };
        let context = if fixture::enabled(&s) {
            Context::fixture(&s)
        } else {
            s.core.transport.as_ref().and_then(|t| {
                s.core
                    .session
                    .as_ref()
                    .and_then(|a| a.workspaces.get(&team))
                    .and_then(|w| Context::live(&s, t, &s.core.client, w))
            })
        };
        let Some(context) = context else {
            return;
        };
        if entry(&s, &team, &channel)
            .is_some_and(|e| e.loading || e.restart_required || !e.pages.has_more())
        {
            return;
        }
        let latest = s
            .core
            .workspaces
            .get(&team)
            .and_then(|w| w.messages.get(&channel))
            .and_then(|b| b.oldest_ts());
        (
            team,
            channel,
            s.timeline_scope(),
            s.core.account_epoch,
            context,
            latest,
        )
    };
    state.write().loading_older = true;
    let ticket = refresh(state, context, &team, &channel, latest, true, None).await;
    if state.peek().timeline_scope() != scope
        || state.peek().core.account_epoch != epoch
        || ticket.is_none_or(|t| !ticket_current(&state.peek(), &team, &channel, t))
    {
        return;
    }
    state.write().loading_older = false;
    if !fixture::enabled(&state.peek()) {
        super::common::persist_workspace(&state, &team);
        super::session::hydrate_current_surface(state).await;
    }
}

fn retire(mut state: Signal<ShellState>, team: &str, channel: &str, ticket: u64) {
    release(&mut state.write(), team, channel, ticket);
}
fn release(s: &mut ShellState, team: &str, channel: &str, ticket: u64) {
    let Some(e) = s
        .ordinary_history
        .entries
        .get(&(team.into(), channel.into()))
    else {
        return;
    };
    // Credential retirement forbids publication but still releases this exact
    // ticket in the same account. Never clear a replacement account or read.
    if e.ticket != ticket || e.account != s.core.active_account || e.epoch != s.core.account_epoch {
        return;
    }
    let same_view = e.scope == s.timeline_scope();
    s.ordinary_history
        .entries
        .get_mut(&(team.into(), channel.into()))
        .unwrap()
        .loading = false;
    if let Some(b) = s
        .core
        .workspaces
        .get_mut(team)
        .and_then(|w| w.messages.get_mut(channel))
    {
        b.history_refreshing = false;
    }
    if same_view {
        s.loading_older = false;
    }
}

#[cfg(test)]
mod tests;
