//! Profile requests belong to a specific account and pane visit.
use crate::state::{ChannelOpen, MainView, Overlay, ShellState};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::config::WorkspaceSession;
use super_platinum_core::slack::{Transport, api};

pub(super) struct ProfileVisit {
    account: Option<String>,
    epoch: u64,
    team: String,
    self_user: Option<String>,
    session: Option<WorkspaceSession>,
    transport: Option<Arc<Transport>>,
    generation: u64,
    user: Option<String>,
}

impl ProfileVisit {
    pub(super) fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        Some(Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            self_user: shell
                .core
                .workspaces
                .get(&team)
                .map(|workspace| workspace.self_user_id.clone()),
            session: shell
                .core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&team))
                .cloned(),
            team,
            transport: shell.core.transport.clone(),
            generation: shell.profile_generation,
            user: shell.profile_user.clone(),
        })
    }
    fn owner_current(&self, shell: &ShellState) -> bool {
        shell.core.active_account == self.account
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell
                .core
                .workspaces
                .get(&self.team)
                .map(|workspace| &workspace.self_user_id)
                == self.self_user.as_ref()
            && match (
                &self.session,
                shell
                    .core
                    .session
                    .as_ref()
                    .and_then(|session| session.workspaces.get(&self.team)),
            ) {
                (Some(expected), Some(current)) => {
                    expected.team_id == current.team_id && expected.user_id == current.user_id
                }
                (None, None) => true,
                _ => false,
            }
    }
    fn account_current(&self, shell: &ShellState) -> bool {
        self.owner_current(shell)
            && match (
                &self.session,
                shell
                    .core
                    .session
                    .as_ref()
                    .and_then(|session| session.workspaces.get(&self.team)),
            ) {
                (Some(expected), Some(current)) => {
                    expected.team_id == current.team_id
                        && expected.user_id == current.user_id
                        && expected.token == current.token
                        && expected.url == current.url
                        && expected.enterprise_id == current.enterprise_id
                }
                (None, None) => true,
                _ => false,
            }
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    pub(super) fn unavailable(&self, shell: &mut ShellState) {
        if self.current(shell)
            && let Some(pane) = shell.core.profile_pane.as_mut()
        {
            pane.loading = false;
            pane.error = Some("Sign in to refresh these profile details.".into());
        }
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.account_current(shell)
            && shell.profile_generation == self.generation
            && shell.profile_user == self.user
    }

    /// A credential refresh invalidates data, but must not leave this same
    /// account's unchanged pane permanently showing a loading indicator.
    pub(super) fn retire(&self, shell: &mut ShellState) {
        if self.owner_current(shell)
            && shell.profile_generation == self.generation
            && shell.profile_user == self.user
            && let Some(pane) = shell.core.profile_pane.as_mut()
            && Some(&pane.user) == self.user.as_ref()
        {
            pane.loading = false;
            pane.error = Some("Open this profile again to refresh its details.".into());
        }
    }
}

struct DmVisit {
    profile: ProfileVisit,
    request: u64,
    user: String,
    channel: u64,
    thread: u64,
    view: MainView,
    overlay: Option<Overlay>,
    palette: u64,
    search: u64,
}

impl DmVisit {
    fn begin(shell: &mut ShellState, user: String) -> Option<Self> {
        if shell.profile_dm_opening.as_ref() == Some(&user)
            && shell.profile_dm_link_generation.is_none()
        {
            return None;
        }
        let profile = ProfileVisit::capture(shell)?;
        if !shell
            .core
            .workspaces
            .get(&profile.team)?
            .users
            .contains_key(&user)
        {
            return None;
        }
        shell.cancel_profile_dm();
        shell.profile_dm_opening = Some(user.clone());
        Some(Self {
            profile,
            request: shell.profile_dm_generation,
            user,
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            view: shell.main_view,
            overlay: shell.overlay,
            palette: shell.palette_generation,
            search: shell.search_open_generation,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.profile.current(shell)
            && shell.profile_dm_generation == self.request
            && shell.profile_dm_opening.as_ref() == Some(&self.user)
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.main_view == self.view
            && shell.overlay == self.overlay
            && shell.palette_generation == self.palette
            && shell.search_open_generation == self.search
    }
    fn release(&self, shell: &mut ShellState) {
        if self.profile.owner_current(shell) && shell.profile_dm_generation == self.request {
            shell.profile_dm_opening = None;
            shell.profile_dm_link_generation = None;
        }
    }
    fn finish(&self, shell: &mut ShellState, id: String) -> bool {
        if !self.current(shell) || id.is_empty() {
            return false;
        }
        let Some(workspace) = shell.core.workspaces.get_mut(&self.profile.team) else {
            return false;
        };
        workspace.channels.entry(id.clone()).or_insert_with(|| {
            super_platinum_core::slack::models::Channel {
                id: id.clone(),
                is_im: true,
                user: Some(self.user.clone()),
                ..Default::default()
            }
        });
        shell.refresh_from_core();
        let Some(index) = shell.channels.iter().position(|c| c.id == id) else {
            return false;
        };
        shell.select_channel(index, ChannelOpen::Global);
        shell.close_profile();
        shell.profile_hover = None;
        true
    }
}

/// Capture intent in the click handler, before yielding to native Slack work.
pub fn open_profile_dm(mut state: Signal<ShellState>, user: String) {
    let existing = state
        .peek()
        .channels
        .iter()
        .position(|channel| channel.is_im && channel.user_id.as_deref() == Some(user.as_str()));
    if let Some(index) = existing {
        let mut shell = state.write();
        shell.select_channel(index, ChannelOpen::Global);
        shell.close_profile();
        shell.profile_hover = None;
        drop(shell);
        spawn(super::refresh_selected_channel(state));
        return;
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        state
            .write()
            .show_toast("No direct message channel is loaded yet.");
        return;
    }
    let Some(visit) = DmVisit::begin(&mut state.write(), user) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !visit.current(&state.peek()) {
            visit.release(&mut state.write());
            return;
        }
        let credentials = super::credentials(&state).and_then(|(transport, client, sessions)| {
            sessions
                .into_iter()
                .find(|s| s.team_id == visit.profile.team)
                .map(|session| (transport, client, session))
        });
        let Some((transport, client, session)) = credentials else {
            let mut shell = state.write();
            visit.release(&mut shell);
            shell.show_toast("Sign in to open this direct message.");
            return;
        };
        let result = api::open_dm(&transport, &client, &session, visit.user.clone()).await;
        if !visit.current(&state.peek()) {
            visit.release(&mut state.write());
            return;
        }
        match result {
            Ok(id) => {
                let opened = visit.finish(&mut state.write(), id);
                if opened {
                    super::refresh_selected_channel(state).await;
                } else {
                    let mut shell = state.write();
                    visit.release(&mut shell);
                    shell.show_toast(
                        "Slack opened the direct message, but it is not available yet.",
                    );
                }
            }
            Err(error) => {
                let mut shell = state.write();
                visit.release(&mut shell);
                shell.report_failure(
                    &error,
                    "Connect to Slack to open this direct message.",
                    || "Could not open this direct message. Try again.".into(),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> ShellState {
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "profile-pane")
    }
    #[test]
    fn profile_refresh_is_scoped_to_account_transport_and_pane_visit() {
        let mutations: [fn(&mut ShellState); 5] = [
            |s| s.core.account_epoch += 1,
            |s| s.core.active_team = Some("OTHER".into()),
            |s| s.core.transport = Some(Arc::new(Transport::new("fixture").unwrap())),
            |s| s.profile_user = Some("U2".into()),
            |s| {
                s.close_profile();
                s.profile_user = Some("U1".into());
            },
        ];
        for mutate in mutations {
            let mut shell = shell();
            let visit = ProfileVisit::capture(&shell).unwrap();
            assert!(visit.current(&shell));
            mutate(&mut shell);
            assert!(!visit.current(&shell));
        }
    }
    #[test]
    fn renewed_transport_invalidates_same_account_profile_and_dm_requests() {
        let mut shell = shell();
        shell.core.transport = Some(Arc::new(Transport::new("fixture").unwrap()));
        let profile = ProfileVisit::capture(&shell).unwrap();
        let dm = DmVisit::begin(&mut shell, "U1".into()).unwrap();
        assert!(profile.current(&shell));
        assert!(dm.current(&shell));
        shell.core.transport = Some(Arc::new(Transport::new("fixture").unwrap()));
        assert!(!profile.current(&shell));
        assert!(!dm.finish(&mut shell, "D_STALE_TRANSPORT".into()));
        assert!(
            !shell.core.workspaces["T1"]
                .channels
                .contains_key("D_STALE_TRANSPORT")
        );
    }
    #[test]
    fn workspace_round_trip_retires_the_old_profile_refresh() {
        let mut shell = shell();
        let mut workspace = shell.core.workspaces["T1"].clone();
        workspace.team_id = "OTHER".into();
        shell.core.workspaces.insert("OTHER".into(), workspace);
        shell.workspaces.push(crate::state::WorkspaceVm {
            id: "OTHER".into(),
            name: "Other".into(),
            initials: "O".into(),
        });
        let visit = ProfileVisit::capture(&shell).unwrap();
        let original = shell.active_workspace;
        assert!(shell.select_workspace(shell.workspaces.len() - 1));
        assert!(shell.profile_user.is_none());
        assert!(shell.select_workspace(original));
        shell.profile_user = Some("U1".into());
        assert!(!visit.current(&shell));
    }
    #[test]
    fn missing_credentials_finish_only_the_current_profile_loading_state() {
        let mut shell = shell();
        shell.core.profile_pane.as_mut().unwrap().loading = true;
        let old = ProfileVisit::capture(&shell).unwrap();
        old.unavailable(&mut shell);
        assert!(!shell.core.profile_pane.as_ref().unwrap().loading);
        assert!(shell.core.profile_pane.as_ref().unwrap().error.is_some());
        shell.close_profile();
        shell.profile_user = Some("U1".into());
        shell.core.profile_pane = Some(super_platinum_core::domain::ProfilePaneState {
            user: "U1".into(),
            loading: true,
            error: None,
        });
        old.unavailable(&mut shell);
        assert!(shell.core.profile_pane.as_ref().unwrap().loading);
        assert!(shell.core.profile_pane.as_ref().unwrap().error.is_none());
    }
    #[test]
    fn dm_completion_opens_home_closes_profile_and_preserves_outgoing_draft() {
        let mut shell = shell();
        let old = shell.core.active_channel.clone().unwrap();
        shell.core.composer = super_platinum_core::ComposerState::with_text("Unsent profile draft");
        let visit = DmVisit::begin(&mut shell, "U1".into()).unwrap();
        assert!(visit.finish(&mut shell, "D_PROFILE_NEW".into()));
        assert_eq!(shell.core.active_channel.as_deref(), Some("D_PROFILE_NEW"));
        assert_eq!(shell.main_view, MainView::Home);
        assert!(shell.profile_user.is_none());
        assert!(shell.profile_dm_opening.is_none());
        assert!(shell.core.composer.text.is_empty());
        let index = shell.channels.iter().position(|c| c.id == old).unwrap();
        shell.select_channel(index, ChannelOpen::Global);
        assert_eq!(shell.core.composer.text, "Unsent profile draft");
    }
    #[test]
    fn stale_dm_completion_cannot_insert_or_navigate_after_leaving() {
        let mutations: [fn(&mut ShellState); 10] = [
            |s| s.core.account_epoch += 1,
            |s| s.core.active_team = Some("OTHER".into()),
            |s| s.core.transport = Some(Arc::new(Transport::new("fixture").unwrap())),
            |s| {
                s.close_profile();
                s.profile_user = Some("U1".into());
            },
            |s| s.select_channel(s.active_channel, ChannelOpen::InSurface),
            |s| s.close_thread(),
            |s| s.main_view = MainView::Dms,
            |s| {
                s.open_palette();
                s.dismiss_overlay();
            },
            |s| {
                s.open_search(false);
                s.dismiss_overlay();
            },
            |s| {
                s.open_preferences();
                s.dismiss_overlay();
            },
        ];
        for mutate in mutations {
            let mut shell = shell();
            let visit = DmVisit::begin(&mut shell, "U1".into()).unwrap();
            mutate(&mut shell);
            let active = shell.core.active_channel.clone();
            assert!(!visit.finish(&mut shell, "D_STALE".into()));
            assert!(!shell.core.workspaces["T1"].channels.contains_key("D_STALE"));
            assert_eq!(shell.core.active_channel, active);
        }
    }
    #[test]
    fn duplicate_open_is_ignored_and_old_cleanup_cannot_clear_a_replacement() {
        let mut shell = shell();
        let old = DmVisit::begin(&mut shell, "U1".into()).unwrap();
        assert!(DmVisit::begin(&mut shell, "U1".into()).is_none());
        let current = DmVisit::begin(&mut shell, "U2".into()).unwrap();
        old.release(&mut shell);
        assert!(!old.current(&shell));
        assert!(current.current(&shell));
        assert_eq!(shell.profile_dm_opening.as_deref(), Some("U2"));
        current.release(&mut shell);
        assert!(shell.profile_dm_opening.is_none());
    }

    #[test]
    fn message_supersedes_a_user_links_pending_intent_and_still_closes_profile() {
        let mut shell = shell();
        shell.cancel_profile_dm();
        shell.profile_dm_opening = Some("U1".into());
        shell.profile_dm_link_generation = Some(shell.slack_link_generation);
        let link_request = shell.profile_dm_generation;
        let message = DmVisit::begin(&mut shell, "U1".into()).unwrap();
        assert_ne!(shell.profile_dm_generation, link_request);
        assert!(shell.profile_dm_link_generation.is_none());
        assert!(message.finish(&mut shell, "D_PROFILE_NEW".into()));
        assert!(shell.profile_user.is_none());
    }

    #[test]
    fn profile_data_is_invalid_after_account_or_same_transport_session_replacement() {
        for change in 0..8 {
            let mut shell = shell();
            shell.core.active_account = Some("account-one".into());
            let session = WorkspaceSession {
                team_id: "T1".into(),
                user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                token: "synthetic-only".into(),
                url: "https://fixture.slack.com".into(),
                enterprise_id: None,
                name: "Fixture".into(),
            };
            shell.core.session = Some(super_platinum_core::config::Session {
                unified_user_client_enabled: Default::default(),
                d_cookie: "synthetic-only".into(),
                workspaces: [("T1".into(), session)].into(),
            });
            let visit = ProfileVisit::capture(&shell).unwrap();
            assert!(visit.current(&shell));
            match change {
                0 => shell.core.active_account = Some("account-two".into()),
                1 => {
                    shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .token = "rotated".into()
                }
                2 => {
                    shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .url = "https://other.slack.com".into()
                }
                3 => {
                    shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .enterprise_id = Some("EOTHER".into())
                }
                4 => {
                    shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .user_id = "OTHER".into()
                }
                5 => {
                    shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .team_id = "OTHER".into()
                }
                6 => shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "OTHER".into(),
                _ => shell.core.session = None,
            }
            assert!(!visit.current(&shell));
            if matches!(change, 0 | 4 | 5 | 6 | 7) {
                shell.core.profile_pane.as_mut().unwrap().loading = true;
                visit.retire(&mut shell);
                assert!(shell.core.profile_pane.as_ref().unwrap().loading);
            }
        }
    }

    #[test]
    fn retired_profile_cleanup_clears_only_the_same_owners_unchanged_pane() {
        let mut shell = shell();
        let old = ProfileVisit::capture(&shell).unwrap();
        shell.core.transport = Some(Arc::new(Transport::new("synthetic-only").unwrap()));
        old.retire(&mut shell);
        assert!(!shell.core.profile_pane.as_ref().unwrap().loading);
        assert!(shell.core.profile_pane.as_ref().unwrap().error.is_some());
        shell.core.profile_pane.as_mut().unwrap().loading = true;
        shell.core.active_account = Some("replacement".into());
        old.retire(&mut shell);
        assert!(shell.core.profile_pane.as_ref().unwrap().loading);
        shell.core.active_account = None;
        shell.profile_generation += 1;
        old.retire(&mut shell);
        assert!(shell.core.profile_pane.as_ref().unwrap().loading);
    }
}
