//! Resolve saved-message metadata before choosing the channel or thread route.
use super::{Context, MainView, SavedKey, ShellState, api};
use dioxus::prelude::*;
use super_platinum_core::slack::models::{Message, MessagesListPage};

pub(super) struct OpenVisit {
    request: u64,
    channel: u64,
    list: u64,
    thread: u64,
    overlay: Option<crate::state::Overlay>,
    shortcut: u64,
}
impl OpenVisit {
    pub(super) fn begin(shell: &mut ShellState) -> Self {
        shell.later_open_generation = shell.later_open_generation.wrapping_add(1);
        Self {
            request: shell.later_open_generation,
            channel: shell.channel_generation,
            list: shell.core.later.generation,
            thread: shell.thread_generation,
            overlay: shell.overlay,
            shortcut: shell.shortcut_generation,
        }
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.main_view == MainView::Later
            && shell.later_open_generation == self.request
            && shell.channel_generation == self.channel
            && shell.core.later.generation == self.list
            && shell.thread_generation == self.thread
            && shell.overlay == self.overlay
            && shell.shortcut_generation == self.shortcut
    }
}

fn selected_message(mut page: MessagesListPage, key: &SavedKey) -> Option<Message> {
    page.messages_data
        .remove(&key.item_id)?
        .messages
        .into_iter()
        .find(|message| message.ts.as_ref() == Some(&key.ts))
}

pub fn open(mut state: Signal<ShellState>, key: SavedKey) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::Later(key.clone()),
    ) {
        return;
    }
    if key.item_type == "file" {
        super::files::open(state, key);
        return;
    }
    if key.item_type != "message" || state.peek().main_view != MainView::Later {
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    let visit = OpenVisit::begin(&mut state.write());
    dioxus::core::spawn_forever(async move {
        let current = |shell: &ShellState| {
            ctx.current(shell)
                && shell.core.active_team.as_ref() == Some(&ctx.session.team_id)
                && visit.current(shell)
        };
        if !current(&state.peek()) {
            return;
        }
        // A missing preview is unknown routing metadata, not proof that this
        // message belongs in the channel transcript. Resolve this one item even
        // if the background preview batch failed or has not reached it yet.
        let cached = state.peek().core.later.hydrated.get(&key).cloned();
        let message = if let Some(message) = cached {
            message
        } else {
            let hydration_revision = state.write().core.later.begin_hydration();
            let result = api::fetch_messages_list(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                vec![(key.item_id.clone(), vec![key.ts.clone()])],
            )
            .await;
            if !current(&state.peek()) {
                return;
            }
            match result {
                Ok(page) => {
                    let Some(message) = selected_message(page, &key) else {
                        state
                            .write()
                            .show_toast("This saved message is unavailable.");
                        return;
                    };
                    let mut shell = state.write();
                    shell.core.hydrate_later_message(
                        &ctx.session.team_id,
                        key.clone(),
                        message,
                        hydration_revision,
                    );
                    shell.core.later.hydrated[&key].clone()
                }
                Err(error) => {
                    state.write().report_failure(
                        &error,
                        "Connect to Slack to open this saved message.",
                        || "Could not open this saved message. Please try again.".into(),
                    );
                    return;
                }
            }
        };
        let known_channel = state
            .peek()
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .is_some_and(|workspace| workspace.channels.contains_key(&key.item_id));
        if !known_channel {
            let posting =
                crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session);
            let result = api::fetch_channels_info(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                vec![key.item_id.clone()],
            )
            .await;
            if !current(&state.peek()) {
                return;
            }
            match result {
                Ok(channels) => {
                    let Some(channel) = channels.into_iter().find(|c| c.id == key.item_id) else {
                        state
                            .write()
                            .show_toast("This conversation is unavailable.");
                        return;
                    };
                    posting.channels(&state, std::slice::from_ref(&channel));
                    if let Some(workspace) =
                        state.write().core.workspaces.get_mut(&ctx.session.team_id)
                    {
                        workspace.insert_channel_metadata(channel);
                    }
                }
                Err(error) => {
                    state.write().report_failure(
                        &error,
                        "Connect to Slack to open this conversation.",
                        || "Could not open this conversation. Please try again.".into(),
                    );
                    return;
                }
            }
        }
        let root = message.thread_ts.filter(|root| root != &key.ts);
        let (generation, thread_generation) = {
            let mut shell = state.write();
            shell.core.active_channel = Some(key.item_id.clone());
            shell.refresh_from_core();
            if !shell.open_later_item(&key.item_id, root.as_deref().unwrap_or(&key.ts)) {
                shell.show_toast("This conversation is unavailable.");
                return;
            }
            (shell.channel_generation, shell.thread_generation)
        };
        crate::bootstrap::refresh_selected_channel(state).await;
        {
            let shell = state.peek();
            if !ctx.current(&shell)
                || shell.core.active_team.as_ref() != Some(&ctx.session.team_id)
                || shell.core.active_channel.as_ref() != Some(&key.item_id)
                || shell.channel_generation != generation
                || shell.thread_generation != thread_generation
                || shell.later_open_generation != visit.request
                || shell.main_view != MainView::Later
            {
                return;
            }
        }
        if let Some(root) = root {
            crate::bootstrap::open_thread_at(state, key.item_id, root, Some(key.ts)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_saved_reply_preserves_root_and_rejects_other_channel_or_timestamp() {
        let page: MessagesListPage = serde_json::from_value(serde_json::json!({
            "messages_data": {
                "C1": {"messages": [
                    {"ts":"100.1", "text":"root"},
                    {"ts":"101.1", "thread_ts":"100.1", "text":"saved reply"}
                ]},
                "C2": {"messages": [{"ts":"101.1", "thread_ts":"99.1"}]}
            }
        }))
        .unwrap();
        let reply = selected_message(page.clone(), &SavedKey::message("C1", "101.1")).unwrap();
        assert_eq!(reply.thread_ts.as_deref(), Some("100.1"));
        assert!(selected_message(page.clone(), &SavedKey::message("C3", "101.1")).is_none());
        assert!(selected_message(page, &SavedKey::message("C1", "102.1")).is_none());
    }

    #[test]
    fn delayed_metadata_cannot_reopen_replaced_saved_item_or_surface() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.main_view = MainView::Later;
        let first = OpenVisit::begin(&mut shell);
        assert!(first.current(&shell));
        let second = OpenVisit::begin(&mut shell);
        assert!(!first.current(&shell));
        assert!(second.current(&shell));
        shell.channel_generation += 1;
        assert!(!second.current(&shell));
        let visit = OpenVisit::begin(&mut shell);
        shell.core.later.generation += 1;
        assert!(!visit.current(&shell));
        let visit = OpenVisit::begin(&mut shell);
        shell.close_thread();
        assert!(!visit.current(&shell));
        let visit = OpenVisit::begin(&mut shell);
        shell.main_view = MainView::Home;
        assert!(!visit.current(&shell));
    }
}

#[cfg(test)]
mod canvas_departure_tests {
    use super::*;
    #[test]
    fn pending_metadata_does_not_cross_a_new_canvas_visit_or_overlay_aba() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.main_view = MainView::Later;
        shell.overlay = None;
        let visit = OpenVisit::begin(&mut shell);
        assert!(visit.current(&shell));
        shell.overlay = Some(crate::state::Overlay::CanvasCreate);
        assert!(!visit.current(&shell));
        shell.shortcut_generation += 1;
        shell.overlay = None;
        assert!(!visit.current(&shell));
    }
}
