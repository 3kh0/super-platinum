//! Search navigation resolves unknown conversations before changing the visible one.
use crate::state::{MainView, Overlay, SearchResultVm, ShellState};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::slack::{Transport, api, models::GenericInfoPage};

struct OpenVisit {
    request: u64,
    epoch: u64,
    team: Option<String>,
    transport: Option<Arc<Transport>>,
    query: u64,
    channel: u64,
    thread: u64,
    view: MainView,
}
impl OpenVisit {
    fn begin(shell: &mut ShellState, hit: &SearchResultVm) -> Self {
        shell.cancel_search_open();
        shell.search_opening = Some((hit.channel_id.clone(), hit.ts.clone()));
        Self {
            request: shell.search_open_generation,
            epoch: shell.core.account_epoch,
            team: shell.core.active_team.clone(),
            transport: shell.core.transport.clone(),
            query: shell.search_pager.revision(),
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            view: shell.main_view,
        }
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_team == self.team
            && shell.search_open_generation == self.request
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    fn source_current(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell.overlay == Some(Overlay::Search)
            && shell.search_pager.revision() == self.query
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.main_view == self.view
    }
    fn resolve(
        &self,
        shell: &mut ShellState,
        target: &str,
        channel: super_platinum_core::slack::models::Channel,
        revision: u64,
    ) -> bool {
        if !self.source_current(shell) || channel.id != target {
            return false;
        }
        let Some(workspace) = self
            .team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get_mut(team))
        else {
            return false;
        };
        workspace.apply_channel_refresh_since(
            GenericInfoPage {
                channels: vec![channel],
                ..Default::default()
            },
            revision,
        );
        true
    }
    fn release(&self, shell: &mut ShellState) {
        if shell.search_open_generation == self.request {
            shell.search_opening = None;
        }
    }
}

pub fn open_search_hit(mut state: Signal<ShellState>, hit: SearchResultVm) {
    let visit = {
        let mut shell = state.write();
        if shell.overlay != Some(Overlay::Search)
            || !shell
                .search_results
                .iter()
                .any(|r| r.channel_id == hit.channel_id && r.ts == hit.ts)
            || shell.search_opening.as_ref() == Some(&(hit.channel_id.clone(), hit.ts.clone()))
        {
            return;
        }
        OpenVisit::begin(&mut shell, &hit)
    };
    dioxus::core::spawn_forever(async move {
        if !visit.source_current(&state.peek()) {
            visit.release(&mut state.write());
            return;
        }
        let Some(team) = visit.team.as_ref() else {
            visit.release(&mut state.write());
            return;
        };
        let known = state
            .peek()
            .core
            .workspaces
            .get(team)
            .is_some_and(|w| w.channels.contains_key(&hit.channel_id));
        if !known {
            let Some((transport, client, sessions)) = super::credentials(&state) else {
                visit.release(&mut state.write());
                state
                    .write()
                    .show_toast("Sign in to open this conversation.");
                return;
            };
            let Some(session) = sessions.into_iter().find(|s| &s.team_id == team) else {
                visit.release(&mut state.write());
                return;
            };
            let revision = state
                .peek()
                .core
                .workspaces
                .get(team)
                .map(|w| w.channel_lifecycle.revision())
                .unwrap_or(0);
            let result =
                api::fetch_conversation_info(&transport, &client, &session, hit.channel_id.clone())
                    .await;
            if !visit.source_current(&state.peek()) {
                visit.release(&mut state.write());
                return;
            }
            match result {
                Ok(channel) => {
                    if !visit.resolve(&mut state.write(), &hit.channel_id, channel, revision) {
                        visit.release(&mut state.write());
                        state
                            .write()
                            .show_toast("This conversation is unavailable.");
                        return;
                    }
                }
                Err(error) => {
                    let mut shell = state.write();
                    visit.release(&mut shell);
                    shell.report_failure(
                        &error,
                        "Connect to Slack to open this conversation.",
                        || {
                            "Could not open this conversation. It may no longer be accessible."
                                .into()
                        },
                    );
                    return;
                }
            }
        }
        if !visit.source_current(&state.peek()) {
            visit.release(&mut state.write());
            return;
        }
        let (generation, thread_generation) = {
            let mut shell = state.write();
            visit.release(&mut shell);
            if !shell
                .open_search_result(&hit.channel_id, hit.thread_ts.as_deref().unwrap_or(&hit.ts))
            {
                shell.show_toast("This conversation is unavailable.");
                return;
            }
            (shell.channel_generation, shell.thread_generation)
        };
        super::refresh_selected_channel(state).await;
        let current = {
            let shell = state.peek();
            visit.current(&shell)
                && shell.channel_generation == generation
                && shell.thread_generation == thread_generation
                && shell.core.active_channel.as_ref() == Some(&hit.channel_id)
                && shell.main_view == MainView::Home
        };
        if current {
            if let Some(root) = hit.thread_ts {
                super::open_thread_at(state, hit.channel_id, root, Some(hit.ts)).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture_shell() -> ShellState {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.overlay = Some(Overlay::Search);
        shell
    }
    fn hit() -> SearchResultVm {
        SearchResultVm {
            channel_id: "C_ARCHIVED".into(),
            channel_name: "archived".into(),
            ts: "100.1".into(),
            thread_ts: None,
            author: "Rowan".into(),
            text: "a result".into(),
        }
    }
    #[test]
    fn delayed_resolution_cannot_reopen_closed_search_even_after_reopening() {
        let mut shell = fixture_shell();
        let visit = OpenVisit::begin(&mut shell, &hit());
        assert!(visit.source_current(&shell));
        shell.dismiss_overlay();
        shell.overlay = Some(Overlay::Search);
        assert!(!visit.source_current(&shell));
        assert!(shell.search_opening.is_none());
    }
    #[test]
    fn newer_hit_query_sort_workspace_and_thread_cancel_resolution() {
        let mut shell = fixture_shell();
        let old = OpenVisit::begin(&mut shell, &hit());
        let current = OpenVisit::begin(&mut shell, &hit());
        old.release(&mut shell);
        assert!(shell.search_opening.is_some());
        assert!(!old.source_current(&shell));
        assert!(current.source_current(&shell));
        shell
            .search_pager
            .change_sort(super_platinum_core::search::SearchSort::Oldest);
        assert!(!current.source_current(&shell));
        let visit = OpenVisit::begin(&mut shell, &hit());
        shell.set_search_query("new query".into());
        assert!(!visit.source_current(&shell));
        let visit = OpenVisit::begin(&mut shell, &hit());
        shell.core.account_epoch += 1;
        assert!(!visit.source_current(&shell));
        let visit = OpenVisit::begin(&mut shell, &hit());
        shell.core.active_team = Some("OTHER".into());
        assert!(!visit.source_current(&shell));
        let visit = OpenVisit::begin(&mut shell, &hit());
        shell.close_thread();
        assert!(!visit.source_current(&shell));
    }
    #[test]
    fn cold_metadata_is_scoped_and_does_not_change_conversation_until_navigation() {
        let mut shell = fixture_shell();
        let old_channel = shell.core.active_channel.clone();
        let visit = OpenVisit::begin(&mut shell, &hit());
        let channel = super_platinum_core::slack::models::Channel {
            id: "C_ARCHIVED".into(),
            name: Some("archived".into()),
            is_archived: true,
            ..Default::default()
        };
        assert!(
            !shell.core.workspaces["T1"]
                .channels
                .contains_key("C_ARCHIVED")
        );
        assert!(!visit.resolve(&mut shell, "WRONG", channel.clone(), 0));
        assert!(visit.resolve(&mut shell, "C_ARCHIVED", channel.clone(), 0));
        assert_eq!(shell.core.active_channel, old_channel);
        assert!(shell.open_search_result("C_ARCHIVED", "100.1"));
        assert_eq!(shell.core.active_channel.as_deref(), Some("C_ARCHIVED"));
        let mut stale = fixture_shell();
        let visit = OpenVisit::begin(&mut stale, &hit());
        stale.dismiss_overlay();
        assert!(!visit.resolve(&mut stale, "C_ARCHIVED", channel, 0));
        assert!(
            !stale.core.workspaces["T1"]
                .channels
                .contains_key("C_ARCHIVED")
        );
    }

    #[test]
    fn cached_archived_search_target_opens_without_rebinding_outgoing_draft() {
        let mut shell = fixture_shell();
        let old_channel = shell.core.active_channel.clone().unwrap();
        shell.core.composer.text = "keep this draft".into();
        shell.checkpoint_composers();
        let workspace = shell.core.workspaces.get_mut("T1").unwrap();
        workspace.channels.insert(
            "C_ARCHIVED".into(),
            super_platinum_core::slack::models::Channel {
                id: "C_ARCHIVED".into(),
                name: Some("archived".into()),
                is_archived: true,
                ..Default::default()
            },
        );
        shell.refresh_from_core();
        assert!(!shell.channels.iter().any(|c| c.id == "C_ARCHIVED"));
        assert!(shell.open_search_result("C_ARCHIVED", "100.1"));
        assert_eq!(shell.core.active_channel.as_deref(), Some("C_ARCHIVED"));
        assert!(shell.composer_unavailable().is_some());
        assert!(shell.core.composer.text.is_empty());
        assert!(shell.open_search_result(&old_channel, "100.2"));
        assert_eq!(shell.core.composer.text, "keep this draft");
    }
    #[test]
    fn current_conversation_shortcut_is_scoped_and_leaves_the_draft_alone() {
        let mut shell = fixture_shell();
        shell.signed_in = true;
        shell.loading = false;
        let channel = shell.core.active_channel.clone().unwrap();
        shell.core.composer.text = "unsent text".into();
        shell.open_search(true);
        assert_eq!(shell.search_query, format!("in:<#{channel}> "));
        assert_eq!(shell.core.composer.text, "unsent text");
        assert_eq!(shell.overlay, Some(Overlay::Search));
        assert!(shell.search_focus_at_end);
        shell.open_search(false);
        assert!(!shell.search_focus_at_end);
    }
}
