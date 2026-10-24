//! Re-drive data for the current visit without turning reconnect into navigation.
use crate::state::{ChannelTab, MainView, ShellState};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::slack::Transport;

pub(crate) mod check;
pub(crate) mod fixture;
pub(super) mod history_gate;

#[derive(Clone)]
pub(super) struct Visit {
    account: Option<String>,
    epoch: u64,
    team: Option<String>,
    transport: Option<Arc<Transport>>,
    view: MainView,
    channel: Option<String>,
    channel_generation: u64,
    root: Option<String>,
    thread_generation: u64,
    shortcut: u64,
    request: u64,
    overlay: Option<crate::state::Overlay>,
    tab: ChannelTab,
}
impl Visit {
    fn begin(s: &mut ShellState) -> Self {
        s.reconnect_generation = s.reconnect_generation.wrapping_add(1);
        // Retire the old serial's busy flags synchronously. Cached content,
        // paging bounds, error, target and the thread visit remain untouched.
        s.thread_loading = false;
        s.thread_history.loading = None;
        Self::capture(s)
    }
    fn capture(s: &ShellState) -> Self {
        Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone(),
            transport: s.core.transport.clone(),
            view: s.main_view,
            channel: s.core.active_channel.clone(),
            channel_generation: s.channel_generation,
            root: s.thread_root.clone(),
            thread_generation: s.thread_generation,
            shortcut: s.shortcut_generation,
            request: s.reconnect_generation,
            overlay: s.overlay.clone(),
            tab: s.channel_tab,
        }
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.account == s.core.active_account
            && self.epoch == s.core.account_epoch
            && self.team == s.core.active_team
            && self.view == s.main_view
            && self.channel == s.core.active_channel
            && self.channel_generation == s.channel_generation
            && self.root == s.thread_root
            && self.thread_generation == s.thread_generation
            && self.request == s.reconnect_generation
            && self.shortcut == s.shortcut_generation
            && self.overlay == s.overlay
            && self.tab == s.channel_tab
            && match (&self.transport, &s.core.transport) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}

pub async fn reload_after_outage(mut state: Signal<ShellState>) {
    let visit = {
        let mut s = state.write();
        Visit::begin(&mut s)
    };
    let run = fixture::begin(&state.peek());
    crate::slack_lists::invalidate_visible_for_reconnect(&mut state.write());
    crate::slack_lists::refresh(state);
    if state.peek().detail_open() && visit.tab != ChannelTab::Messages {
        crate::channel_resources::refresh(state).await;
        return;
    }
    super::discovery::refresh_main_view(state, visit.view, Some(&visit)).await;
    let phase = fixture::boundary(state, run, "surface").await;
    let current = visit.current(&state.peek());
    fixture::finish_boundary(phase, &state.peek(), current);
    if !current {
        return;
    }
    super::history::refresh_selected_channel_owned(state, &visit).await;
    let phase = fixture::boundary(state, run, "history").await;
    let current = visit.current(&state.peek());
    fixture::finish_boundary(phase, &state.peek(), current);
    if !current {
        return;
    }
    if let Some((root, channel)) = visit.root.clone().zip(visit.channel.clone()) {
        // A pointer/wheel action may have consumed the original target while
        // data was refreshing. Its current value, including None, owns this load.
        let (target, scope) = {
            let s = state.peek();
            if !visit.current(&s) {
                return;
            }
            (s.thread_target.clone(), s.thread_scope())
        };
        super::thread::load_started_thread_owned(state, channel, root, target, scope, Some(visit))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retiring_old_read_releases_busy_state_without_resetting_thread() {
        use super_platinum_core::thread_history::{Direction, Page};
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.begin_thread("1.0".into(), Some("1.1".into()));
        s.thread_error = Some("cached failure".into());
        s.thread_history.earlier = Some(Page {
            latest: Some("1.1".into()),
            ..Default::default()
        });
        s.thread_history.newer = Some(Page {
            oldest: Some("1.1".into()),
            ..Default::default()
        });
        s.thread_history.loading = Some(Direction::Earlier);
        s.thread_history.failed = Some(Direction::Newer);
        let before = Visit::capture(&s);
        let earlier = s.thread_history.earlier.clone();
        let newer = s.thread_history.newer.clone();
        let key = ("T1".to_owned(), "C2".to_owned(), "1.0".to_owned());
        s.core.threads.insert(
            key.clone(),
            super_platinum_core::state::ChannelMessages {
                messages: vec![super_platinum_core::slack::models::Message {
                    ts: Some("1.1".into()),
                    text: Some("cached body".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let input = s.thread_window.input_generation;
        let current = Visit::begin(&mut s);
        assert!(!before.current(&s));
        assert!(current.current(&s));
        assert!(!s.thread_loading);
        assert!(s.thread_history.loading.is_none());
        assert_eq!(s.thread_history.earlier, earlier);
        assert_eq!(s.thread_history.newer, newer);
        assert_eq!(s.thread_history.failed, Some(Direction::Newer));
        assert_eq!(s.thread_root.as_deref(), Some("1.0"));
        assert_eq!(s.thread_target.as_deref(), Some("1.1"));
        assert_eq!(s.thread_error.as_deref(), Some("cached failure"));
        assert_eq!(s.thread_generation, before.thread_generation);
        assert_eq!(
            s.core.threads[&key].messages[0].text.as_deref(),
            Some("cached body")
        );
        assert_eq!(s.thread_window.input_generation, input);
    }
    #[test]
    fn reconnect_preserves_consumed_target_but_rejects_replaced_owners() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        s.begin_thread("1.0".into(), Some("1.1".into()));
        let v = Visit::capture(&s);
        s.thread_target = None;
        assert!(v.current(&s));
        s.reconnect_generation += 1;
        assert!(!v.current(&s));
        s.reconnect_generation -= 1;
        s.thread_generation += 1;
        assert!(!v.current(&s));
        s.thread_generation -= 1;
        s.channel_generation += 1;
        assert!(!v.current(&s));
        s.channel_generation -= 1;
        s.shortcut_generation += 1;
        assert!(!v.current(&s));
        s.shortcut_generation -= 1;
        s.core.account_epoch += 1;
        assert!(!v.current(&s));
        s.core.account_epoch -= 1;
        s.channel_tab = ChannelTab::Files;
        assert!(!v.current(&s));
        s.channel_tab = v.tab;
        s.overlay = Some(crate::state::Overlay::CanvasCreate);
        assert!(!v.current(&s));
        s.overlay = v.overlay.clone();
        s.main_view = MainView::Later;
        assert!(!v.current(&s));
        s.main_view = v.view;
        assert!(v.current(&s));
        s.close_thread();
        assert!(!v.current(&s));
    }
}
