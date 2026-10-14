//! Native attention checks shared by realtime counting and automatic read marks.

use dioxus::prelude::*;

use crate::state::ShellState;

impl ShellState {
    pub(crate) fn reading_latest(&self, focused: bool) -> bool {
        crate::read_preferences::renderer::can_read(self, focused, false)
    }

    pub(crate) fn reading_thread(&self, focused: bool, channel: &str, root: &str) -> bool {
        focused
            && !crate::read_shortcuts::busy_thread(self)
            && !crate::read_shortcuts::bulk::blocks_reads(self)
            && self.detail_open()
            && self.core.active_channel.as_deref() == Some(channel)
            && self.thread_root.as_deref() == Some(root)
            && self.thread_at_bottom
            && self.thread_target.is_none()
            && !self.thread_loading
            && self.thread_history.newer.is_none()
            && !crate::thread_unread::blocks_auto_read(self)
    }
}

pub(crate) fn focused() -> bool {
    dioxus::desktop::window().window.is_focused()
}

pub(crate) fn use_focus_read_marks(state: Signal<ShellState>) {
    let window_id = dioxus::desktop::window().window.id();
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if matches!(event, dioxus::desktop::tao::event::Event::WindowEvent {
            window_id: id, event: dioxus::desktop::WindowEvent::Focused(true), ..
        } if *id == window_id)
        {
            spawn(async move {
                // The event handler can precede the platform's focus flag.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                crate::bootstrap::mark_visible_read(state).await;
                let thread = {
                    let shell = state.peek();
                    shell
                        .core
                        .active_channel
                        .clone()
                        .zip(shell.thread_root.clone())
                };
                if let Some((channel, root)) = thread {
                    crate::bootstrap::mark_thread_read(state, channel, root).await;
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        media::MediaRegistry,
        state::{ChannelTab, MainView},
    };

    #[test]
    fn incoming_messages_require_focus_and_a_settled_latest_transcript() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        shell.core.active_account = Some("isolated-read-test".into());
        shell.core.pending_scroll_to = None;
        shell.stick_to_bottom = true;
        let team = shell.core.active_team.clone().unwrap();
        let channel = shell.core.active_channel.clone().unwrap();
        let last_read = shell.core.workspaces[&team].messages[&channel]
            .last_read
            .clone()
            .unwrap();
        shell.unread_anchor = Some((channel, last_read));
        crate::read_preferences::renderer::record_fixture_observation(&mut shell, true, true);
        assert!(shell.reading_latest(true));
        assert!(!shell.reading_latest(false));
        shell.stick_to_bottom = false;
        crate::read_preferences::renderer::record_fixture_observation(&mut shell, false, false);
        assert!(!shell.reading_latest(true));
        shell.stick_to_bottom = true;
        crate::read_preferences::renderer::record_fixture_observation(&mut shell, true, true);
        shell.core.pending_scroll_to = Some((
            "C1".into(),
            super_platinum_core::domain::PendingScrollTarget::Latest,
        ));
        assert!(!shell.reading_latest(true));
        shell.core.pending_scroll_to = None;
        shell.channel_tab = ChannelTab::Files;
        assert!(!shell.reading_latest(true));
        shell.channel_tab = ChannelTab::Messages;
        shell.main_view = MainView::Scheduled;
        assert!(!shell.reading_latest(true));
    }

    #[test]
    fn thread_marks_require_the_current_loaded_latest_thread_and_focus() {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let channel = shell.core.active_channel.clone().unwrap();
        shell.thread_root = Some("root".into());
        shell.thread_loading = false;
        shell.thread_target = None;
        shell.thread_at_bottom = true;
        assert!(shell.reading_thread(true, &channel, "root"));
        assert!(!shell.reading_thread(false, &channel, "root"));
        assert!(!shell.reading_thread(true, "other", "root"));
        assert!(!shell.reading_thread(true, &channel, "old-root"));
        shell.thread_at_bottom = false;
        assert!(!shell.reading_thread(true, &channel, "root"));
        shell.thread_at_bottom = true;
        shell.thread_target = Some("older-reply".into());
        assert!(!shell.reading_thread(true, &channel, "root"));
        shell.thread_target = None;
        shell.thread_loading = true;
        assert!(!shell.reading_thread(true, &channel, "root"));
        shell.thread_loading = false;
        shell.thread_history.newer = Some(Default::default());
        assert!(
            !shell.reading_thread(true, &channel, "root"),
            "a cached tail beyond a history gap was not read"
        );
    }
}
