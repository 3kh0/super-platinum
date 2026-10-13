use crate::state::{ChannelOpen, Overlay, ShellState};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    palette::PaletteTarget,
    slack::{Transport, api},
};

struct Visit {
    request: u64,
    epoch: u64,
    team: String,
    transport: Option<Arc<Transport>>,
    channel: u64,
    thread: u64,
    view: crate::state::MainView,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> ShellState {
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "palette-people")
    }
    #[test]
    fn person_without_dm_is_visible_without_mutating_conversations() {
        let mut shell = shell();
        let channels = shell.channels.len();
        let hits = shell.palette_matches();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].row.name, "Avery Park");
        assert_eq!(hits[0].handle, "@avery.park");
        assert_eq!(
            hits[0].target,
            PaletteTarget::User {
                user: "U_PALETTE_NEW".into(),
                dm: None
            }
        );
        assert_eq!(shell.channels.len(), channels);
        shell.move_palette(-1);
        assert_eq!(shell.palette_selected, 0);
    }
    #[test]
    fn new_dm_completion_opens_home_and_preserves_the_outgoing_draft() {
        let mut shell = shell();
        let old = shell.core.active_channel.clone().unwrap();
        shell.core.composer = super_platinum_core::ComposerState::with_text("Unsent draft");
        let visit = Visit::begin(&mut shell, "U_PALETTE_NEW").unwrap();
        assert!(visit.finish(&mut shell, "U_PALETTE_NEW", "D_NEW".into()));
        assert_eq!(shell.core.active_channel.as_deref(), Some("D_NEW"));
        assert_eq!(shell.main_view, crate::state::MainView::Home);
        assert!(shell.overlay.is_none());
        assert!(shell.palette_opening.is_none());
        assert!(shell.core.composer.text.is_empty());
        let index = shell.channels.iter().position(|c| c.id == old).unwrap();
        shell.select_channel(index, ChannelOpen::Global);
        assert_eq!(shell.core.composer.text, "Unsent draft");
    }
    #[test]
    fn stale_dm_completion_cannot_navigate_or_insert_metadata() {
        let mutations: [fn(&mut ShellState); 6] = [
            |s| {
                s.dismiss_overlay();
                s.open_palette();
            },
            |s| {
                s.set_palette_query("other".into());
                s.set_palette_query("avery".into());
            },
            |s| s.core.account_epoch += 1,
            |s| s.core.active_team = Some("T_OTHER".into()),
            |s| s.channel_generation += 1,
            |s| s.thread_generation += 1,
        ];
        for mutate in mutations {
            let mut shell = shell();
            let visit = Visit::begin(&mut shell, "U_PALETTE_NEW").unwrap();
            mutate(&mut shell);
            assert!(!visit.finish(&mut shell, "U_PALETTE_NEW", "D_STALE".into()));
            assert!(!shell.core.workspaces["T1"].channels.contains_key("D_STALE"));
        }
    }
    #[test]
    fn old_request_cleanup_does_not_clear_the_new_selection() {
        let mut shell = shell();
        let old = Visit::begin(&mut shell, "U_OLD").unwrap();
        let current = Visit::begin(&mut shell, "U_NEW").unwrap();
        old.release(&mut shell);
        assert_eq!(shell.palette_opening.as_deref(), Some("U_NEW"));
        assert!(!old.current(&shell));
        assert!(current.current(&shell));
    }
}
impl Visit {
    fn begin(shell: &mut ShellState, user: &str) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        shell.cancel_palette_open();
        shell.palette_opening = Some(user.to_owned());
        Some(Self {
            request: shell.palette_generation,
            epoch: shell.core.account_epoch,
            team,
            transport: shell.core.transport.clone(),
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            view: shell.main_view,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.palette_generation == self.request
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.overlay == Some(Overlay::Palette)
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.main_view == self.view
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    fn release(&self, shell: &mut ShellState) {
        if shell.palette_generation == self.request {
            shell.palette_opening = None;
        }
    }
    fn finish(&self, shell: &mut ShellState, user: &str, id: String) -> bool {
        if !self.current(shell) || id.is_empty() {
            return false;
        }
        let Some(workspace) = shell.core.workspaces.get_mut(&self.team) else {
            return false;
        };
        workspace.channels.entry(id.clone()).or_insert_with(|| {
            super_platinum_core::slack::models::Channel {
                id: id.clone(),
                is_im: true,
                user: Some(user.into()),
                ..Default::default()
            }
        });
        shell.refresh_from_core();
        let Some(index) = shell.channels.iter().position(|channel| channel.id == id) else {
            return false;
        };
        shell.select_channel(index, ChannelOpen::Global);
        shell.dismiss_overlay();
        true
    }
}

/// A single action path for row clicks, Enter, and the agent's selection command.
pub(crate) fn open_palette_entry(state: Signal<ShellState>, index: Option<usize>) -> bool {
    let entry = {
        let shell = state.peek();
        if shell.overlay != Some(Overlay::Palette) {
            return false;
        }
        let Some(entry) = shell
            .palette_matches()
            .get(index.unwrap_or(shell.palette_selected))
            .cloned()
        else {
            return false;
        };
        entry
    };
    open_palette_target(state, entry.target)
}

pub(crate) fn open_palette_target(mut state: Signal<ShellState>, target: PaletteTarget) -> bool {
    if state.peek().overlay != Some(Overlay::Palette) {
        return false;
    }
    let existing = {
        let shell = state.peek();
        shell.channels.iter().position(|channel| match &target {
            PaletteTarget::Channel(id) => &channel.id == id,
            PaletteTarget::User { user, dm } => {
                dm.as_ref() == Some(&channel.id)
                    || (channel.is_im && channel.user_id.as_ref() == Some(user))
            }
        })
    };
    if let Some(index) = existing {
        state.write().select_channel(index, ChannelOpen::Global);
        state.write().dismiss_overlay();
        spawn(super::refresh_selected_channel(state));
        return true;
    }
    let PaletteTarget::User { user, .. } = target else {
        return false;
    };
    if !state
        .peek()
        .core
        .active_team
        .as_ref()
        .and_then(|team| {
            state
                .peek()
                .core
                .workspaces
                .get(team)
                .map(|w| w.users.contains_key(&user))
        })
        .unwrap_or(false)
    {
        return false;
    }
    if state.peek().palette_opening.as_ref() == Some(&user) {
        return true;
    }
    let Some(visit) = Visit::begin(&mut state.write(), &user) else {
        return false;
    };
    dioxus::core::spawn_forever(async move {
        if !visit.current(&state.peek()) {
            visit.release(&mut state.write());
            return;
        }
        let credentials = super::credentials(&state).and_then(|(transport, client, sessions)| {
            sessions
                .into_iter()
                .find(|s| s.team_id == visit.team)
                .map(|session| (transport, client, session))
        });
        let Some((transport, client, session)) = credentials else {
            let mut shell = state.write();
            visit.release(&mut shell);
            shell.show_toast("Sign in to open this direct message.");
            return;
        };
        let result = api::open_dm(&transport, &client, &session, user.clone()).await;
        if !visit.current(&state.peek()) {
            visit.release(&mut state.write());
            return;
        }
        match result {
            Ok(id) => {
                let opened = visit.finish(&mut state.write(), &user, id);
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
    true
}
