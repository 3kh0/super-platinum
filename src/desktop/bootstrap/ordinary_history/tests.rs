use super::*;
use crate::realtime::activity_counts::CountsOwner;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    domain::PendingScrollTarget,
    slack::Transport,
};
fn state() -> (ShellState, String, String) {
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
    s.core.active_account = Some("isolated-history-owner".into());
    s.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "fixture-cookie".into(),
        workspaces: [(team.clone(), session.clone())].into(),
    });
    let transport = Arc::new(Transport::new("fixture-cookie").unwrap());
    s.core.transport = Some(transport.clone());
    let owner = Arc::new(context::Owner::Live(
        CountsOwner::capture(&s.core, &transport, &session).unwrap(),
    ));
    s.ordinary_history.entries.insert(
        (team.clone(), channel.clone()),
        Entry {
            owner,
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            scope: s.timeline_scope(),
            ticket: 7,
            pages: Pages::new(HistoryArgs {
                channel: channel.clone(),
                ..Default::default()
            }),
            loading: true,
            error: false,
            paused: false,
            restart_required: false,
            initial: false,
        },
    );
    s.loading_older = true;
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut(&channel)
        .unwrap()
        .history_refreshing = true;
    (s, team, channel)
}
#[test]
fn ordinary_owner_survives_navigation_and_socket_reconnect_but_not_credentials() {
    let (mut s, team, channel) = state();
    s.channel_generation += 1;
    s.reconnect_generation += 1;
    s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    assert!(ticket_current(&s, &team, &channel, 7));
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token = "replacement".into();
    assert!(!ticket_current(&s, &team, &channel, 7));
}
#[test]
fn retired_credentials_release_exact_ticket_without_touching_draft_or_cache() {
    let (mut s, team, channel) = state();
    let cache = format!("{:?}", s.core.workspaces[&team].messages[&channel].messages);
    let draft = s.core.composer.clone();
    s.core.session.as_mut().unwrap().d_cookie = "replacement".into();
    assert!(!ticket_current(&s, &team, &channel, 7));
    release(&mut s, &team, &channel, 7);
    assert!(!s.loading_older);
    assert!(!s.ordinary_history.entries[&(team.clone(), channel.clone())].loading);
    assert!(!s.core.workspaces[&team].messages[&channel].history_refreshing);
    assert_eq!(
        cache,
        format!("{:?}", s.core.workspaces[&team].messages[&channel].messages)
    );
    assert_eq!(draft, s.core.composer);
}
#[test]
fn late_release_cannot_clear_newer_ticket_account_or_view_spinner() {
    let (mut s, team, channel) = state();
    s.ordinary_history
        .entries
        .get_mut(&(team.clone(), channel.clone()))
        .unwrap()
        .ticket = 8;
    release(&mut s, &team, &channel, 7);
    assert!(s.loading_older);
    assert!(s.core.workspaces[&team].messages[&channel].history_refreshing);
    s.core.account_epoch += 1;
    release(&mut s, &team, &channel, 8);
    assert!(s.loading_older);
    s.core.account_epoch -= 1;
    s.channel_generation += 1;
    release(&mut s, &team, &channel, 8);
    assert!(s.loading_older);
    assert!(!s.core.workspaces[&team].messages[&channel].history_refreshing);
}
#[test]
fn unread_opening_excludes_ordinary_automatic_and_retry_admission() {
    let (mut s, team, channel) = state();
    s.ordinary_history
        .entries
        .get_mut(&(team.clone(), channel.clone()))
        .unwrap()
        .loading = false;
    s.core.pending_scroll_to = None;
    assert!(automatic(&s, &team, &channel));
    s.core.pending_scroll_to = Some((
        channel.clone(),
        PendingScrollTarget::FirstUnreadAfter("10.000000".into()),
    ));
    assert!(unread_owned(&s));
    assert!(!automatic(&s, &team, &channel));
    s.core.pending_scroll_to = None;
    s.read_presentation.unread_loading = Some((s.timeline_scope(), "10.000000".into()));
    assert!(unread_owned(&s));
}
