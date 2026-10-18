//! A cold first-unread opening must reach the unread boundary, not fetch before
//! it. conversations.history returns newest first; cursor/time pagination is
//! documented at https://docs.slack.dev/reference/methods/conversations.history/.
use crate::realtime::activity_counts::CursorOwner;
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    domain::PendingScrollTarget,
    slack::{Error, SlackClient, Transport, api},
    state::cmp_ts,
};

const PAGE_BUDGET: usize = 8;
const TIME_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

fn current(
    s: &ShellState,
    owner: &CursorOwner,
    scope: &str,
    channel: &str,
    boundary: &str,
) -> bool {
    owner.current(&s.core)
        && s.timeline_scope() == scope
        && s.core.pending_scroll_to
            == Some((
                channel.into(),
                PendingScrollTarget::FirstUnreadAfter(boundary.into()),
            ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ShellState, Arc<Transport>, WorkspaceSession) {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        let team = s.core.active_team.clone().unwrap();
        let channel = s.core.active_channel.clone().unwrap();
        let session = WorkspaceSession {
            team_id: team.clone(),
            user_id: s.core.workspaces[&team].self_user_id.clone(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.invalid".into(),
            token: "fixture-token".into(),
        };
        s.core.active_account = Some("isolated-read-owner".into());
        s.core.session = Some(super_platinum_core::config::Session {
            unified_user_client_enabled: Default::default(),
            d_cookie: "fixture-cookie".into(),
            workspaces: [(team, session.clone())].into(),
        });
        let transport = Arc::new(Transport::new("fixture-cookie").unwrap());
        s.core.transport = Some(transport.clone());
        s.core.pending_scroll_to = Some((
            channel,
            PendingScrollTarget::FirstUnreadAfter("10.000001".into()),
        ));
        (s, transport, session)
    }
    #[test]
    fn same_transport_credential_rotation_cannot_accept_old_history_pages() {
        let (mut s, transport, session) = fixture();
        let scope = s.timeline_scope();
        let channel = s.core.active_channel.clone().unwrap();
        let owner = CursorOwner::capture(&s.core, &transport, &session).unwrap();
        assert!(current(&s, &owner, &scope, &channel, "10.000001"));
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&session.team_id)
            .unwrap()
            .token = "rotated-fixture-token".into();
        assert!(!current(&s, &owner, &scope, &channel, "10.000001"));
    }
    #[test]
    fn rotation_cleanup_releases_only_the_task_that_owned_the_spinner() {
        let (mut s, _, session) = fixture();
        let channel = s.core.active_channel.clone().unwrap();
        let ticket = (s.timeline_scope(), "10.000001".into());
        let account = s.core.active_account.clone();
        let epoch = s.core.account_epoch;
        s.read_presentation.unread_fetching = Some(ticket.clone());
        s.core
            .workspaces
            .get_mut(&session.team_id)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap()
            .history_refreshing = true;
        release(
            &mut s,
            &("other-visit".into(), "10.000001".into()),
            &account,
            epoch,
            &session.team_id,
            &channel,
            true,
        );
        assert!(s.core.workspaces[&session.team_id].messages[&channel].history_refreshing);
        release(
            &mut s,
            &ticket,
            &account,
            epoch,
            &session.team_id,
            &channel,
            true,
        );
        assert!(!s.core.workspaces[&session.team_id].messages[&channel].history_refreshing);
        assert!(s.read_presentation.unread_fetching.is_none());
        assert!(s.read_presentation.unread_paused);
    }
}

fn release(
    s: &mut ShellState,
    ticket: &(String, String),
    account: &Option<String>,
    epoch: u64,
    team: &str,
    channel: &str,
    stale_owner: bool,
) {
    // Credential rotation must release this task's spinner even though its
    // exact authority no longer matches. Never release another account/task.
    if s.core.account_epoch != epoch
        || &s.core.active_account != account
        || s.read_presentation.unread_fetching.as_ref() != Some(ticket)
    {
        return;
    }
    s.read_presentation.unread_fetching = None;
    s.read_presentation.unread_paused = true;
    if stale_owner {
        s.read_presentation.unread_continuation = None;
    }
    if let Some(bag) = s
        .core
        .workspaces
        .get_mut(team)
        .and_then(|w| w.messages.get_mut(channel))
    {
        bag.history_refreshing = false;
    }
}

pub(super) async fn refresh(
    mut state: Signal<ShellState>,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    boundary: &str,
) {
    use crate::read_preferences::presentation::UnreadContinuation;
    let (scope, account, epoch) = {
        let s = state.peek();
        (
            s.timeline_scope(),
            s.core.active_account.clone(),
            s.core.account_epoch,
        )
    };
    let ticket = (scope.clone(), boundary.to_owned());
    let Some(owner) = CursorOwner::capture(&state.peek().core, transport, session).map(Arc::new)
    else {
        return;
    };
    let mut pages = {
        let mut s = state.write();
        if !current(&s, &owner, &scope, channel, boundary)
            || s.read_presentation.unread_fetching.as_ref() == Some(&ticket)
        {
            return;
        }
        let pages = s
            .read_presentation
            .unread_continuation
            .take()
            .filter(|c| c.scope == scope && c.boundary == boundary && c.owner.current(&s.core))
            .map(|c| c.pages)
            .unwrap_or_default();
        s.read_presentation.unread_fetching = Some(ticket.clone());
        s.read_presentation.unread_loading = Some(ticket.clone());
        s.read_presentation.unread_paused = false;
        if let Some(w) = s.core.workspaces.get_mut(&session.team_id) {
            w.messages
                .entry(channel.into())
                .or_default()
                .history_refreshing = true;
        }
        pages
    };
    let deadline = tokio::time::Instant::now() + TIME_BUDGET;
    for page_index in 0..PAGE_BUDGET {
        if !current(&state.peek(), &owner, &scope, channel, boundary) {
            release(
                &mut state.write(),
                &ticket,
                &account,
                epoch,
                &session.team_id,
                channel,
                true,
            );
            return;
        }
        state.write().read_presentation.unread_continuation = Some(UnreadContinuation {
            scope: scope.clone(),
            boundary: boundary.into(),
            pages: pages.clone(),
            owner: owner.clone(),
        });
        let request = api::HistoryArgs {
            channel: channel.into(),
            oldest: Some(boundary.into()),
            latest: pages.latest.clone(),
            cursor: pages.cursor.clone(),
            limit: Some(100),
            inclusive: false,
        };
        let file_read = state.write().core.begin_file_display_read(&session.team_id);
        let result = tokio::time::timeout_at(
            deadline,
            api::fetch_history(transport, client, session, request),
        )
        .await
        .unwrap_or_else(|_| Err(Error::Api("history_opening_timed_out".into())));
        if !current(&state.peek(), &owner, &scope, channel, boundary) {
            release(
                &mut state.write(),
                &ticket,
                &account,
                epoch,
                &session.team_id,
                channel,
                true,
            );
            return;
        }
        let (page, more, next, files) = match result.and_then(|page| {
            let files = state
                .read()
                .core
                .finish_file_display_read(&session.team_id, file_read)
                .map_err(|reason| Error::Api(reason.into()))?;
            if page.messages.len() > 100 {
                return Err(Error::Api("history_opening_page_too_large".into()));
            }
            let mut next = pages.clone();
            next.advance(&page, boundary)
                .map(|more| (page, more, next, files))
        }) {
            Ok(value) => value,
            Err(error) => {
                let mut s = state.write();
                release(
                    &mut s,
                    &ticket,
                    &account,
                    epoch,
                    &session.team_id,
                    channel,
                    false,
                );
                if let Some(bag) = s
                    .core
                    .workspaces
                    .get_mut(&session.team_id)
                    .and_then(|w| w.messages.get_mut(channel))
                {
                    bag.history_failed = true;
                }
                s.report_failure(&error,"",||"Could not finish loading the unread starting point. Continue loading to try again.".into());
                return;
            }
        };
        pages = next;
        let mut s = state.write();
        let upload_ids = crate::messaging::upload_recovery::incoming(
            &s,
            &session.team_id,
            channel,
            &page.messages,
        );
        if let Some(w) = s.core.workspaces.get_mut(&session.team_id) {
            let bag = w.messages.entry(channel.into()).or_default();
            for message in page.messages {
                let message = files.message(message);
                if super_platinum_core::state::is_channel_timeline_visible(&message) {
                    if message.ts.as_deref().is_some_and(|ts| {
                        bag.latest
                            .as_deref()
                            .is_none_or(|latest| cmp_ts(Some(ts), Some(latest)).is_gt())
                    }) {
                        bag.latest = message.ts.clone();
                    }
                    bag.upsert(message);
                }
            }
            bag.loaded = true;
            bag.history_failed = false;
            bag.history_refreshing = more;
            bag.has_more_older = true;
        }
        crate::messaging::upload_recovery::reconcile(
            &mut s,
            &session.team_id,
            channel,
            Some(&upload_ids),
        );
        if !more {
            s.read_presentation.unread_loading = None;
            s.read_presentation.unread_fetching = None;
            s.read_presentation.unread_continuation = None;
            s.refresh_from_core();
            if crate::read_preferences::opening::first_after(&s, boundary).is_none() {
                s.core.pending_scroll_to = Some((channel.into(), PendingScrollTarget::Latest));
                s.stick_to_bottom = true;
            }
            s.reset_timeline_window();
            return;
        }
        s.read_presentation.unread_continuation = Some(UnreadContinuation {
            scope: scope.clone(),
            boundary: boundary.into(),
            pages: pages.clone(),
            owner: owner.clone(),
        });
        if page_index + 1 == PAGE_BUDGET || tokio::time::Instant::now() >= deadline {
            release(
                &mut s,
                &ticket,
                &account,
                epoch,
                &session.team_id,
                channel,
                false,
            );
            s.refresh_from_core();
            return;
        }
        // Projection is rebuilt once at completion/pause, not for every page.
    }
}
