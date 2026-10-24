//! Resolve saved-message metadata before choosing the channel or thread route.
use super::{MainView, SavedKey, ShellState};
mod context;
use context::Context;
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

// The route itself advances channel/thread generations. Retain the list/request
// owner while binding the new destination before awaiting its history.
struct Destination {
    request: u64,
    list: u64,
    channel: u64,
    thread: u64,
    overlay: Option<crate::state::Overlay>,
    shortcut: u64,
}
impl Destination {
    fn capture(s: &ShellState, v: &OpenVisit) -> Self {
        Self {
            request: v.request,
            list: v.list,
            channel: s.channel_generation,
            thread: s.thread_generation,
            overlay: s.overlay,
            shortcut: v.shortcut,
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        s.main_view == MainView::Later
            && s.later_open_generation == self.request
            && s.core.later.generation == self.list
            && s.channel_generation == self.channel
            && s.thread_generation == self.thread
            && s.overlay == self.overlay
            && s.shortcut_generation == self.shortcut
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
                && shell.core.active_team.as_deref() == Some(ctx.team())
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
            let (request, result) = ctx.messages(&key).await;
            let owned = current(&state.peek());
            ctx.accepted(request, owned);
            if !owned {
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
                        ctx.team(),
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
            .get(ctx.team())
            .is_some_and(|workspace| workspace.channels.contains_key(&key.item_id));
        if !known_channel {
            let posting = ctx.posting(&state);
            let (request, result) = ctx.channels(&key).await;
            let owned = current(&state.peek());
            ctx.accepted(request, owned);
            if !owned {
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
                    if let Some(posting) = posting {
                        posting.channels(&state, std::slice::from_ref(&channel));
                    }
                    if let Some(workspace) = state.write().core.workspaces.get_mut(ctx.team()) {
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
        let destination = {
            let mut shell = state.write();
            shell.core.active_channel = Some(key.item_id.clone());
            shell.refresh_from_core();
            if !shell.open_later_item(&key.item_id, root.as_deref().unwrap_or(&key.ts)) {
                shell.show_toast("This conversation is unavailable.");
                return;
            }
            Destination::capture(&shell, &visit)
        };
        crate::bootstrap::refresh_selected_channel(state).await;
        let history_request = ctx.history_barrier(&key).await;
        let owned = {
            let shell = state.peek();
            ctx.current(&shell)
                && shell.core.active_team.as_deref() == Some(ctx.team())
                && shell.core.active_channel.as_ref() == Some(&key.item_id)
                && destination.current(&shell)
        };
        ctx.accepted(history_request, owned);
        if !owned {
            return;
        }
        if let Some(root) = root {
            crate::bootstrap::open_thread_at(
                state,
                key.item_id.clone(),
                root,
                Some(key.ts.clone()),
            )
            .await;
        }
        if super::message_fixture::enabled(&state.peek()) {
            super::message_fixture::completed(&state.peek(), &key);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_continuation_retains_list_and_shortcut_owner_after_its_own_open() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.main_view = MainView::Later;
        s.overlay = Some(crate::state::Overlay::CanvasCreate);
        let visit = OpenVisit::begin(&mut s);
        // Our admitted route intentionally advances these and clears its overlay.
        s.channel_generation += 1;
        s.thread_generation += 1;
        s.overlay = None;
        let destination = Destination::capture(&s, &visit);
        assert!(!visit.current(&s));
        assert!(destination.current(&s));
        s.core
            .later
            .begin(super_platinum_core::later::LaterFilter::Completed, false);
        assert!(!destination.current(&s));
        s.core.later.generation -= 1;
        s.overlay = Some(crate::state::Overlay::CanvasCreate);
        assert!(!destination.current(&s));
        s.overlay = None;
        s.shortcut_generation += 1;
        assert!(!destination.current(&s));
        s.shortcut_generation -= 1;
        assert!(destination.current(&s));
        s.thread_generation += 1;
        assert!(!destination.current(&s));
        s.thread_generation -= 1;
        s.channel_generation += 1;
        assert!(!destination.current(&s));
        s.channel_generation -= 1;
        s.later_open_generation += 1;
        assert!(!destination.current(&s));
        s.later_open_generation -= 1;
        s.main_view = MainView::Home;
        assert!(!destination.current(&s));
    }

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
